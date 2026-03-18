use dashmap::DashMap;
use std::collections::HashSet;

pub struct DockerViewerTracker {
    viewers: DashMap<String, HashSet<String>>,
}

impl DockerViewerTracker {
    pub fn new() -> Self {
        Self {
            viewers: DashMap::new(),
        }
    }

    /// Add a viewer. Returns true if this is the first viewer for this server.
    pub fn add_viewer(&self, server_id: &str, connection_id: &str) -> bool {
        let mut set = self.viewers.entry(server_id.to_string()).or_default();
        let was_empty = set.is_empty();
        set.insert(connection_id.to_string());
        was_empty
    }

    /// Remove a viewer. Returns true if this was the last viewer for this server.
    pub fn remove_viewer(&self, server_id: &str, connection_id: &str) -> bool {
        if let Some(mut set) = self.viewers.get_mut(server_id) {
            set.remove(connection_id);
            if set.is_empty() {
                drop(set);
                self.viewers.remove(server_id);
                return true;
            }
        }
        false
    }

    pub fn has_viewers(&self, server_id: &str) -> bool {
        self.viewers
            .get(server_id)
            .is_some_and(|set| !set.is_empty())
    }

    /// Remove all subscriptions for a connection (browser disconnect).
    /// Returns vec of (server_id, was_last_viewer).
    pub fn remove_all_for_connection(&self, connection_id: &str) -> Vec<(String, bool)> {
        let server_ids: Vec<String> = self
            .viewers
            .iter()
            .filter(|entry| entry.value().contains(connection_id))
            .map(|entry| entry.key().clone())
            .collect();
        let mut results = Vec::new();
        for server_id in server_ids {
            let is_last = self.remove_viewer(&server_id, connection_id);
            results.push((server_id, is_last));
        }
        results
    }

    /// Remove all viewers for a server (e.g., capability revocation).
    /// Returns true if there were any viewers.
    pub fn remove_all_for_server(&self, server_id: &str) -> bool {
        self.viewers
            .remove(server_id)
            .is_some_and(|(_, set)| !set.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_first_viewer() {
        let tracker = DockerViewerTracker::new();
        assert!(tracker.add_viewer("srv1", "conn1"));
        assert!(!tracker.add_viewer("srv1", "conn2"));
    }

    #[test]
    fn test_last_viewer() {
        let tracker = DockerViewerTracker::new();
        tracker.add_viewer("srv1", "conn1");
        tracker.add_viewer("srv1", "conn2");
        assert!(!tracker.remove_viewer("srv1", "conn1"));
        assert!(tracker.remove_viewer("srv1", "conn2"));
    }

    #[test]
    fn test_has_viewers() {
        let tracker = DockerViewerTracker::new();
        assert!(!tracker.has_viewers("srv1"));
        tracker.add_viewer("srv1", "conn1");
        assert!(tracker.has_viewers("srv1"));
    }

    #[test]
    fn test_remove_all_for_connection() {
        let tracker = DockerViewerTracker::new();
        tracker.add_viewer("srv1", "conn1");
        tracker.add_viewer("srv2", "conn1");
        tracker.add_viewer("srv2", "conn2");

        let affected = tracker.remove_all_for_connection("conn1");
        assert_eq!(affected.len(), 2);
        assert!(affected.iter().any(|(id, last)| id == "srv1" && *last));
        assert!(affected.iter().any(|(id, last)| id == "srv2" && !*last));
    }

    #[test]
    fn test_remove_all_for_server() {
        let tracker = DockerViewerTracker::new();
        tracker.add_viewer("srv1", "conn1");
        tracker.add_viewer("srv1", "conn2");
        assert!(tracker.remove_all_for_server("srv1"));
        assert!(!tracker.has_viewers("srv1"));
        assert!(!tracker.remove_all_for_server("srv1"));
    }
}
