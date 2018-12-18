use std::collections::HashMap;

use generational_arena::Index;

use dependency_graph::DepGraph;
use service::{Service, State};

pub fn graph_from_services(services: Vec<Service>) -> DepGraph<Service> {
    let mut graph = DepGraph::with_capacity(services.len());
    graph_add_services(&mut graph, services);
    graph
}

pub fn graph_add_services(graph: &mut DepGraph<Service>, mut services: Vec<Service>) {
    let services: HashMap<String, Index> = services.drain(..)
        .map(|service| (service.name.clone(), graph.insert(service)) )
        .collect();
    
    for parent in services.values() {
        let dependencies = graph.get(*parent)
            .expect("services were just added")
            .dependencies.clone();
        
        if let Some(ref dependencies) = dependencies {
            for dependency in dependencies.iter() {
                match services.get(dependency) {
                    Some(child) => graph.dependency(*parent, *child)
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

pub fn start_services(mut graph: DepGraph<Service>, provide_hooks: HashMap<String, impl Fn()>) {
    let resolved = graph.linear_resolve();
    
    for index in resolved.iter() {
        let service = graph.get_mut(*index)
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
        
        if let Some(ref provides) = service.provides {
            for provide in provides.iter() {
                if let Some(on_provided) = provide_hooks.get(provide) {
                    on_provided();
                }
            }
        }
    }
}
