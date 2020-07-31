#![deny(warnings)]

extern crate syscall;

use std::fs::{File, read_dir};
use std::io::{Read, Error, Result};
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
use std::path::Path;
use std::{env, process};

use syscall::flag::{O_RDONLY, O_WRONLY};

fn switch_stdio(stdio: &str) -> Result<()> {
    let stdin = unsafe { File::from_raw_fd(
        syscall::open(stdio, O_RDONLY).map_err(|err| Error::from_raw_os_error(err.errno))? as RawFd
    ) };
    let stdout = unsafe { File::from_raw_fd(
        syscall::open(stdio, O_WRONLY).map_err(|err| Error::from_raw_os_error(err.errno))? as RawFd
    ) };
    let stderr = unsafe { File::from_raw_fd(
        syscall::open(stdio, O_WRONLY).map_err(|err| Error::from_raw_os_error(err.errno))? as RawFd
    ) };

    syscall::dup2(stdin.as_raw_fd() as usize, 0, &[]).map_err(|err| Error::from_raw_os_error(err.errno))?;
    syscall::dup2(stdout.as_raw_fd() as usize, 1, &[]).map_err(|err| Error::from_raw_os_error(err.errno))?;
    syscall::dup2(stderr.as_raw_fd() as usize, 2, &[]).map_err(|err| Error::from_raw_os_error(err.errno))?;

    Ok(())
}

pub struct Context {
    children: Vec<process::Child>,
}

pub fn run(file: &Path, context: &mut Context) -> Result<()> {
    let mut data = String::new();
    File::open(file)?.read_to_string(&mut data)?;

    'lines: for line in data.lines() {
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
                        if let Err(err) = run(&Path::new(&new_file), context) {
                            println!("init: failed to run '{}': {}", new_file, err);
                        }
                    } else {
                        println!("init: failed to run: no argument");
                    },
                    "run.d" => if let Some(new_dir) = args.next() {
                        println!("init: doing run.d on dir {}", new_dir);
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
                            if let Err(err) = run(&entry, context) {
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
                    "%NOFORK" => {
                        let cmd = match args.next() {
                            Some(arg) => arg,
                            None => {
                                println!("init: expected command after %NOFORK prefix");
                                continue 'lines;
                            }
                        };
                        // TODO: Use io_uring asynchronous waitpid.

                        let line = line.to_owned();

                        let mut command = process::Command::new(cmd);
                        command.args(args);

                        println!("init: starting secondary thread");
                        match command.spawn() {
                            Ok(child) => context.children.push(child),
                            Err(err) => println!("init: failed to asynchronously execute '{}': {}", line, err),
                        }
                    }
                    _ => {
                        let mut command = process::Command::new(cmd);
                        command.args(args);

                        match command.spawn() {
                            Ok(mut child) => match child.wait() {
                                Ok(status) => println!("init: waited for {}: {}", line, status),
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
    let mut context = Context {
        children: Vec::new(),
    };

    if let Err(err) = run(&Path::new("initfs:etc/init.rc"), &mut context) {
        println!("init: failed to run initfs:etc/init.rc: {}", err);
    }

    syscall::setrens(0, 0).expect("init: failed to enter null namespace");

    loop {
        let mut status = 0;
        match syscall::waitpid(0, &mut status, 0) {
            Ok(_) => (),
            Err(err) => {
                println!("init: error when waiting: {}", err);
                break;
            },
        }
    }

    println!("init: starting to wait for remaining asynchronous commands to complete");
    for mut child in context.children {
        println!("init: waiting for spawned child: {:?}", child);
        match child.wait() {
            Ok(status) => println!("init: waited for spawned child {:?}, status: {}", child, status),
            Err(err) => println!("init: failed to asynchronously wait for {:?}: {}", child, err),
        }
    }
    println!("init: waited");
}
