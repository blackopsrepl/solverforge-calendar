use std::collections::{HashMap, HashSet, VecDeque};

use crate::models::{DependencyType, Event, EventDependency};

/* A DAG of events connected by dependency edges. */
/* `edges[A]` = set of event IDs that A **blocks** (A must finish before they start). */
#[derive(Debug, Default, Clone)]
pub struct EventDag {
    // Forward edges: blocker → set of blocked events.
    edges: HashMap<String, HashSet<String>>,
    // Reverse edges: blocked → set of blockers (for reverse traversal).
    reverse: HashMap<String, HashSet<String>>,
}

impl EventDag {
    pub fn new() -> Self {
        Self::default()
    }

    // Build a DAG from a list of dependencies. Skips edges that would create cycles.
    pub fn from_dependencies(deps: &[EventDependency]) -> Self {
        let mut dag = Self::new();
        for dep in deps {
            if dep.dependency_type == DependencyType::Blocks {
                // Silently skip cycle-forming edges (shouldn't happen if DB is clean)
                let _ = dag.add_edge(&dep.from_event_id, &dep.to_event_id);
            }
        }
        dag
    }

    // Add a directed edge: `from` blocks `to`.
    // Returns `Err` if adding the edge would create a cycle.
    pub fn add_edge(&mut self, from: &str, to: &str) -> Result<(), &'static str> {
        // Check if `to` can reach `from` (would create a cycle)
        if self.can_reach(to, from) {
            return Err("cycle detected");
        }
        self.edges
            .entry(from.to_string())
            .or_default()
            .insert(to.to_string());
        self.reverse
            .entry(to.to_string())
            .or_default()
            .insert(from.to_string());
        // Ensure the 'to' node exists in edges map (for iteration)
        self.edges.entry(to.to_string()).or_default();
        self.reverse.entry(from.to_string()).or_default();
        Ok(())
    }

    // Remove a directed edge.
    pub fn remove_edge(&mut self, from: &str, to: &str) {
        if let Some(blocked) = self.edges.get_mut(from) {
            blocked.remove(to);
        }
        if let Some(blockers) = self.reverse.get_mut(to) {
            blockers.remove(from);
        }
    }

    // True if `from` can reach `to` through forward edges (BFS).
    pub fn can_reach(&self, from: &str, to: &str) -> bool {
        if from == to {
            return true;
        }
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(from.to_string());
        while let Some(node) = queue.pop_front() {
            if visited.contains(&node) {
                continue;
            }
            visited.insert(node.clone());
            if let Some(blocked) = self.edges.get(&node) {
                for next in blocked {
                    if next == to {
                        return true;
                    }
                    queue.push_back(next.clone());
                }
            }
        }
        false
    }

    // Topological sort of all nodes using Kahn's algorithm.
    // Returns nodes in dependency order (blockers before blocked).
    // Returns `None` if the graph has a cycle (shouldn't happen if `add_edge` is used).
    pub fn topological_sort(&self) -> Option<Vec<String>> {
        let all_nodes: HashSet<&String> = self.edges.keys().chain(self.reverse.keys()).collect();

        // Compute in-degrees
        let mut in_degree: HashMap<&String, usize> = HashMap::new();
        for node in &all_nodes {
            in_degree.entry(node).or_insert(0);
        }
        for (_, blocked_set) in &self.edges {
            for blocked in blocked_set {
                *in_degree.entry(blocked).or_insert(0) += 1;
            }
        }

        // Start with nodes that have no incoming edges
        let mut queue: VecDeque<&String> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(node, _)| *node)
            .collect();

        let mut sorted = Vec::new();
        while let Some(node) = queue.pop_front() {
            sorted.push(node.clone());
            if let Some(blocked_set) = self.edges.get(node) {
                for blocked in blocked_set {
                    let deg = in_degree.entry(blocked).or_insert(0);
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(blocked);
                    }
                }
            }
        }

        if sorted.len() == all_nodes.len() {
            Some(sorted)
        } else {
            None // cycle
        }
    }

    // Returns event IDs that have ALL their blocking dependencies resolved.
    // `completed_ids`: set of event IDs already marked complete.
    pub fn next_actionable<'a>(
        &self,
        all_event_ids: impl Iterator<Item = &'a str>,
        completed_ids: &HashSet<String>,
    ) -> Vec<String> {
        all_event_ids
            .filter(|&id| {
                // Not already completed
                if completed_ids.contains(id) {
                    return false;
                }
                // All blockers are completed
                if let Some(blockers) = self.reverse.get(id) {
                    blockers
                        .iter()
                        .all(|blocker| completed_ids.contains(blocker))
                } else {
                    true // no blockers — immediately actionable
                }
            })
            .map(|s| s.to_string())
            .collect()
    }

    // Critical path length from a given start node (longest dependency chain).
    pub fn critical_path_from(&self, start: &str) -> usize {
        self.longest_path(start)
    }

    fn longest_path(&self, node: &str) -> usize {
        if let Some(blocked_set) = self.edges.get(node) {
            if blocked_set.is_empty() {
                return 0;
            }
            1 + blocked_set
                .iter()
                .map(|next| self.longest_path(next))
                .max()
                .unwrap_or(0)
        } else {
            0
        }
    }

    // Overall critical path length of the entire DAG.
    pub fn max_critical_path(&self) -> usize {
        // Start from nodes with no incoming edges (roots)
        let roots: Vec<&String> = self
            .edges
            .keys()
            .filter(|node| self.reverse.get(*node).map_or(true, |r| r.is_empty()))
            .collect();

        roots
            .iter()
            .map(|root| self.longest_path(root))
            .max()
            .unwrap_or(0)
    }

    // All events that block the given event (direct + transitive predecessors).
    pub fn all_blockers(&self, event_id: &str) -> HashSet<String> {
        let mut result = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(event_id.to_string());
        while let Some(node) = queue.pop_front() {
            if let Some(blockers) = self.reverse.get(&node) {
                for b in blockers {
                    if result.insert(b.clone()) {
                        queue.push_back(b.clone());
                    }
                }
            }
        }
        result
    }

    // All events blocked by the given event (direct + transitive dependents).
    pub fn all_blocked_by(&self, event_id: &str) -> HashSet<String> {
        let mut result = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(event_id.to_string());
        while let Some(node) = queue.pop_front() {
            if let Some(blocked_set) = self.edges.get(&node) {
                for b in blocked_set {
                    if result.insert(b.clone()) {
                        queue.push_back(b.clone());
                    }
                }
            }
        }
        result
    }

    // Returns events in topological order filtered to those in `project_event_ids`.
    pub fn project_order(&self, project_event_ids: &HashSet<String>) -> Vec<String> {
        if let Some(sorted) = self.topological_sort() {
            sorted
                .into_iter()
                .filter(|id| project_event_ids.contains(id))
                .collect()
        } else {
            project_event_ids.iter().cloned().collect()
        }
    }

    // Direct blockers of an event (one hop).
    pub fn direct_blockers(&self, event_id: &str) -> Vec<String> {
        self.reverse
            .get(event_id)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    // Direct dependents of an event (one hop).
    pub fn direct_blocked_by(&self, event_id: &str) -> Vec<String> {
        self.edges
            .get(event_id)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }
}

/* Compute project progress from a list of events and the DAG. */
/* An event is considered "complete" if it has `deleted_at` set */
/* (placeholder — in real usage, a dedicated `completed` column or status field would be used). */
pub fn compute_project_progress(
    events: &[Event],
    _dag: &EventDag,
    completed_ids: &HashSet<String>,
) -> (usize, usize, usize) {
    let total = events.len();
    let completed = events
        .iter()
        .filter(|e| completed_ids.contains(&e.id))
        .count();
    let actionable = events
        .iter()
        .filter(|e| !completed_ids.contains(&e.id))
        .count(); // simplified
    (total, completed, actionable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topological_sort() {
        let mut dag = EventDag::new();
        dag.add_edge("A", "B").unwrap();
        dag.add_edge("B", "C").unwrap();
        let sorted = dag.topological_sort().unwrap();
        let a_pos = sorted.iter().position(|x| x == "A").unwrap();
        let b_pos = sorted.iter().position(|x| x == "B").unwrap();
        let c_pos = sorted.iter().position(|x| x == "C").unwrap();
        assert!(a_pos < b_pos);
        assert!(b_pos < c_pos);
    }

    #[test]
    fn test_cycle_detection() {
        let mut dag = EventDag::new();
        dag.add_edge("A", "B").unwrap();
        dag.add_edge("B", "C").unwrap();
        assert!(dag.add_edge("C", "A").is_err());
    }

    #[test]
    fn test_critical_path() {
        let mut dag = EventDag::new();
        dag.add_edge("A", "B").unwrap();
        dag.add_edge("B", "C").unwrap();
        dag.add_edge("A", "D").unwrap();
        // A→B→C is length 2, A→D is length 1
        assert_eq!(dag.critical_path_from("A"), 2);
    }

    #[test]
    fn test_next_actionable() {
        let mut dag = EventDag::new();
        dag.add_edge("A", "B").unwrap();
        dag.add_edge("B", "C").unwrap();
        let completed: HashSet<String> = vec!["A".to_string()].into_iter().collect();
        let all = ["A", "B", "C"];
        let actionable = dag.next_actionable(all.iter().copied(), &completed);
        assert!(actionable.contains(&"B".to_string()));
        assert!(!actionable.contains(&"C".to_string()));
    }
}
