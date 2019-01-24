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

use crate::service::Service;
use crate::service_tree::ServiceGraph;

const INITFS_SERVICE_DIR: &str = "initfs:/etc/init.d";
const FS_SERVICE_DIR: &str = "file:/etc/init.d";

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
        let initfs_services = Service::from_dir(INITFS_SERVICE_DIR)
            .unwrap_or_else(|err| {
                error!("error parsing service directory '{}': {}", INITFS_SERVICE_DIR, err);
                vec![]
            });
        
        let mut service_graph = ServiceGraph::new();
        service_graph.push_services(initfs_services);
        service_graph.start_services();
        
        //*
        crate::switch_stdio("display:1")
            .unwrap_or_else(|err| {
                error!("error switching stdio: {}", err);
            });
        // */
        
        let fs_services = Service::from_dir(FS_SERVICE_DIR)
            .unwrap_or_else(|err| {
                error!("error parsing service directory '{}': {}", FS_SERVICE_DIR, err);
                vec![]
            });
        
        service_graph.push_services(fs_services);
        service_graph.start_services();
    }
    
    // Might should not do this
    syscall::setrens(0, 0).expect("init: failed to enter null namespace");
    
    loop {
        let mut status = 0;
        syscall::waitpid(0, &mut status, 0).unwrap();
    }
}
