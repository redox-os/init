extern crate syscall;

use std::env;
use std::fs::{File, read_dir};
use std::io::{BufReader, BufRead, Error, Result};
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
use std::path::Path;
use std::process::Command;
use syscall::flag::{WaitFlags, O_RDONLY, O_WRONLY};

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

pub fn run(file: &Path) -> Result<()> {
    let file = File::open(file)?;
    let reader = BufReader::new(file);
    for line_res in reader.lines() {
        let line_raw = line_res?;
        let line = line_raw.trim();
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
                        // On startup, the VESA display driver is started which basically makes use of the framebuffer
                        // provided by the firmware. The GPU device are latter started by `pcid` (such as `virtio-gpu`).
                        let mut devices = vec![];
                        let schemes = std::fs::read_dir(":").unwrap();
                    
                        for entry in schemes {
                            let path = entry.unwrap().path();
                            let path_str = path
                                .into_os_string()
                                .into_string()
                                .expect("init: failed to convert path to string");
                    
                            if path_str.contains("display") {
                                println!("init: found display scheme {}", path_str);
                                devices.push(path_str);
                            }
                        }
                    
                        let device = devices.iter().filter(|d| !d.contains("vesa")).collect::<Vec<_>>();
                        let device = if device.is_empty() {
                            // No GPU available, fallback to VESA display which *should* always be accessible.
                           "vesa"
                        } else {
                            // :/display/virtio-gpu
                            //           ^^^^^^^^^^
                            device[0].split("/").nth(2).unwrap()
                        };

                        std::env::set_var("DISPLAY", &format!("display/{}:3/activate", device));

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
    if let Err(err) = run(&Path::new("initfs:etc/init.rc")) {
        println!("init: failed to run initfs:etc/init.rc: {}", err);
    }

    syscall::setrens(0, 0).expect("init: failed to enter null namespace");

    loop {
        let mut status = 0;
        syscall::waitpid(0, &mut status, WaitFlags::empty()).unwrap();
    }
}
