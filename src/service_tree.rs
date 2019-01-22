use std::collections::HashMap;

use generational_arena::Index;
use log::{error, warn};

use crate::dep_graph::DepGraph;
use crate::service::{Service, services, State};

/// Main data structure for init, containing the main interface
/// for dealing with services
pub struct ServiceTree {
    graph: DepGraph<Service>,
    //provide_hooks: HashMap<String, Box<FnMut(&mut ServiceTree)>>
}

impl ServiceTree {
    pub fn new() -> ServiceTree {
        ServiceTree {
            graph: DepGraph::new(),
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
    /// Note that this does not start any services, only their metadata
    /// is inserted into the graph. Metadata for services is not manipulated
    pub fn push_services(&mut self, mut services: Vec<Service>) {
        self.graph.reserve(services.len());
        
        let services: HashMap<String, Index> = services.drain(..)
            .map(|service| (service.name.clone(), self.graph.insert(service)) )
            .collect();
        
        for parent in services.values() {
            let dependencies = self.graph.get(*parent)
                .expect("services were just added")
                .dependencies.clone();
            
            if let Some(ref dependencies) = dependencies {
                for dependency in dependencies.iter() {
                    match services.get(dependency) {
                        Some(child) => self.graph.dependency(*parent, *child)
                            .unwrap_or_else(|_| warn!("failed to add dependency") ),
                        // It's not a super big deal if a dependency doesn't exist
                        // I mean, it is, but IDK what to do in that situation
                        //   It's really a pkg problem at that point
                        None => warn!("dependency not found: {}", dependency)
                    }
                }
            }
        }
    }
    
    /// WIP: This function attempts to run the start method on each service in the graph
    /// if it is not already running.
    pub fn start_services(&mut self) {
        let resolved = self.graph.linear_resolve();
        
        for index /*group*/ in resolved.iter() {
            //for index in group.iter() {
                let mut service = self.graph.get_mut(*index)
                    // These should all exist, the resolver can only
                    // return indexes that are in the graph anyway
                    .expect("resolved service index did not exist");
                
                if let Some(method) = service.methods.get("start") {
                    if !service.state.is_running() {
                        method.wait();
                        service.state = State::Online;
                    }
                } else {
                    error!("service {} missing 'start' method", service.name);
                }
                
                if let Some(provides) = &service.provides {
                    
                    if provides.contains(&"display:".to_string()) {
                        crate::switch_stdio("display:1")
                            .unwrap_or_else(|err| {
                                warn!("{}", err);
                            });
                    }
                    
                    if provides.contains(&"file:".to_string()) {
                        let fs_services = services("/etc/init.d")
                            .unwrap_or_else(|err| {
                                warn!("{}", err);
                                vec![]
                            });
                        
                        self.push_services(fs_services);
                        self.start_services();
                    }
                }
            //}
        }
    }
}
