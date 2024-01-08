use std::collections::BTreeMap;
use std::env;
use std::fs::{read_dir, File};
use std::io::{BufRead, BufReader, Error, Result};
use std::path::Path;
use std::process::Command;

use libredox::flag::{O_RDONLY, O_WRONLY};

fn switch_stdio(stdio: &str) -> Result<()> {
    let stdin = libredox::Fd::open(stdio, O_RDONLY, 0)
        .map_err(|err| Error::from_raw_os_error(err.errno))?;
    let stdout = libredox::Fd::open(stdio, O_WRONLY, 0)
        .map_err(|err| Error::from_raw_os_error(err.errno))?;
    let stderr = libredox::Fd::open(stdio, O_WRONLY, 0)
        .map_err(|err| Error::from_raw_os_error(err.errno))?;

    stdin
        .dup2(0, &[])
        .map_err(|err| Error::from_raw_os_error(err.errno))?;
    stdout
        .dup2(1, &[])
        .map_err(|err| Error::from_raw_os_error(err.errno))?;
    stderr
        .dup2(2, &[])
        .map_err(|err| Error::from_raw_os_error(err.errno))?;

    Ok(())
}

pub fn run(file: &Path) -> Result<()> {
    let file = File::open(file)?;
    let reader = BufReader::new(file);
    for line_res in reader.lines() {
        let line_raw = line_res?;
        let line = line_raw.trim();
        if !line.is_empty() && !line.starts_with('#') {
            let mut args = line.split(' ').map(|arg| {
                if arg.starts_with('$') {
                    env::var(&arg[1..]).unwrap_or(String::new())
                } else {
                    arg.to_string()
                }
            });

            if let Some(cmd) = args.next() {
                match cmd.as_str() {
                    "cd" => {
                        if let Some(dir) = args.next() {
                            if let Err(err) = env::set_current_dir(&dir) {
                                println!("init: failed to cd to '{}': {}", dir, err);
                            }
                        } else {
                            println!("init: failed to cd: no argument");
                        }
                    }
                    "echo" => {
                        if let Some(arg) = args.next() {
                            print!("{}", arg);
                        }
                        for arg in args {
                            print!(" {}", arg);
                        }
                        print!("\n");
                    }
                    "export" => {
                        if let Some(var) = args.next() {
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
                        }
                    }
                    "run" => {
                        if let Some(new_file) = args.next() {
                            if let Err(err) = run(&Path::new(&new_file)) {
                                println!("init: failed to run '{}': {}", new_file, err);
                            }
                        } else {
                            println!("init: failed to run: no argument");
                        }
                    }
                    "run.d" => {
                        // This must be a BTreeMap to iterate in sorted order.
                        let mut entries = BTreeMap::new();
                        let mut missing_arg = true;

                        for new_dir in args {
                            if !Path::new(&new_dir).exists() {
                                // Skip non-existent dirs
                                continue;
                            }
                            missing_arg = false;

                            match read_dir(&new_dir) {
                                Ok(list) => {
                                    for entry_res in list {
                                        match entry_res {
                                            Ok(entry) => {
                                                // This intentionally overwrites older entries with
                                                // the same filename to allow overriding entries in
                                                // one search dir with those in a later search dir.
                                                entries.insert(entry.file_name(), entry.path());
                                            }
                                            Err(err) => {
                                                println!(
                                                    "init: failed to run.d: '{}': {}",
                                                    new_dir, err
                                                );
                                            }
                                        }
                                    }
                                }
                                Err(err) => {
                                    println!("init: failed to run.d: '{}': {}", new_dir, err);
                                }
                            }
                        }

                        if missing_arg {
                            println!("init: failed to run.d: no argument or all dirs are non-existent");
                        } else {
                            std::env::set_var("DISPLAY", "3");
                            // This takes advantage of BTreeMap iterating in sorted order.
                            for (_, entry_path) in entries {
                                if let Err(err) = run(&entry_path) {
                                    println!(
                                        "init: failed to run '{}': {}",
                                        entry_path.display(),
                                        err
                                    );
                                }
                            }
                        }
                    }
                    "stdio" => {
                        if let Some(stdio) = args.next() {
                            if let Err(err) = switch_stdio(&stdio) {
                                println!("init: failed to switch stdio to '{}': {}", stdio, err);
                            }
                        } else {
                            println!("init: failed to set stdio: no argument");
                        }
                    }
                    _ => {
                        let mut command = Command::new(cmd);
                        for arg in args {
                            command.arg(arg);
                        }

                        match command.spawn() {
                            Ok(mut child) => match child.wait() {
                                Ok(_status) => (), //println!("init: waited for {}: {:?}", line, status.code()),
                                Err(err) => {
                                    println!("init: failed to wait for '{}': {}", line, err)
                                }
                            },
                            Err(err) => println!("init: failed to execute '{}': {}", line, err),
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

    libredox::call::setrens(0, 0).expect("init: failed to enter null namespace");

    loop {
        let mut status = 0;
        libredox::call::waitpid(0, &mut status, 0).unwrap();
    }
}
