use std::collections::{HashMap, HashSet};

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

pub fn build_graph(mut services: Vec<Service>) -> Arena<ServiceNode> {
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
    
    graph
}

/// Naive linear dependency resolution algorithm. The _should_
/// be a solution to the dependency graph. Note that the solution
/// is probably not deterministic.
///
/// # Limitations
/// Not currently detecting dependency cycles
/// Can't figure out
pub fn resolve_linear(graph: &Arena<ServiceNode>) -> Vec<Index> {
    let arena_len = graph.len();
    let mut resolved = Vec::with_capacity(arena_len);
    let mut seen = HashSet::with_capacity(arena_len);
    
    while resolved.len() < arena_len {
        for (index, service_node) in graph.iter() {
            // formatting?
            if (service_node.dependencies.is_empty() ||
                    service_node.dependencies.iter().all(|index| resolved.contains(index))) &&
                    !seen.contains(&index) {
                seen.insert(index);
                resolved.push(index);
            }
        }
    }
    
    resolved
}
