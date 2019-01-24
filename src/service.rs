use std::collections::HashMap;
use std::default::Default;
use std::env;
use std::ffi::OsStr;
use std::fs::{File, read_dir};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use failure::{err_msg, Error};
use log::{debug, error, info, trace};
use serde_derive::Deserialize;
use toml;

use crate::PathExt;
use self::ServiceState::*;

#[derive(Clone, Copy, Debug)]
pub enum ServiceState {
    Offline,
    Online,
    Failed
}

impl ServiceState {
    pub fn is_online(&self) -> bool {
        match self {
            Offline => false,
            Online => true,
            Failed => false
        }
    }
}

impl Default for ServiceState {
    fn default() -> ServiceState { ServiceState::Offline }
}

#[derive(Debug, Deserialize)]
pub struct Method {
    pub cmd: Vec<String>
}

impl Method {
    /// Replace any arguments in `cmd` that are environment variables
    /// with the value stored in that environment variable
    ///
    /// The `$` must be the first character in the argument (other than
    /// whitespace, that should be changed)
    //TODO: Allow env-var args to be only partially env vars
    // Eg: allow `--target=$MY_VAR`
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
    
    pub fn wait(&self, vars: &Option<HashMap<String, String>>, cwd: &Option<impl AsRef<Path>>) -> Result<(), Error> {
        let mut cmd = Command::new(&self.cmd[0]);
        cmd.args(self.cmd[1..].iter())
            .env_clear();
        
        if let Some(vars) = vars {
            // Typechecker hell if you try Command::envs
            //   This is literally the same
            for (var, val) in vars.iter() {
                cmd.env(var, val);
            }
        }
        
        // Is inheriting cwd from `init` OK? Should it use the root of
        //   the filesystem the service was parsed from?
        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        }
        
        debug!("waiting on {:?}", cmd);
        
        cmd.spawn()?
            .wait()?;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
pub struct Service {
    #[serde(skip)]
    pub name: String,
    
    pub dependencies: Option<Vec<String>>,
    pub provides: Option<Vec<String>>,
    pub methods: HashMap<String, Method>,
    
    pub vars: Option<HashMap<String, String>>,
    pub cwd: Option<PathBuf>,
}

impl Service {
    /// Parse a service file, no specific requirements for filetype.
    pub fn from_file(file_path: impl AsRef<Path>) -> Result<Service, Error> {
        let file_path = file_path.as_ref();
        
        let mut data = String::new();
        File::open(&file_path)?
            .read_to_string(&mut data)?;
        
        let mut service = toml::from_str::<Service>(&data)?;
        
        //BUG: Only removes the portion after the last '.'
        service.name = file_path.file_stem()
            .ok_or(err_msg("service file path missing filename"))?
            .to_string_lossy() // Redox uses unicode, this should never fail
            .into();
        service.sub_env();
        
        // Assume that the scheme this service came from is the one
        //   that the service should be started in
        if let None = service.cwd {
            if let Some(scheme) = file_path.scheme() {
                service.cwd = Some(scheme);
            }
        }
        Ok(service)
    }
    
    /// Parse all the toml files in a directory as services
    pub fn from_dir(dir: impl AsRef<Path>) -> Result<Vec<Service>, Error> {
        trace!("parsing services from '{:#?}'", dir.as_ref());
        
        let mut services = vec![];
        
        for file in read_dir(&dir)? {
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
                    Err(err) => error!("error parsing service file '{:#?}': {}", dir.as_ref(), err)
                }
            }
        }
        Ok(services)
    }
    
    /// Substitue all fields which support environment variable
    /// substitution
    fn sub_env(&mut self) {
        for method in self.methods.values_mut() {
            method.sub_env();
        }
    }
    
    /// Spawn the process indicated by a method on this service and `wait()` on it.
    pub fn wait_method(&self, method_name: &String) -> Result<(), Error> {
        match self.methods.get(method_name) {
            Some(method) => {
                info!("running method '{}' for service '{}'", method_name, self.name);
                
                method.wait(&self.vars, &self.cwd)?;
                Ok(())
            },
            None => {
                let msg = format!("service '{}' missing method '{}'", self.name, method_name);
                Err(err_msg(msg))
            }
        }
    }
}
