use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretRecord {
    pub reference: String,
    pub label: String,
    pub backend: String,
}

pub trait SecretVaultBackend: Send + Sync {
    fn kind(&self) -> &'static str;
    fn put(&mut self, reference: String, secret: String, label: String) -> SecretRecord;
    fn get(&self, reference: &str) -> Option<String>;
}

#[derive(Default)]
pub struct InMemoryVaultBackend {
    store: HashMap<String, String>,
    labels: HashMap<String, String>,
}

impl SecretVaultBackend for InMemoryVaultBackend {
    fn kind(&self) -> &'static str {
        "in-memory"
    }

    fn put(&mut self, reference: String, secret: String, label: String) -> SecretRecord {
        self.store.insert(reference.clone(), secret);
        self.labels.insert(reference.clone(), label.clone());
        SecretRecord {
            reference,
            label,
            backend: self.kind().to_string(),
        }
    }

    fn get(&self, reference: &str) -> Option<String> {
        self.store.get(reference).cloned()
    }
}

#[derive(Default)]
pub struct OsNativeVaultBackend {
    inner: InMemoryVaultBackend,
}

impl SecretVaultBackend for OsNativeVaultBackend {
    fn kind(&self) -> &'static str {
        "os-native"
    }

    fn put(&mut self, reference: String, secret: String, label: String) -> SecretRecord {
        let mut record = self.inner.put(reference, secret, label);
        record.backend = self.kind().to_string();
        record
    }

    fn get(&self, reference: &str) -> Option<String> {
        self.inner.get(reference)
    }
}

#[derive(Default)]
pub struct FileVaultBackend {
    inner: InMemoryVaultBackend,
}

impl SecretVaultBackend for FileVaultBackend {
    fn kind(&self) -> &'static str {
        "file-vault"
    }

    fn put(&mut self, reference: String, secret: String, label: String) -> SecretRecord {
        let mut record = self.inner.put(reference, secret, label);
        record.backend = self.kind().to_string();
        record
    }

    fn get(&self, reference: &str) -> Option<String> {
        self.inner.get(reference)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum VaultError {
    BackendNotRegistered(String),
}

pub struct SecretVaultRouter {
    active_backend: String,
    backends: HashMap<String, Box<dyn SecretVaultBackend>>,
}

impl Default for SecretVaultRouter {
    fn default() -> Self {
        let mut router = Self {
            active_backend: "os-native".into(),
            backends: HashMap::new(),
        };
        router.register_backend(Box::<OsNativeVaultBackend>::default());
        router.register_backend(Box::<FileVaultBackend>::default());
        router.register_backend(Box::<InMemoryVaultBackend>::default());
        router
    }
}

impl SecretVaultRouter {
    pub fn register_backend(&mut self, backend: Box<dyn SecretVaultBackend>) {
        self.backends.insert(backend.kind().to_string(), backend);
    }

    pub fn set_active_backend(&mut self, backend: &str) -> Result<(), VaultError> {
        if !self.backends.contains_key(backend) {
            return Err(VaultError::BackendNotRegistered(backend.to_string()));
        }
        self.active_backend = backend.to_string();
        Ok(())
    }

    pub fn active_backend(&self) -> &str {
        &self.active_backend
    }

    pub fn put(
        &mut self,
        reference: impl Into<String>,
        secret: impl Into<String>,
        label: impl Into<String>,
    ) -> Result<SecretRecord, VaultError> {
        let backend = self
            .backends
            .get_mut(&self.active_backend)
            .ok_or_else(|| VaultError::BackendNotRegistered(self.active_backend.clone()))?;
        Ok(backend.put(reference.into(), secret.into(), label.into()))
    }

    pub fn get(&self, reference: &str) -> Result<Option<String>, VaultError> {
        let backend = self
            .backends
            .get(&self.active_backend)
            .ok_or_else(|| VaultError::BackendNotRegistered(self.active_backend.clone()))?;
        Ok(backend.get(reference))
    }
}

#[cfg(test)]
mod tests {
    use super::{SecretVaultRouter, VaultError};

    #[test]
    fn stores_secret_with_stable_reference() {
        let mut router = SecretVaultRouter::default();
        let record = router
            .put("vault://bridgingio/ssh/dev", "PRIVATE_KEY", "dev ssh key")
            .expect("store secret");

        assert_eq!(record.reference, "vault://bridgingio/ssh/dev");
        let loaded = router
            .get(&record.reference)
            .expect("active backend")
            .expect("secret value");
        assert_eq!(loaded, "PRIVATE_KEY");
    }

    #[test]
    fn allows_switching_backends_without_changing_reference_shape() {
        let mut router = SecretVaultRouter::default();
        let reference = "vault://bridgingio/adb/default";

        router
            .set_active_backend("file-vault")
            .expect("switch backend");
        let record = router
            .put(reference, "ADB_SECRET", "adb secret")
            .expect("store in file backend");
        assert_eq!(record.reference, reference);
        assert_eq!(record.backend, "file-vault");
    }

    #[test]
    fn errors_for_unknown_backend() {
        let mut router = SecretVaultRouter::default();
        let err = router
            .set_active_backend("future-vault")
            .expect_err("must fail");
        assert_eq!(err, VaultError::BackendNotRegistered("future-vault".into()));
    }
}
