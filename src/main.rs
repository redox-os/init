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
mod service;

use std::env;
use std::fs::{File, read_dir};
use std::io::{Read, Error, Result};
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::path::Path;
use std::process::Command;

use syscall::flag::{O_RDONLY, O_WRONLY};

use dependency::DepGraph;
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

pub fn run(file: &Path) -> Result<()> {
    let mut data = String::new();
    File::open(file)?.read_to_string(&mut data)?;

    for line in data.lines() {
        let line = line.trim();
        if ! line.is_empty() && ! line.starts_with('#') {
            let mut args = line.split(' ').map(|arg| if arg.starts_with('$') {
                env::var(&arg[1..]).unwrap_or(String::new())
            } else {
                arg.to_string()
            });

            if let Some(cmd) = args.next() {
                match cmd.as_str() {
                    "cd" => if let Some(dir) = args.next() {
                        if let Err(err) = env::set_current_dir(&dir) {
                            println!("init: failed to cd to '{}': {}", dir, err);
                        }
                    } else {
                        println!("init: failed to cd: no argument");
                    },
                    "echo" => {
                        if let Some(arg) = args.next() {
                            print!("{}", arg);
                        }
                        for arg in args {
                            print!(" {}", arg);
                        }
                        print!("\n");
                    },
                    "export" => if let Some(var) = args.next() {
                        let mut value = String::new();
                        if let Some(arg) = args.next() {
                            value.push_str(&arg);
                        }
                        for arg in args {
                            value.push(' ');
                            value.push_str(&arg);
                        }
                        env::set_var(var, value);
                    } else {
                        println!("init: failed to export: no argument");
                    },
                    "run" => if let Some(new_file) = args.next() {
                        if let Err(err) = run(&Path::new(&new_file)) {
                            println!("init: failed to run '{}': {}", new_file, err);
                        }
                    } else {
                        println!("init: failed to run: no argument");
                    },
                    "run.d" => if let Some(new_dir) = args.next() {
                        let mut entries = vec![];
                        match read_dir(&new_dir) {
                            Ok(list) => for entry_res in list {
                                match entry_res {
                                    Ok(entry) => {
                                        entries.push(entry.path());
                                    },
                                    Err(err) => {
                                        println!("init: failed to run.d: '{}': {}", new_dir, err);
                                    }
                                }
                            },
                            Err(err) => {
                                println!("init: failed to run.d: '{}': {}", new_dir, err);
                            }
                        }

                        entries.sort();

                        for entry in entries {
                            if let Err(err) = run(&entry) {
                                println!("init: failed to run '{}': {}", entry.display(), err);
                            }
                        }
                    } else {
                        println!("init: failed to run.d: no argument");
                    },
                    "stdio" => if let Some(stdio) = args.next() {
                        if let Err(err) = switch_stdio(&stdio) {
                            println!("init: failed to switch stdio to '{}': {}", stdio, err);
                        }
                    } else {
                        println!("init: failed to set stdio: no argument");
                    },
                    _ => {
                        let mut command = Command::new(cmd);
                        for arg in args {
                            command.arg(arg);
                        }

                        match command.spawn() {
                            Ok(mut child) => match child.wait() {
                                Ok(_status) => (), //println!("init: waited for {}: {:?}", line, status.code()),
                                Err(err) => println!("init: failed to wait for '{}': {}", line, err)
                            },
                            Err(err) => println!("init: failed to execute '{}': {}", line, err)
                        }
                    }
                }
            }
        }
    }

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
    
    let services = DepGraph::from_services(services)
        .on_provided("file:", || {
            info!("setting cwd to file:");
            if let Err(err) = env::set_current_dir("file:") {
                error!("failed to set cwd: {}", err);
            }
            
            info!("setting PATH=file:/bin");
            env::set_var("PATH", "file:/bin");
            
            // This file has had the services removed now
            if let Err(err) = run(&Path::new("initfs:etc/init.rc")) {
                error!("failed to run initfs:etc/init.rc: {}", err);
            }
        })
        .on_provided("display:", || {
            switch_stdio("display:1")
                .unwrap_or_else(|err| {
                    warn!("{}", err);
                });
        });
    
    info!("setting PATH=initfs:/bin");
    env::set_var("PATH", "initfs:/bin");
    
    services.start().expect("failed to start services");
    
    syscall::setrens(0, 0).expect("init: failed to enter null namespace");
    
    loop {
        let mut status = 0;
        syscall::waitpid(0, &mut status, 0).unwrap();
    }
}
