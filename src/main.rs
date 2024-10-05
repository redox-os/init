use std::collections::BTreeMap;
use std::env;
use std::ffi::CString;
use std::fs::{read_dir, File};
use std::io::{BufRead, BufReader, Result, Write};
use std::path::Path;
use std::process::Command;

use libredox::error::Error as OsError;

use libredox::flag::{O_RDONLY, O_WRONLY};

fn set_default_scheme(scheme: &str) -> std::result::Result<(), OsError> {
    use std::ffi::{c_char, c_int};

    extern "C" {
        fn set_default_scheme(scheme: *const c_char) -> c_int;
    }

    let cstr =
        CString::new(scheme.as_bytes()).expect(&format!("init: invalid default scheme {}", scheme));

    let res = unsafe { set_default_scheme(cstr.as_ptr()) };

    match res {
        0 => Ok(()),
        error_code => Err(OsError::new(error_code)),
    }
}

fn switch_stdio(stdio: &str) -> Result<()> {
    let stdin = libredox::Fd::open(stdio, O_RDONLY, 0)?;
    let stdout = libredox::Fd::open(stdio, O_WRONLY, 0)?;
    let stderr = libredox::Fd::open(stdio, O_WRONLY, 0)?;

    stdin.dup2(0, &[])?;
    stdout.dup2(1, &[])?;
    stderr.dup2(2, &[])?;

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
                    "set-default-scheme" => {
                        if let Some(scheme) = args.next() {
                            if let Err(err) = set_default_scheme(&scheme) {
                                println!(
                                    "init: failed to set default scheme to '{}': {}",
                                    scheme, err
                                );
                            }
                        } else {
                            println!("init: failed to set default scheme: no argument");
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
                            println!(
                                "init: failed to run.d: no argument or all dirs are non-existent"
                            );
                        } else {
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
                    "unset" => {
                        for arg in args {
                            env::remove_var(&arg);
                        }
                    }
                    _ => {
                        let mut command = Command::new(cmd.clone());
                        for arg in args {
                            command.arg(arg);
                        }

                        match command.spawn() {
                            Ok(child) => match child.wait_with_output() {
                                Ok(output) => {
                                    std::io::stdout()
                                        .write_all(output.stdout.as_slice())
                                        .unwrap();
                                    std::io::stderr()
                                        .write_all(output.stderr.as_slice())
                                        .unwrap();
                                    println!("{cmd} done.");
                                }
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
    if let Err(err) = set_default_scheme("initfs") {
        println!("init: failed to set default scheme: {}", err);
    }

    let config = "/etc/init.rc";
    if let Err(err) = run(&Path::new(config)) {
        println!("init: failed to run {}: {}", config, err);
    }

    libredox::call::setrens(0, 0).expect("init: failed to enter null namespace");

    loop {
        let mut status = 0;
        libredox::call::waitpid(0, &mut status, 0).unwrap();
    }
}
