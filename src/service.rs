use std::collections::HashMap;
use std::default::Default;
use std::env;
use std::ffi::OsStr;
use std::fs::{File, read_dir};
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use failure::{err_msg, Error};
use log::{debug, error, info, trace};
use redox_users::{AllGroups, AllUsers};
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
    /// The command that is executed when this method is "called".
    pub cmd: Vec<String>,
    
    /// Environment variables to set for the process executed by
    /// this method. Overrides service-level environment variables,
    /// meaning service-level vars are not set.
    pub vars: Option<HashMap<String, String>>,
    /// The current working directory for the process executed
    /// by this method. Overrides service-level cwd.
    pub cwd: Option<PathBuf>,
    
    /// Username to run this method's process as. Overrides
    /// service-level username. Must be present in order to use
    /// `group`. Defaults to `root`.
    pub user: Option<String>,
    /// Group name to run this method's process as. Overrides
    /// service-level username. If `user` is given and `group`
    /// is not, `user`'s primary group id is used. Defaults to
    /// `root`.
    pub group: Option<String>,
}

impl Method {
    /// Replace any arguments in `cmd` that are environment variables
    /// with the value stored in that environment variable
    ///
    /// The `$` must be the first character in the argument (other than
    /// whitespace)
    //TODO: Allow env-var args to be only partially env vars
    // Eg: allow `--target=$MY_VAR`
    fn sub_env(&mut self) {
        let modified_cmd = self.cmd.drain(..)
            .map(|arg| if arg.trim().starts_with('$') {
                    let (_, varname) = arg.split_at(1);
                    let val = env::var(&varname).unwrap_or(String::new());
                    trace!("replacing env ${}={}", varname, val);
                    val
                } else {
                    arg
                })
            .collect();
        self.cmd = modified_cmd;
    }
    
    pub fn wait(&self, vars: Option<&HashMap<String, String>>,
        cwd: Option<&PathBuf>,
        user: Option<&String>,
        group: Option<&String>,
    ) -> Result<(), Error> {
        
        let mut cmd = Command::new(&self.cmd[0]);
        cmd.args(self.cmd[1..].iter())
            .env_clear();
        
        //TODO: Some mechanic that allows use of service-level
        //   vars. Is that a good idea?
        if let Some(vars) = self.vars.as_ref().or(vars) {
            // Typechecker hell if you try Command::envs
            //   This is the verbatim impl
            for (var, val) in vars.iter() {
                cmd.env(var, val);
            }
        }
        
        // Is inheriting cwd from `init` OK? Should it use the root of
        //   the filesystem the service was parsed from?
        if let Some(cwd) = self.cwd.as_ref().or(cwd) {
            cmd.current_dir(cwd);
        }
        
        // Same as above goes for user and group
        if let Some(user) = self.user.as_ref().or(user) {
            let users = AllUsers::new(false)?;
            
            if let Some(user) = users.get_by_name(user) {
                //BUG
                cmd.uid(user.uid as u32);
                
                // Once we know the the user exists, then we can check
                //   for group stuff.
                if let Some(group) = self.group.as_ref().or(group) {
                    let groups = AllGroups::new()?;
                    
                    if let Some(group) = groups.get_by_name(group) {
                        //BUG
                        cmd.gid(group.gid as u32);
                    } else {
                        error!("group does not exist: {}", group);
                        cmd.gid(user.gid as u32);
                    }
                } else {
                    cmd.gid(user.gid as u32);
                }
            } else {
                error!("user does not exist: {}", user);
            }
        }
        
        debug!("waiting on {:?}", cmd);
        
        cmd.spawn()?
            .wait()?;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
pub struct Service {
    /// Deduced from the service configuration file name
    #[serde(skip)]
    pub name: String,
    
    /// A dependency is required in order for this service
    /// to be started.
    /// A dependency is a string that can be either the name
    /// of another service or a "provide".
    pub dependencies: Option<Vec<String>>,
    /// A provide can be used to more generally refer to a
    /// system service as a dependency. For example, depending on
    /// `file:` instead of `redoxfs` (which provides `file:`).
    /// This is a very flexible way of defining dependencies.
    pub provides: Option<Vec<String>>,
    /// Pretending that Services are objects in an OOP manner,
    /// they must have methods. A method can be "called" by
    /// init for any number of reasons, or by the user via a
    /// currently WIP cli utility.
    /// A service must provide one method: `start`. This is called
    /// by init in order to start the service. The following
    /// methods are automatically created internally and can be
    /// overridden in a service configuration file:
    ///  - stop
    ///  - restart
    pub methods: HashMap<String, Method>,
    
    /// Environment variables used for all methods that are a
    /// part of this service.
    pub vars: Option<HashMap<String, String>>,
    /// The current working directory for all methods that are
    /// a part of this service/
    pub cwd: Option<PathBuf>,
    
    /// Username to run all service methods as
    pub user: Option<String>,
    /// Groupname to run all service methods as
    pub group: Option<String>,
}

impl Service {
    /// Parse a service file, no specific requirements for filetype.
    pub fn from_file(file_path: impl AsRef<Path>) -> Result<Service, Error> {
        let file_path = file_path.as_ref();
        trace!("parsing service file: {:#?}", file_path);
        
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
        trace!("parsing services from {:#?}", dir.as_ref());
        
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
                
                method.wait(self.vars.as_ref(), self.cwd.as_ref(), self.user.as_ref(), self.group.as_ref())?;
                Ok(())
            },
            None => {
                let msg = format!("service '{}' missing method '{}'", self.name, method_name);
                Err(err_msg(msg))
            }
        }
    }
}
