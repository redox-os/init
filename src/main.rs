//#![deny(warnings)]

extern crate failure;
extern crate generational_arena;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;
extern crate simple_logger;
extern crate syscall;
extern crate toml;

mod dep_graph;
mod legacy;
mod service;
mod service_tree;

use std::env;
use std::fs::{self, File};
use std::io::{Error, Result};
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::path::Path;

use syscall::flag::{O_RDONLY, O_WRONLY};

use service::services;
use service_tree::ServiceTree;

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

pub fn main() {
    simple_logger::init()
        .unwrap_or_else(|err| {
            println!("init: failed to start logger: {}", err);
        });
    
    // This way we can continue to support old systems that still have init.rc
    if let Err(_) = fs::metadata("initfs:/etc/init.rc") {
        if let Err(err) = legacy::run(&Path::new("initfs:/etc/init.rc")) {
            error!("failed to run initfs:/etc/init.rc: {}", err);
        }
    } else {
        let service_list = services("initfs:/etc/init.d")
            .unwrap_or_else(|err| {
                warn!("{}", err);
                vec![]
            });
        
        let mut service_graph = ServiceTree::new();
        //let service_graph2 = service_graph.clone();
        service_graph.push_services(service_list);
        /*
        service_graph.provide_hook("file:".to_string(), Box::new(|service_graph| {
                info!("setting cwd to file:");
                if let Err(err) = env::set_current_dir("file:") {
                    error!("failed to set cwd: {}", err);
                }
                
                info!("setting PATH=file:/bin");
                env::set_var("PATH", "file:/bin");
                
                let fs_services = services("/etc/init.d")
                    .unwrap_or_else(|err| {
                        warn!("{}", err);
                        vec![]
                    });
                
                service_graph.push_services(fs_services);
                service_graph.start_services();
            }));
        service_graph.provide_hook("display:".to_string(), Box::new(|service_graph| {
                switch_stdio("display:1")
                    .unwrap_or_else(|err| {
                        warn!("{}", err);
                    });
            }));*/
        
        info!("setting PATH=initfs:/bin");
        env::set_var("PATH", "initfs:/bin");
        
        service_graph.start_services();
    }
    
    // Might should not do this
    syscall::setrens(0, 0).expect("init: failed to enter null namespace");
    
    loop {
        let mut status = 0;
        syscall::waitpid(0, &mut status, 0).unwrap();
    }
}
