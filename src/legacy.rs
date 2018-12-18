//! Just a hiding place for the code that made up the old
//! init system. It's still here mostly because service files
//! for most packages don't exist yet.

use std::env;
use std::fs::{File, read_dir};
use std::io::{Read, Result};
use std::path::Path;
use std::process::Command;

use switch_stdio;

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
                                        let path = entry.path();
                                        // Ignore .toml service files
                                        if let None = path.extension() {
                                            entries.push(path);
                                        }
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
