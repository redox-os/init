use std::collections::{HashMap, HashSet};

use failure::{Error, err_msg};
use generational_arena::{Arena, Index};

use service::Service;

#[derive(Debug)]
pub struct ServiceNode {
    pub service: Service,
    pub dependencies: Vec<Index>
}

impl ServiceNode {
    // Empty dependencies
    fn from_service(service: Service) -> ServiceNode {
        ServiceNode {
            service,
            dependencies: vec![]
        }
    }
}

pub struct DepGraph {
    pub graph: Arena<ServiceNode>,
    on_provides: HashMap<String, fn()>
}

impl DepGraph {
    pub fn on_provided(mut self, provide: impl AsRef<str>, callback: fn()) -> DepGraph {
        self.on_provides.insert(provide.as_ref().to_string(), callback);
        self
    }
    
    pub fn from_services(mut services: Vec<Service>) -> DepGraph {
        let mut graph = Arena::with_capacity(services.len());
        
        let services: HashMap<String, Index> = services.drain(..)
            .map(|service| (service.name.clone(), graph.insert(ServiceNode::from_service(service))) )
            .collect();
        
        for index in services.values() {
            let node = graph.get_mut(*index)
                .expect("services were just added");
            
            if let Some(ref dependencies) = node.service.dependencies {
                for dependency in dependencies.iter() {
                    match services.get(dependency) {
                        Some(index) => node.dependencies.push(*index),
                        // It's not a super big deal if a dependency doesn't exist
                        None => warn!("dependency not found: {}", dependency)
                    }
                }
            }
        }
        
        DepGraph {
            graph,
            on_provides: HashMap::new()
        }
    }

    /// Naive linear dependency resolution algorithm. The _should_
    /// be a solution to the dependency graph. Note that the solution
    /// is probably not deterministic.
    ///
    /// # Limitations
    /// Not currently detecting dependency cycles
    /// Can't figure out
    fn resolve_linear(&self) -> Vec<Index> {
        let arena_len = self.graph.len();
        let mut resolved = Vec::with_capacity(arena_len);
        let mut seen = HashSet::with_capacity(arena_len);
        
        while resolved.len() < arena_len {
            for (index, service_node) in self.graph.iter() {
                // formatting?
                if !seen.contains(&index) &&
                        (service_node.dependencies.is_empty() ||
                        service_node.dependencies.iter().all(|index| resolved.contains(index)))
                {
                    seen.insert(index);
                    resolved.push(index);
                }
            }
        }
        resolved
    }
    
    /// Resolve dependencies and start them. This is not multi-threaded right now,
    /// although that is the goal eventually
    pub fn start(&self) -> Result<(), Error> {
        let resolved = self.resolve_linear();
        
        for index in resolved.iter() {
            // These should all exist, we just got them out
            let node = self.graph.get(*index).unwrap();
            
            if let Some(method) = node.service.methods.get("start") {
                method.wait();
            } else {
                let msg = format!("service {} missing 'start' method", node.service.name);
                return Err(err_msg(msg));
            }
            
            if let Some(ref provides) = node.service.provides {
                for provide in provides.iter() {
                    if let Some(on_provided) = self.on_provides.get(provide) {
                        on_provided();
                    }
                }
            }
        }
        Ok(())
    }
}
