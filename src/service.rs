use std::collections::HashMap;
use std::default::Default;
use std::env;
use std::ffi::OsStr;
use std::fs::{File, read_dir};
use std::io::Read;
use std::path::Path;
use std::process::Command;

use failure::Error;
use toml;

#[derive(Debug)]
pub enum State {
    Offline,
    Online,
    Failed
}

impl State {
    pub fn is_running(&self) -> bool {
        match self {
            State::Offline => false,
            State::Online => true,
            State::Failed => false
        }
    }
}

impl Default for State {
    fn default() -> State { State::Offline }
}

#[derive(Debug, Deserialize)]
pub struct Method {
    pub cmd: Vec<String>
}

impl Method {
    /// Replace any arguments that are environment variables
    /// with the value stored in that environment variable
    ///
    /// The `$` must be the first character in the argument
    // (Maybe change that)
    fn sub_env(&mut self) {
        let modified_cmd = self.cmd.drain(..)
            .map(|arg| if arg.trim().starts_with('$') {
                    let (_, varname) = arg.split_at(1);
                    let val = env::var(varname).unwrap_or(String::new());
                    println!("{:?}", val);
                    val
                } else {
                    arg
                })
            .collect();
        self.cmd = modified_cmd;
    }
    
    pub fn wait(&self) {
        let mut cmd = Command::new(&self.cmd[0]);
        cmd.args(self.cmd[1..].iter());
        info!("waiting on service method start: {:?}", cmd);
        
        match cmd.spawn() {
            Ok(mut child) => match child.wait() {
                Ok(_status) => {},
                Err(err) => error!("failed to wait for: {:?}: {}", cmd, err)
            },
            Err(err) => error!("failed to spawn: {:?}: {}", cmd, err)
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Service {
    #[serde(skip)]
    pub name: String,
    
    pub dependencies: Option<Vec<String>>,
    pub provides: Option<Vec<String>>,
    pub methods: HashMap<String, Method>,
    
    #[serde(skip)]
    pub state: State
}

impl Service {
    /// Parse a service file
    pub fn from_file(file_path: impl AsRef<Path>) -> Result<Service, Error> {
        let mut data = String::new();
        File::open(&file_path)?
            .read_to_string(&mut data)?;
        
        let mut service = toml::from_str::<Service>(&data)?;
        
        //BUG: Only removes the portion after the last '.'
        service.name = file_path.as_ref().file_stem()
            .expect("file name empty") // shouldn't be able to happen
            .to_string_lossy() // Redox uses unicode, this should never fail
            .into();
        service.sub_env();
        Ok(service)
    }
    
    /// Substitue all fields which support environment variable
    /// substitution
    fn sub_env(&mut self) {
        for method in self.methods.values_mut() {
            method.sub_env();
        }
    }
}

/// Parse all the toml files in a directory as services
pub fn services(dir: impl AsRef<Path>) -> Result<Vec<Service>, Error> {
    let mut services = vec![];
    
    for file in read_dir(dir)? {
        let file_path = match file {
            Ok(file) => file,
            Err(err) => {
                error!("{}", err);
                continue
            }
        }.path();
        
        let is_toml = match file_path.extension() {
            Some(ext) => ext == OsStr::new("toml"),
            None => false
        };
        
        if is_toml {
            match Service::from_file(file_path) {
                Ok(service) => services.push(service),
                Err(err) => error!("{}", err)
            }
        }
    }
    Ok(services)
}
