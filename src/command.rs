use std::collections::HashMap;
use std::env;
use std::fmt::{Display, Formatter, self};
use std::fs::File;
use std::io::{Error, Result};
use std::os::unix::io::AsRawFd;

use syscall::{
    self,
    error::Error as SyscallError
};

/// An alternative to `std::process::Command` for
/// just redox, which allows for more flexibility
/// in extending the API.
#[derive(Debug)]
pub struct Command {
    bin: String,
    args: Vec<String>,
    env: HashMap<String, String>,
    clear_env: bool,
    
    cwd: Option<String>,
    uid: Option<usize>,
    gid: Option<usize>,
    ns: Option<Vec<String>>,
}

impl Command {
    pub fn new(bin: String) -> Command {
        Command {
            bin,
            args: vec![],
            env: HashMap::new(),
            clear_env: false,
            
            cwd: None,
            uid: None,
            gid: None,
            ns: None,
        }
    }
    
    pub fn arg(&mut self, arg: String) -> &mut Command {
        self.args.push(arg);
        self
    }
    
    pub fn args(&mut self, mut args: Vec<String>) -> &mut Command {
        self.args.append(&mut args);
        self
    }
    
    pub fn env(&mut self, var: String, val: String) -> &mut Command {
        self.env.insert(var, val);
        self
    }
    
    pub fn env_clear(&mut self) -> &mut Command {
        self.clear_env = true;
        self
    }
    
    pub fn cwd(&mut self, cwd: String) -> &mut Command {
        self.cwd = Some(cwd);
        self
    }
    
    pub fn uid(&mut self, uid: usize) -> &mut Command {
        self.uid = Some(uid);
        self
    }
    
    pub fn gid(&mut self, gid: usize) -> &mut Command {
        self.gid = Some(gid);
        self
    }
    
    pub fn ns(&mut self, ns: Vec<String>) -> &mut Command {
        self.ns = Some(ns);
        self
    }
    
    pub fn spawn(&self) -> Result<()> {
        const CLOEXEC_MSG_FOOTER: &[u8] = b"NOEX";
        
        let bin = File::open(&self.bin)?;
        
        // This is ust copied from the std redox impl
        let pid = unsafe {
             match syscall::clone(0).map_err(|e| Error::from_raw_os_error(e.errno) )? {
                 0 => {
                     let err = self.do_exec(bin);
                     let bytes = [
                         (err.errno >> 24) as u8,
                         (err.errno >> 16) as u8,
                         (err.errno >>  8) as u8,
                         (err.errno >>  0) as u8,
                         CLOEXEC_MSG_FOOTER[0], CLOEXEC_MSG_FOOTER[1],
                         CLOEXEC_MSG_FOOTER[2], CLOEXEC_MSG_FOOTER[3]
                     ];
                     // pipe I/O up to PIPE_BUF bytes should be atomic, and then
                     // we want to be sure we *don't* run at_exit destructors as
                     // we're being torn down regardless
                     //assert!(output.write(&bytes).is_ok());
                     let _ = syscall::exit(1);
                     panic!("failed to exit");
                 }
                 n => n,
             }
         };
         
         let mut status = 0;
         syscall::waitpid(pid, &mut status, 0)
            .map_err(|e| Error::from_raw_os_error(e.errno) )?;
        
        Ok(())
    }
    
    /// This puppy sets env vars, user/group ids, cwd, namespaces,
    /// etc, and actually calls fexec.
    // Currently not parsing shebangs or $PATH for bin locations
    // Open files and things at the top here, so that it
    //   doesn't interfere with namespace setting
    fn do_exec(&self, bin: File) -> SyscallError {
        macro_rules! t {
            ($err:expr) => {
                match $err {
                    Ok(val) => val,
                    Err(e) => return e
                }
            }
        }
        
        if let Some(g) = self.gid {
            t!(syscall::setregid(g, g));
        }
        if let Some(u) = self.uid {
            t!(syscall::setreuid(u, u));
        }
        if let Some(ref cwd) = self.cwd {
            t!(syscall::chdir(cwd));
        }
        if let Some(ref ns) = self.ns {
            let ns = t!(syscall::mkns(&raw_ns(ns)));
            t!(syscall::setrens(ns, ns));
        }
        
        if self.clear_env {
            for (k, _) in env::vars_os() {
                env::remove_var(k);
            }
        }
        
        for (key, val) in self.env.iter() {
            env::set_var(key, val);
        }
        
        let vars: Vec<[usize; 2]> = env::vars_os()
            .map(|(var, val)| format!("{}={}", var.to_string_lossy(), val.to_string_lossy()) )
            .map(|var| [var.as_ptr() as usize, var.len()] )
            .collect();
        
        let mut args: Vec<[usize; 2]> = Vec::with_capacity(1 + self.args.len());
        args.push([self.bin.as_ptr() as usize, self.bin.len()]);
        args.extend(self.args.iter().map(|arg| [arg.as_ptr() as usize, arg.len()]));
        
        if let Err(err) = syscall::fexec(bin.as_raw_fd(), &args, &vars) {
            err
        } else {
            panic!("return from exec without err");
        }
    }
}

impl Display for Command {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.bin)?;
        for arg in self.args.iter() {
            write!(f, " {}", arg)?;
        }
        Ok(())
    }
}

//TODO: Feels like this could be done better
fn raw_ns(schemes: &Vec<String>) -> Vec<[usize; 2]> {
    let mut ptrs = vec![];
    for scheme in schemes.iter() {
        ptrs.push([scheme.as_ptr() as usize, scheme.len()]);
    }
    ptrs
}
