use std::ops::Deref;
use std::sync::RwLock;

use chashmap::CHashMap;
use failure::{err_msg, Error};
use generational_arena::Index;
use log::{error, warn};
//use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::dep_graph::DepGraph;
use crate::service::{Service, ServiceState};

/// Main data structure for init, containing the main interface
/// for dealing with services.
/// This is implemented entirely with thread-safe interiorly mutable
/// structures.
pub struct ServiceGraph {
    graph: RwLock<DepGraph<Service>>,
    
    /// Names of services (and provides of services) mapped to the
    ///   service index that provides that name.
    // Must be sorta global so that dependencies across `push_services`
    //   boundaries link up correctly.
    redirect_map: CHashMap<String, Index>,
    
    state_map: CHashMap<Index, ServiceState>
}

impl ServiceGraph {
    pub fn new() -> ServiceGraph {
        ServiceGraph {
            graph: RwLock::new(DepGraph::new()),
            redirect_map: CHashMap::new(),
            state_map: CHashMap::new()
        }
    }
    
    /// Push some services into the graph, and add their dependency nodes.
    /// Note that this does not start any services, only their metadata
    /// is inserted into the graph. Metadata for services is not manipulated
    //TODO: Split this into a couple funcs
    pub fn push_services(&self, mut services: Vec<Service>) {
        let mut graph = self.graph.write()
            .expect("service graph mutex poisoned");
        
        graph.reserve(services.len());
        self.state_map.reserve(services.len());
        
        let mut service_indexes = vec![];
        
        // This is sorta ugly, but provides have to work
        for service in services.drain(..) {
            let name = service.name.clone();
            let provides = service.provides.clone();
            let index = graph.insert(service);
            
            if let Some(mut provides) = provides {
                self.redirect_map.reserve(provides.len() + 1);
                for provide in provides.drain(..) {
                    self.redirect_map.insert(provide, index);
                }
            }
            
            service_indexes.push(index);
            self.redirect_map.insert(name, index);
            self.state_map.insert(index, ServiceState::default());
        }
        
        //TODO: Only iterate over the services that are being added
        for parent in service_indexes {
            let dependencies = graph.get(parent)
                .expect("services were just added")
                .dependencies.clone();
            
            if let Some(ref dependencies) = dependencies {
                for dependency in dependencies.iter() {
                    match self.redirect_map.get(dependency) {
                        Some(child) => graph.dependency(parent, *child)
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
        let graph = self.graph.read()
            .expect("service graph lock poisoned");
        self.start_service_with_graph(&graph, index)
    }
    
    // Allow for less time spent locking and unlocking the graph
    // Ofc `self.graph.read()` and `graph` are probably going to
    // end up the same, the point is to prevent lots of unessasary locking.
    fn start_service_with_graph(&self, graph: &DepGraph<Service>, index: Index) -> Result<(), Error> {
        let service = graph.get(index)
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
        let graph = self.graph.read()
            .expect("service graph lock poisoned");
        let resolved = graph.grouped_resolve();
        
        for group in resolved.iter() {
            //TODO: Use par_iter() if rayon will work on redox
            group.iter().for_each(|index| {
                self.start_service_with_graph(&graph, *index)
                    .unwrap_or_else(|err| { error!("error starting service: {}", err) });
            });
        }
    }
}
