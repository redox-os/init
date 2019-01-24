use std::collections::HashMap;
use std::ops::Deref;

use chashmap::CHashMap;
use failure::{err_msg, Error};
use generational_arena::Index;
use log::{error, warn};
//use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::dep_graph::DepGraph;
use crate::service::{Service, ServiceState};

/// Main data structure for init, containing the main interface
/// for dealing with services
pub struct ServiceGraph {
    graph: DepGraph<Service>,
    
    // Must be sorta global so that dependencies across `push_services`
    //   boundaries link up correctly.
    redirect_map: HashMap<String, Index>,
    
    state_map: CHashMap<Index, ServiceState>
}

impl ServiceGraph {
    pub fn new() -> ServiceGraph {
        ServiceGraph {
            graph: DepGraph::new(),
            redirect_map: HashMap::new(),
            state_map: CHashMap::new()
        }
    }
    
    /// Push some services into the graph, and add their dependency nodes.
    /// Note that this does not start any services, only their metadata
    /// is inserted into the graph. Metadata for services is not manipulated
    pub fn push_services(&mut self, mut services: Vec<Service>) {
        self.graph.reserve(services.len());
        self.state_map.reserve(services.len());
        
        // This is sorta ugly, but provides have to work
        for service in services.drain(..) {
            let name = service.name.clone();
            let provides = service.provides.clone();
            let index = self.graph.insert(service);
            
            if let Some(mut provides) = provides {
                self.redirect_map.reserve(provides.len() + 1);
                for provide in provides.drain(..) {
                    self.redirect_map.insert(provide, index);
                }
            }
            
            self.redirect_map.insert(name, index);
            self.state_map.insert(index, ServiceState::default());
        }
        
        //TODO: Only iterate over the services that are being added
        for parent in self.redirect_map.values() {
            let dependencies = self.graph.get(*parent)
                .expect("services were just added")
                .dependencies.clone();
            
            if let Some(ref dependencies) = dependencies {
                for dependency in dependencies.iter() {
                    match self.redirect_map.get(dependency) {
                        Some(child) => self.graph.dependency(*parent, *child)
                            .unwrap_or_else(|()| warn!("failed to add dependency") ),
                        // It's not a super big deal if a dependency doesn't exist
                        // I mean, it is, but IDK what to do in that situation
                        //   It's really a pkg problem at that point
                        //TODO: The dep really needs to be invalidated in some way
                        None => warn!("dependency not found: '{}'", dependency)
                    }
                }
            }
        }
    }
    
    /// Runs the `start` method of a service if the service referenced by
    /// `index` exists, and the service is not already running.
    pub fn start_service(&self, index: Index) -> Result<(), Error> {
        let service = self.graph.get(index)
            .ok_or(err_msg("service not found"))?;
        
        let service_state = self.state_map.get(&index)
            .map(|guard| *guard.deref() )
            .unwrap_or(ServiceState::Offline);
        
        if !service_state.is_online() {
            service.wait_method(&"start".to_string())?;
            
            self.state_map.insert(index, ServiceState::Online);
        }
        Ok(())
    }
    
    /// Find a solution to the dependency tree and run `ServiceGraph::start_service`
    /// on each index in the tree.
    pub fn start_services(&self) {
        let resolved = self.graph.grouped_resolve();
        
        for group in resolved.iter() {
            //TODO: Use par_iter() if rayon will work on redox
            group.iter().for_each(|index| {
                self.start_service(*index)
                    .unwrap_or_else(|err| { error!("error starting service: {}", err) });
            });
        }
    }
}
