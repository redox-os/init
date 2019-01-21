//#![deny(warnings)]

mod dep_graph;
mod legacy;
mod service;
mod service_tree;

use std::fs::{self, File};
use std::io::{Error, Result};
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::path::{Path, PathBuf};

use crossbeam::channel;
use generational_arena::Index;
use log::{error, warn};
use syscall::flag::{O_RDONLY, O_WRONLY};

use crate::service::{services, State};
use crate::service_tree::ServiceTree;

fn switch_stdio(stdio: &str) -> Result<()> {
    let stdin = unsafe { File::from_raw_fd(
        syscall::open(stdio, O_RDONLY).map_err(|err| Error::from_raw_os_error(err.errno))?
    ) };
    let stdout = unsafe { File::from_raw_fd(
        syscall::open(stdio, O_WRONLY).map_err(|err| Error::from_raw_os_error(err.errno))?
    ) };
    let stderr = unsafe { File::from_raw_fd(
        syscall::open(stdio, O_WRONLY).map_err(|err| Error::from_raw_os_error(err.errno))?
    ) };
    
    syscall::dup2(stdin.as_raw_fd(), 0, &[]).map_err(|err| Error::from_raw_os_error(err.errno))?;
    syscall::dup2(stdout.as_raw_fd(), 1, &[]).map_err(|err| Error::from_raw_os_error(err.errno))?;
    syscall::dup2(stderr.as_raw_fd(), 2, &[]).map_err(|err| Error::from_raw_os_error(err.errno))?;
    
    Ok(())
}

trait SchemeElement {
    fn scheme(&self) -> Option<&Path>;
}

impl SchemeElement for Path {
    fn scheme(&self) -> Option<&Path> {
        let last = self.ancestors()
            .filter(|element| element != &Path::new("") )
            .last();
        // lossy is fine 'cause Redox
        if String::from(last?.to_string_lossy()).pop()? == ':' {
            Some(last?)
        } else {
            None
        }
    }
}

pub enum Event {
    RegisterServices(PathBuf),
    // A service should be started if it's dependencies are satisfied
    ServiceStart(Index),
    // A service has changed state
    ServiceChangedState(Index, State)
}

pub fn main() {
    simple_logger::init()
        .unwrap_or_else(|err| {
            println!("init: failed to start logger: {}", err);
        });
    
    // This way we can continue to support old systems that still have init.rc
    if let Ok(_) = fs::metadata("initfs:/etc/init.rc") {
        if let Err(err) = legacy::run(&Path::new("initfs:/etc/init.rc")) {
            error!("failed to run initfs:/etc/init.rc: {}", err);
        }
    } else {
        let (transmitter, receiver) = channel::unbounded();
        let mut service_graph = ServiceTree::new(transmitter.clone());
        
        transmitter.send(Event::RegisterServices(PathBuf::from("initfs:/etc/init.d")))
            .expect("failed to send initial event"); // Shouldn't be possible
        
        //crossbeam::scope(|scope| {
            loop {
                match receiver.recv() {
                    Ok(event) => match event {
                        Event::RegisterServices(path) => {
                            /* Not dealing with non-canonical paths, 'cause there are
                             * better things to waste time on right now.
                             *
                            //TODO: This behavior should change and be more robust,
                            //  but it will do for now
                            if let Some(scheme) = path.scheme() {
                                let mut bin_dir = scheme.to_path_buf();
                                bin_dir.push("/bin");
                                
                                if let Ok(_) = fs::metadata(&bin_dir) {
                                    /// Lossy is fine because it's redox, and it's a log...
                                    info!("setting PATH={}", bin_dir.to_string_lossy());
                                    env::set_var("PATH", bin_dir);
                                }
                            }
                            info!("PATH={:?}", env::var("PATH"));
                            */
                            
                            let service_list = services(&path)
                                .unwrap_or_else(|err| {
                                    warn!("{}", err);
                                    vec![]
                                });
                            service_graph.push_services(service_list);
                        },
                        Event::ServiceStart(index) => {
                            //scope.spawn(|_| {
                                service_graph.start_service(index);
                            //});
                        }
                        Event::ServiceChangedState(index, state) => {
                            service_graph.set_service_state(index, state);
                        }
                    },
                    Err(err) => error!("error recieving event: {}", err)
                }
            }
        //});
    }
    /*
    loop {
        let mut status = 0;
        syscall::waitpid(0, &mut status, 0).unwrap();
    }*/
}
