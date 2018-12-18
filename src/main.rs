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

mod dependency;
mod dependency_graph;
mod legacy;
mod service;

use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
use std::io::{Error, Result};
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::path::Path;

use syscall::flag::{O_RDONLY, O_WRONLY};

use dependency::{graph_from_services, start_services};
use service::services;

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
        if let Err(err) = legacy::run(&Path::new("initfs:etc/init.rc")) {
            error!("failed to run initfs:etc/init.rc: {}", err);
        }
    } else {
        let service_list = services("initfs:/etc/init.d")
            .unwrap_or_else(|err| {
                warn!("{}", err);
                vec![]
            });
        
        let service_graph = graph_from_services(service_list);
        let mut provide_hooks = HashMap::with_capacity(2);
        
        provide_hooks.insert("file:".into(), || {
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
                
                dependency::graph_add_services(&mut service_graph, fs_services);
                start_services(service_graph, HashMap::new());
            });
        provide_hooks.insert("display:".into(), || {
                switch_stdio("display:1")
                    .unwrap_or_else(|err| {
                        warn!("{}", err);
                    });
            });
        
        info!("setting PATH=initfs:/bin");
        env::set_var("PATH", "initfs:/bin");
        
        start_services(service_graph, provide_hooks);
    }
    
    syscall::setrens(0, 0).expect("init: failed to enter null namespace");
    
    loop {
        let mut status = 0;
        syscall::waitpid(0, &mut status, 0).unwrap();
    }
}
