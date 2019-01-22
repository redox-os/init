//#![deny(warnings)]

mod dep_graph;
mod legacy;
mod service;
mod service_tree;

use std::fs::{self, File};
use std::io::{Error, Result};
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::path::Path;

use log::{error, warn};
use syscall::flag::{O_RDONLY, O_WRONLY};

use crate::service::services;
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
        let service_list = services("initfs:/etc/init.d")
            .unwrap_or_else(|err| {
                warn!("{}", err);
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
