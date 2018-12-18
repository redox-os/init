use std::collections::HashSet;

use generational_arena::{Arena, Index};

/// A container struct that allows for a
/// nice abstraction of a dependency graph
struct Node<T> {
    inner: T,
    dependencies: Vec<Index>
}

impl<T> Node<T> {
    fn new(inner: T) -> Node<T> {
        Node {
            inner,
            dependencies: vec![]
        }
    }
    
    fn get_inner(&self) -> &T {
        &self.inner
    }
    
    fn get_mut_inner(&mut self) -> &mut T {
        &mut self.inner
    }
    
    fn unwrap(self) -> T {
        self.inner
    }
}

/// A sorta thin wrapper over a Generational arena that includes
/// dependency relationships between nodes and some solvers
pub struct DepGraph<T> {
    graph: Arena<Node<T>>
}

impl<T> DepGraph<T> {
    /// Wrapper over `generational_arena::Arena::with_capacity`
    pub fn with_capacity(n: usize) -> DepGraph<T> {
        DepGraph {
            graph: Arena::with_capacity(n)
        }
    }
    
    /// Add an element to the graph, returning an index to the element
    pub fn insert(&mut self, inner: T) -> Index {
        self.graph.insert(Node::new(inner))
    }
    
    /// Get an immutable borrow of an element by index if it exists
    pub fn get(&self, indx: Index) -> Option<&T> {
        self.graph.get(indx)
            .map(|node| node.get_inner() )
    }
    
    /// Get a mutable borrow of an element by index, if it exists
    pub fn get_mut(&mut self, indx: Index) -> Option<&mut T> {
        self.graph.get_mut(indx)
            .map(|node| node.get_mut_inner() )
    }
    
    /// Remove an element from the graph by index, returning
    /// the value if it exists
    pub fn remove(&mut self, indx: Index) -> Option<T> {
        self.graph.remove(indx)
            .map(|node| node.unwrap() )
    }
    
    /// Add a dependent relationship between a parent and a child
    ///
    /// Returns Err() if either of the indecies do not exist in the graph
    pub fn dependency(&mut self, parent: Index, child: Index) -> Result<(), ()> {
        if self.graph.contains(parent) && self.graph.contains(child) {
            self.graph.get_mut(parent)
                .unwrap() // Cannot be None
                .dependencies.push(child);
            Ok(())
        } else {
            Err(())
        }
    }
    
    pub fn linear_resolve(&self) -> Vec<Index> {
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
}
