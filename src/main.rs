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
use std::fs::File;
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
    
    let services = services("initfs:/etc/init.d")
        .unwrap_or_else(|err| {
            warn!("{}", err);
            vec![]
        });
    
    let services = graph_from_services(services);
    let mut provide_hooks: HashMap<_, fn()> = HashMap::with_capacity(2);
    provide_hooks.insert("file:".into(), || {
            info!("setting cwd to file:");
            if let Err(err) = env::set_current_dir("file:") {
                error!("failed to set cwd: {}", err);
            }
            
            info!("setting PATH=file:/bin");
            env::set_var("PATH", "file:/bin");
            
            // This file has had the services removed now
            if let Err(err) = legacy::run(&Path::new("initfs:etc/init.rc")) {
                error!("failed to run initfs:etc/init.rc: {}", err);
            }
        });
    provide_hooks.insert("display:".into(), || {
            switch_stdio("display:1")
                .unwrap_or_else(|err| {
                    warn!("{}", err);
                });
        });
    
    info!("setting PATH=initfs:/bin");
    env::set_var("PATH", "initfs:/bin");
    
    start_services(services, provide_hooks).expect("failed to start services");
    
    syscall::setrens(0, 0).expect("init: failed to enter null namespace");
    
    loop {
        let mut status = 0;
        syscall::waitpid(0, &mut status, 0).unwrap();
    }
}
