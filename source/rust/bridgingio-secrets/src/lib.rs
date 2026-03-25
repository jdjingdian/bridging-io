use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretRecord {
    pub reference: String,
    pub label: String,
}

pub trait SecretVault {
    fn put(&mut self, reference: String, secret: String, label: String) -> SecretRecord;
    fn get(&self, reference: &str) -> Option<&str>;
}

#[derive(Default)]
pub struct InMemorySecretVault {
    store: HashMap<String, String>,
    labels: HashMap<String, String>,
}

impl SecretVault for InMemorySecretVault {
    fn put(&mut self, reference: String, secret: String, label: String) -> SecretRecord {
        self.store.insert(reference.clone(), secret);
        self.labels.insert(reference.clone(), label.clone());
        SecretRecord { reference, label }
    }

    fn get(&self, reference: &str) -> Option<&str> {
        self.store.get(reference).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::{InMemorySecretVault, SecretVault};

    #[test]
    fn stores_secret_and_keeps_reference() {
        let mut vault = InMemorySecretVault::default();
        let record = vault.put(
            "vault:ssh-key:dev".into(),
            "PRIVATE_KEY".into(),
            "dev ssh key".into(),
        );

        assert_eq!(record.reference, "vault:ssh-key:dev");
        assert_eq!(vault.get(&record.reference), Some("PRIVATE_KEY"));
    }
}

