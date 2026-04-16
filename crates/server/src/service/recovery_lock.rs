use dashmap::DashSet;

#[derive(Default)]
pub struct RecoveryLockService {
    frozen: DashSet<String>,
}

impl RecoveryLockService {
    pub fn new() -> Self {
        Self {
            frozen: DashSet::new(),
        }
    }

    pub fn freeze(&self, server_id: &str) {
        self.frozen.insert(server_id.to_string());
    }

    pub fn release(&self, server_id: &str) {
        self.frozen.remove(server_id);
    }

    pub fn writes_allowed_for(&self, server_id: &str) -> bool {
        !self.frozen.contains(server_id)
    }
}

#[cfg(test)]
mod tests {
    use super::RecoveryLockService;

    #[test]
    fn locked_server_denies_writes_until_released() {
        let locks = RecoveryLockService::new();

        assert!(locks.writes_allowed_for("srv-1"));

        locks.freeze("srv-1");
        assert!(!locks.writes_allowed_for("srv-1"));

        locks.release("srv-1");
        assert!(locks.writes_allowed_for("srv-1"));
    }
}
