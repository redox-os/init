use std::collections::HashMap;

use crossbeam::channel::Sender;
use generational_arena::Index;
use log::{error, info, warn};

use crate::Event;
use crate::dep_graph::DepGraph;
use crate::service::{Service, State};

/// Main data structure for init, containing the main interface
/// for dealing with services
pub struct ServiceTree {
    graph: DepGraph<Service>,
    // Redirection table for service names to indexes, improves insert perf
    name_table: HashMap<String, Index>,
    sender: Sender<Event>
    //provide_hooks: HashMap<String, Box<FnMut(&mut ServiceTree)>>
}

impl ServiceTree {
    pub fn new(sender: Sender<Event>) -> ServiceTree {
        ServiceTree {
            graph: DepGraph::new(),
            name_table: HashMap::new(),
            sender
            //provide_hooks: HashMap::new()
        }
    }
    /*
     * There are a couple of places where code like this is commented out,
     * the code that exists is horribly broken. Looking for a better solution.
     */
    /// Add a hook to be called after a dependency has been provided.
    /// The dep can be a service's name, or anything listed in the 'provides'
    /// field in a service.toml. Currently this is backed by a hashmap, so
    /// it will silently overwrite an existing entry if called multiple
    /// times with the same dep.
    /*
    pub fn provide_hook(&mut self, dep: String, hook: Box<FnMut(&mut ServiceTree)>) {
        self.provide_hooks.insert(dep, hook);
    }*/
    
    /// Push some services into the graph, and add their dependency nodes.
    /// Then send a StartService event via the `Sender` passed to `Self::new`
    pub fn push_services(&mut self, mut services: Vec<Service>) {
        self.graph.reserve(services.len());
        
        // These iterations MUST be done separately b/c otherwise an existing
        //   dep might not be in the graph yet.
        self.name_table = services.drain(..)
            .map(|service| (service.name.clone(), self.graph.insert(service)) )
            .collect();
        
        for (service_name, service_index) in self.name_table.iter() {
            let service = self.graph.get(*service_index)
                .expect("services were just added");
            
            if let Some(ref dependencies) = service.dependencies.clone() {
                for dependency in dependencies.iter() {
                    match self.name_table.get(dependency) {
                        Some(dependent) => self.graph.dependency(*service_index, *dependent)
                            .unwrap_or_else(|_| warn!("failed to add dependency") ),
                        // It's not a super big deal if a dependency doesn't exist
                        // I mean, it is, but IDK what to do in that situation
                        //   It's really a pkg/sysadmin problem at that point
                        None => warn!("dependency not found: {}", dependency)
                    }
                }
            }
            
            self.sender.send(Event::ServiceStart(*service_index))
                .unwrap_or_else(|err| error!("error sending start service event: {}", err));
        }
    }
    
    /// Attempts to start a service. If the service has unmet dependencies,
    /// it is not started and those dependencies are sent on the Sender
    /// passed to `Self::new` followed by the service itself.
    pub fn start_service(&self, index: Index) {
        if let Some(dependencies) = self.graph.dependencies(index) {
            let mut service_startable = true;
            
            // Could be after the loop, but is helpful for error reporting
            let service = self.graph.get(index)
                .unwrap(); // We already checked that this index exists
            
            if service.state.is_running() {
                return ();
            }
            
            for dep_index in dependencies {
                if let Some(dep) = self.graph.get(*dep_index) {
                    if !dep.state.is_running() {
                        self.sender.send(Event::ServiceStart(*dep_index))
                            .unwrap_or_else(|err| error!("error sending start service event: {}", err));
                        service_startable = false;
                    }
                } else {
                    warn!("missing a dependency for service: {}", service.name);
                }
            }
            
            if service_startable {
                if let Some(method) = service.methods.get("start") {
                    if !service.state.is_running() {
                        method.wait();
                        //TODO: Somehow fix this
                        self.sender.send(Event::ServiceChangedState(index, State::Online));
                    }
                } else {
                    error!("missing 'start' method for service: {}", service.name);
                }
            } else {
                self.sender.send(Event::ServiceStart(index))
                    .unwrap_or_else(|err| error!("error sending start service event: {}", err));
            }
        } else {
            warn!("attempted to start service that did not exist or was removed");
        }
    }
    
    pub fn set_service_state(&mut self, service_index: Index, state: State) -> Option<()> {
        self.graph.get_mut(service_index)?.state = state;
        Some(())
    }
    
    /// WIP: This function attempts to run the start method on each service in the graph
    /// if it is not already running.
    pub fn start_services(&mut self, start_file_services: bool) {
        let resolved = self.graph.grouped_resolve();
        
        for group in resolved.iter() {
            group.iter().for_each(|index| {
                let service = self.graph.get(*index)
                    // These should all exist, the resolver can only
                    // return indexes that are in the graph anyway
                    .expect("resolved service index did not exist");
                
                if let Some(method) = service.methods.get("start") {
                    if !service.state.is_running() {
                        method.wait();
                    }
                } else {
                    error!("service {} missing 'start' method", service.name);
                }
            });
        }
        
        //Nasty hack. TODO: Remove
        
        if start_file_services {
            use std::env;
            use crate::service::services;
            
            // display:
            crate::switch_stdio("display:1")
                .unwrap_or_else(|err| {
                    warn!("{}", err);
                });
            
            // file:
            info!("setting cwd to file:");
            if let Err(err) = env::set_current_dir("file:") {
                error!("failed to set cwd: {}", err);
            }
            
            info!("setting PATH=file:/bin");
            env::set_var("PATH", "file:/bin");
            
            let fs_services = services("/etc/init.d")
                .unwrap_or_else(|err| {
                    warn!("{}", err);
                    vec![]
                });
            
            self.push_services(fs_services);
            self.start_services(false);
        }
        
        /*
        for index in resolved.iter() {
            let service = self.graph.get_mut(*index)
                // These should all exist, we just got them out
                .expect("resolved service index did not exist");
            
            if let Some(method) = service.methods.get("start") {
                if !service.state.is_running() {
                    method.wait();
                }
            } else {
                error!("service {} missing 'start' method", service.name);
                service.state = State::Failed;
            }
            
            //TODO: Better solution to this
            //  Should be able to get rid of the mutable borrow here I hope
            service.state = State::Online;
            /*
            if let Some(on_provided) = self.provide_hooks.get(&service.name) {
                on_provided(self);
            }
            
            if let Some(ref provides) = service.provides {
                for provide in provides.iter() {
                    if let Some(on_provided) = self.provide_hooks.get(provide) {
                        on_provided(self);
                    }
                }
            }*/
        }*/
    }
}
