#![allow(dead_code)]

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
/// dependency relationships between nodes and some solvers.
pub struct DepGraph<T> {
    graph: Arena<Node<T>>
}

impl<T> DepGraph<T> {
    pub fn new() -> DepGraph<T> {
        DepGraph {
            graph: Arena::new()
        }
    }
    
    /// Wrapper over `generational_arena::Arena::with_capacity`
    pub fn with_capacity(capacity: usize) -> DepGraph<T> {
        DepGraph {
            graph: Arena::with_capacity(capacity)
        }
    }
    
    /// Wrapper over `generaltional_arena::Arena::reserve`
    pub fn reserve(&mut self, additional_capacity: usize) {
        self.graph.reserve(additional_capacity);
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
    /// Returns Err(()) if either of the indecies do not exist in the graph
    pub fn dependency(&mut self, dependent: Index, dependency: Index) -> Result<(), ()> {
        if self.graph.contains(dependent) && self.graph.contains(dependency) {
            self.graph.get_mut(dependent)
                .unwrap() // Cannot be None
                .dependencies.push(dependency);
            Ok(())
        } else {
            Err(())
        }
    }
    
    /// This function provides a very naive and straightforward algorithm to resolve
    /// a dependency graph. The Vector that is returned is a list that should be a solution
    /// to the graph.
    ///
    /// # Note
    /// This function currently does NOT resolve dependency cycles or other complicated things.
    /// Be careful if your tree includes circular dependencies, you'll likely end up with an infinite loop.
    pub fn linear_resolve(&self) -> Vec<Index> {
        let arena_len = self.graph.len();
        let mut resolved = Vec::with_capacity(arena_len);
        let mut seen = HashSet::with_capacity(arena_len);
        
        while resolved.len() < arena_len {
            for (index, node) in self.graph.iter() {
                // formatting?
                if !seen.contains(&index) &&
                        (node.dependencies.is_empty() ||
                        node.dependencies.iter().all(|index| resolved.contains(index)))
                {
                    seen.insert(index);
                    resolved.push(index);
                }
            }
        }
        resolved
    }
    
    /// Another naive algorithm that resolves the dependency graph into groups of
    /// dependencies whose contents are not co-dependent.
    ///
    /// # Example
    /// ```rust
    /// let groups = dep_graph.grouped_resolve();
    /// # groups[0] contains no Ts that are dependent upon each other
    /// ```
    // Don't ask how this works, I have no idea
    pub fn grouped_resolve(&self) -> Vec<Vec<Index>> {
        let arena_len = self.graph.len();
        let mut groups = vec![vec![]];
        let mut group_count = 0;
        let mut seen = HashSet::with_capacity(arena_len);
        
        while seen.len() < arena_len {
            for (index, node) in self.graph.iter() {
                if !seen.contains(&index) &&
                    (node.dependencies.is_empty() ||
                    node.dependencies.iter()
                        .all(|index| if group_count != 0 {
                            groups[0..group_count - 1].iter().flatten()
                                .any(|i| i == index)
                        } else {
                            false
                        }))
                {
                    seen.insert(index);
                    groups[group_count].push(index);
                }
            }
            group_count += 1;
            groups.push(vec![]);
        }
        groups
    }
}
