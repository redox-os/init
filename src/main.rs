//#![deny(warnings)]
#![feature(dbg_macro)]

mod dep_graph;
mod legacy;
mod service;
mod service_tree;

use std::fs::{self, File};
use std::io::{Error, Result};
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::path::{Path, PathBuf};

use log::error;
use syscall::flag::{O_RDONLY, O_WRONLY};

use crate::service::services;
use crate::service_tree::ServiceTree;

const INITFS_SERVICE_DIR: &str = "initfs:/etc/init.d";

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

trait PathExt {
    fn scheme(&self) -> Option<PathBuf>;
}

impl PathExt for Path {
    //TODO: Could be better written, gross indexing
    fn scheme(&self) -> Option<PathBuf> {
        /*
        let last = self//.as_ref()
            .ancestors()
            .filter(|element| element != &Path::new("") )
            .last();
        // lossy is fine 'cause Redox
        let last = String::from(last?.to_string_lossy());
        let last_len: usize = last.len();
        
        // Redox returns `file:/` as the last, not `file:`
        if last.get(last_len - 1usize)? == ":" {
            Some(Path::new(&last))
        } else if (last.get(last_len - 1usize)? == "/") && (last.get(last_len - 2usize)? == ":") {
            Some(Path::new(last.get(0usize..last_len - 1usize)?))
        } else {
            None
        }*/
        let path = self.as_os_str()
            .to_string_lossy();
        
        if let Some(indx) = path.find(':') {
            Some(PathBuf::from(&path[..indx + 1]))
        } else {
            None
        }
    }
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
        let service_list = services(INITFS_SERVICE_DIR)
            .unwrap_or_else(|err| {
                error!("error parsing service directory '{}': {}", INITFS_SERVICE_DIR, err);
                vec![]
            });
        
        let mut service_graph = ServiceTree::new();
        service_graph.push_services(service_list);
        service_graph.start_services();
    }
    
    // Might should not do this
    syscall::setrens(0, 0).expect("init: failed to enter null namespace");
    
    loop {
        let mut status = 0;
        syscall::waitpid(0, &mut status, 0).unwrap();
    }
}
