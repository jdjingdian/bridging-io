use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use bridgingio_domain::{ArtifactFilter, ArtifactKind, ArtifactRecord};
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactRefineMode {
    Keyword,
    Regex,
    Auto,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArtifactCacheBackend {
    Memory,
    Filesystem,
}

impl ArtifactCacheBackend {
    pub fn parse(value: &str) -> Result<Self, ArtifactStoreError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "memory" => Ok(Self::Memory),
            "filesystem" => Ok(Self::Filesystem),
            other => Err(ArtifactStoreError::new(format!(
                "unsupported artifact backend: {other}"
            ))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Filesystem => "filesystem",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArtifactEvictionPolicy {
    Lru,
}

impl ArtifactEvictionPolicy {
    pub fn parse(value: &str) -> Result<Self, ArtifactStoreError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "lru" => Ok(Self::Lru),
            other => Err(ArtifactStoreError::new(format!(
                "unsupported artifact eviction policy: {other}"
            ))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Lru => "lru",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactStoreConfig {
    pub backend: ArtifactCacheBackend,
    pub root: PathBuf,
    pub max_bytes: u64,
    pub eviction_policy: ArtifactEvictionPolicy,
}

impl Default for ArtifactStoreConfig {
    fn default() -> Self {
        Self {
            backend: ArtifactCacheBackend::Memory,
            root: PathBuf::from(".bridgingio-artifacts"),
            max_bytes: 256 * 1024 * 1024,
            eviction_policy: ArtifactEvictionPolicy::Lru,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactStoreUsage {
    pub backend: ArtifactCacheBackend,
    pub root: Option<PathBuf>,
    pub max_bytes: u64,
    pub used_bytes: u64,
    pub artifact_count: usize,
    pub eviction_policy: ArtifactEvictionPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactReadResult {
    pub record: ArtifactRecord,
    pub offset: usize,
    pub limit: usize,
    pub total_chunks: usize,
    pub chunks: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactStoreError {
    pub message: String,
}

impl ArtifactStoreError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ArtifactStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ArtifactStoreError {}

#[derive(Default)]
pub struct InMemoryArtifactStore {
    pub artifacts: HashMap<String, ArtifactRecord>,
    pub chunks: HashMap<String, Vec<String>>,
}

impl InMemoryArtifactStore {
    pub fn create_raw(
        &mut self,
        id: impl Into<String>,
        logical_session_id: impl Into<String>,
        channel_id: Option<String>,
        transport_session_id: Option<String>,
        source_command: impl Into<String>,
        summary: impl Into<String>,
        created_at: SystemTime,
    ) -> ArtifactRecord {
        let logical_session_id = logical_session_id.into();
        let record = ArtifactRecord {
            id: id.into(),
            content_digest: String::new(),
            logical_session_id: logical_session_id.clone(),
            transport_session_id,
            channel_id,
            session_id: logical_session_id,
            parent_id: None,
            kind: ArtifactKind::RawCommandOutput,
            source_command: Some(source_command.into()),
            filter: None,
            created_at,
            last_accessed_at: created_at,
            byte_count: 0,
            line_count: 0,
            summary: summary.into(),
        };
        self.chunks.insert(record.id.clone(), Vec::new());
        self.artifacts.insert(record.id.clone(), record.clone());
        record
    }

    pub fn append_chunk(&mut self, artifact_id: &str, chunk: impl Into<String>) {
        let chunk = chunk.into();
        self.chunks
            .entry(artifact_id.to_string())
            .or_default()
            .push(chunk.clone());
        if let Some(record) = self.artifacts.get_mut(artifact_id) {
            record.line_count += 1;
            record.byte_count += chunk.as_bytes().len() as u64;
        }
    }

    pub fn read_chunks(&self, artifact_id: &str, offset: usize, limit: usize) -> Vec<String> {
        self.chunks
            .get(artifact_id)
            .map(|all| all.iter().skip(offset).take(limit).cloned().collect())
            .unwrap_or_default()
    }

    pub fn derive_with_keyword(
        &mut self,
        id: impl Into<String>,
        parent_id: impl Into<String>,
        keyword: impl Into<String>,
        created_at: SystemTime,
    ) -> Option<ArtifactRecord> {
        self.derive_with_pattern(
            id,
            parent_id,
            keyword,
            ArtifactRefineMode::Keyword,
            false,
            created_at,
        )
        .ok()
        .flatten()
    }

    pub fn derive_with_pattern(
        &mut self,
        id: impl Into<String>,
        parent_id: impl Into<String>,
        pattern: impl Into<String>,
        mode: ArtifactRefineMode,
        ignore_case: bool,
        created_at: SystemTime,
    ) -> Result<Option<ArtifactRecord>, String> {
        let parent_id = parent_id.into();
        let pattern = pattern.into();
        let Some(parent) = self.artifacts.get(&parent_id) else {
            return Ok(None);
        };
        let parent_chunks = self.chunks.get(&parent_id).cloned().unwrap_or_default();
        let effective_mode = select_refine_mode(mode, &pattern);
        let matcher = build_matcher(&pattern, effective_mode, ignore_case)?;

        let derived_id = id.into();
        let filtered: Vec<String> = parent_chunks
            .into_iter()
            .filter(|line| matcher.is_match(line))
            .collect();
        let filter = filter_for_mode(effective_mode, &pattern);
        let mode_label = match effective_mode {
            ArtifactRefineMode::Keyword => "keyword",
            ArtifactRefineMode::Regex => "regex",
            ArtifactRefineMode::Auto => "auto",
        };
        let case_label = if ignore_case { " (ignore_case)" } else { "" };
        let record = ArtifactRecord {
            id: derived_id.clone(),
            content_digest: String::new(),
            logical_session_id: parent.logical_session_id.clone(),
            transport_session_id: parent.transport_session_id.clone(),
            channel_id: parent.channel_id.clone(),
            session_id: parent.session_id.clone(),
            parent_id: Some(parent_id),
            kind: ArtifactKind::DerivedView,
            source_command: parent.source_command.clone(),
            filter: Some(filter),
            created_at,
            last_accessed_at: created_at,
            byte_count: filtered
                .iter()
                .map(|line| line.as_bytes().len() as u64)
                .sum(),
            line_count: filtered.len(),
            summary: format!("filtered by {mode_label}: {pattern}{case_label}"),
        };

        self.chunks.insert(derived_id.clone(), filtered);
        self.artifacts.insert(derived_id, record.clone());
        Ok(Some(record))
    }
}

pub struct ArtifactStore {
    inner: ArtifactStoreInner,
}

enum ArtifactStoreInner {
    Memory(MemoryArtifactBackend),
    Filesystem(FilesystemArtifactBackend),
}

impl ArtifactStore {
    pub fn new(config: ArtifactStoreConfig) -> Result<Self, ArtifactStoreError> {
        let inner = match config.backend {
            ArtifactCacheBackend::Memory => {
                ArtifactStoreInner::Memory(MemoryArtifactBackend::new(config))
            }
            ArtifactCacheBackend::Filesystem => {
                ArtifactStoreInner::Filesystem(FilesystemArtifactBackend::new(config)?)
            }
        };
        Ok(Self { inner })
    }

    pub fn memory() -> Self {
        Self {
            inner: ArtifactStoreInner::Memory(MemoryArtifactBackend::new(
                ArtifactStoreConfig::default(),
            )),
        }
    }

    pub fn usage(&self) -> ArtifactStoreUsage {
        match &self.inner {
            ArtifactStoreInner::Memory(store) => store.usage(),
            ArtifactStoreInner::Filesystem(store) => store.usage(),
        }
    }

    pub fn ingest_raw_artifact(
        &mut self,
        template: ArtifactRecord,
        chunks: Vec<String>,
    ) -> Result<ArtifactRecord, ArtifactStoreError> {
        match &mut self.inner {
            ArtifactStoreInner::Memory(store) => store.ingest_raw_artifact(template, chunks),
            ArtifactStoreInner::Filesystem(store) => store.ingest_raw_artifact(template, chunks),
        }
    }

    pub fn read_artifact(
        &mut self,
        artifact_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<ArtifactReadResult, ArtifactStoreError> {
        match &mut self.inner {
            ArtifactStoreInner::Memory(store) => store.read_artifact(artifact_id, offset, limit),
            ArtifactStoreInner::Filesystem(store) => {
                store.read_artifact(artifact_id, offset, limit)
            }
        }
    }

    pub fn read_chunks(
        &mut self,
        artifact_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<String>, ArtifactStoreError> {
        self.read_artifact(artifact_id, offset, limit)
            .map(|view| view.chunks)
    }

    pub fn get_artifact(
        &mut self,
        artifact_id: &str,
    ) -> Result<Option<ArtifactRecord>, ArtifactStoreError> {
        match &mut self.inner {
            ArtifactStoreInner::Memory(store) => store.get_artifact(artifact_id),
            ArtifactStoreInner::Filesystem(store) => store.get_artifact(artifact_id),
        }
    }

    pub fn refine_artifact(
        &mut self,
        source_artifact_id: &str,
        pattern: &str,
        mode: ArtifactRefineMode,
        ignore_case: bool,
        created_at: SystemTime,
    ) -> Result<ArtifactRecord, ArtifactStoreError> {
        match &mut self.inner {
            ArtifactStoreInner::Memory(store) => {
                store.refine_artifact(source_artifact_id, pattern, mode, ignore_case, created_at)
            }
            ArtifactStoreInner::Filesystem(store) => {
                store.refine_artifact(source_artifact_id, pattern, mode, ignore_case, created_at)
            }
        }
    }
}

struct MemoryArtifactBackend {
    config: ArtifactStoreConfig,
    artifacts: HashMap<String, StoredArtifact>,
    used_bytes: u64,
}

impl MemoryArtifactBackend {
    fn new(config: ArtifactStoreConfig) -> Self {
        Self {
            config,
            artifacts: HashMap::new(),
            used_bytes: 0,
        }
    }

    fn usage(&self) -> ArtifactStoreUsage {
        ArtifactStoreUsage {
            backend: self.config.backend.clone(),
            root: None,
            max_bytes: self.config.max_bytes,
            used_bytes: self.used_bytes,
            artifact_count: self.artifacts.len(),
            eviction_policy: self.config.eviction_policy.clone(),
        }
    }

    fn ingest_raw_artifact(
        &mut self,
        template: ArtifactRecord,
        chunks: Vec<String>,
    ) -> Result<ArtifactRecord, ArtifactStoreError> {
        let persisted = finalize_persisted_artifact(template, chunks)?;
        let artifact_id = persisted.record.id.clone();
        let stored = StoredArtifact::from_persisted(persisted)?;
        self.insert_with_limit(stored, &[artifact_id.clone()])?;
        self.artifacts
            .get(&artifact_id)
            .map(|artifact| artifact.record.clone())
            .ok_or_else(|| ArtifactStoreError::new("artifact missing after insert"))
    }

    fn read_artifact(
        &mut self,
        artifact_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<ArtifactReadResult, ArtifactStoreError> {
        let artifact = self
            .artifacts
            .get_mut(artifact_id)
            .ok_or_else(|| ArtifactStoreError::new(format!("artifact not found: {artifact_id}")))?;
        artifact.record.last_accessed_at = SystemTime::now();
        Ok(ArtifactReadResult {
            record: artifact.record.clone(),
            offset,
            limit,
            total_chunks: artifact.chunks.len(),
            chunks: artifact
                .chunks
                .iter()
                .skip(offset)
                .take(limit)
                .cloned()
                .collect(),
        })
    }

    fn get_artifact(
        &mut self,
        artifact_id: &str,
    ) -> Result<Option<ArtifactRecord>, ArtifactStoreError> {
        let Some(artifact) = self.artifacts.get_mut(artifact_id) else {
            return Ok(None);
        };
        artifact.record.last_accessed_at = SystemTime::now();
        Ok(Some(artifact.record.clone()))
    }

    fn refine_artifact(
        &mut self,
        source_artifact_id: &str,
        pattern: &str,
        mode: ArtifactRefineMode,
        ignore_case: bool,
        created_at: SystemTime,
    ) -> Result<ArtifactRecord, ArtifactStoreError> {
        let source = self
            .artifacts
            .get(source_artifact_id)
            .cloned()
            .ok_or_else(|| ArtifactStoreError::new("source artifact not found"))?;
        let filtered = refine_chunks(&source.chunks, pattern, mode, ignore_case)?;
        let template = derived_template(&source.record, pattern, mode, ignore_case, created_at);
        let persisted = finalize_persisted_artifact(template, filtered)?;
        let artifact_id = persisted.record.id.clone();
        self.insert_with_limit(
            stored_from_persisted(persisted)?,
            &[artifact_id.clone(), source.record.id.clone()],
        )?;
        self.artifacts
            .get(&artifact_id)
            .map(|artifact| artifact.record.clone())
            .ok_or_else(|| ArtifactStoreError::new("artifact missing after refine"))
    }

    fn insert_with_limit(
        &mut self,
        artifact: StoredArtifact,
        protected_ids: &[String],
    ) -> Result<(), ArtifactStoreError> {
        let artifact_id = artifact.record.id.clone();
        let replaced = self.artifacts.insert(artifact_id.clone(), artifact);
        if let Some(previous) = replaced {
            self.used_bytes = self.used_bytes.saturating_sub(previous.persisted_bytes);
        }
        if let Some(current) = self.artifacts.get(&artifact_id) {
            self.used_bytes += current.persisted_bytes;
        }
        if let Err(err) = self.enforce_limit(protected_ids) {
            if let Some(removed) = self.artifacts.remove(&artifact_id) {
                self.used_bytes = self.used_bytes.saturating_sub(removed.persisted_bytes);
            }
            return Err(err);
        }
        Ok(())
    }

    fn enforce_limit(&mut self, protected_ids: &[String]) -> Result<(), ArtifactStoreError> {
        if self.config.max_bytes == 0 {
            return Ok(());
        }
        let protected = protected_ids.iter().cloned().collect::<HashSet<_>>();
        while self.used_bytes > self.config.max_bytes {
            let Some(candidate_id) = self
                .artifacts
                .values()
                .filter(|artifact| !protected.contains(&artifact.record.id))
                .min_by_key(|artifact| artifact.record.last_accessed_at)
                .map(|artifact| artifact.record.id.clone())
            else {
                return Err(ArtifactStoreError::new(
                    "artifact cache exceeded max_bytes and no evictable artifact remained",
                ));
            };
            if let Some(removed) = self.artifacts.remove(&candidate_id) {
                self.used_bytes = self.used_bytes.saturating_sub(removed.persisted_bytes);
            }
        }
        Ok(())
    }
}

struct FilesystemArtifactBackend {
    config: ArtifactStoreConfig,
    index: HashMap<String, ArtifactIndexEntry>,
    used_bytes: u64,
}

impl FilesystemArtifactBackend {
    fn new(config: ArtifactStoreConfig) -> Result<Self, ArtifactStoreError> {
        fs::create_dir_all(&config.root).map_err(|err| {
            ArtifactStoreError::new(format!("create artifact root failed: {err}"))
        })?;
        let mut backend = Self {
            config,
            index: HashMap::new(),
            used_bytes: 0,
        };
        backend.load_existing()?;
        Ok(backend)
    }

    fn usage(&self) -> ArtifactStoreUsage {
        ArtifactStoreUsage {
            backend: self.config.backend.clone(),
            root: Some(self.config.root.clone()),
            max_bytes: self.config.max_bytes,
            used_bytes: self.used_bytes,
            artifact_count: self.index.len(),
            eviction_policy: self.config.eviction_policy.clone(),
        }
    }

    fn ingest_raw_artifact(
        &mut self,
        template: ArtifactRecord,
        chunks: Vec<String>,
    ) -> Result<ArtifactRecord, ArtifactStoreError> {
        let persisted = finalize_persisted_artifact(template, chunks)?;
        let artifact_id = persisted.record.id.clone();
        self.write_with_limit(persisted, &[artifact_id.clone()])?;
        self.index
            .get(&artifact_id)
            .map(|entry| entry.record.clone())
            .ok_or_else(|| ArtifactStoreError::new("artifact missing after persist"))
    }

    fn read_artifact(
        &mut self,
        artifact_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<ArtifactReadResult, ArtifactStoreError> {
        let mut persisted = self.read_persisted_artifact(artifact_id)?;
        persisted.record.last_accessed_at = SystemTime::now();
        self.persist_artifact(&persisted)?;
        Ok(ArtifactReadResult {
            record: persisted.record,
            offset,
            limit,
            total_chunks: persisted.chunks.len(),
            chunks: persisted
                .chunks
                .iter()
                .skip(offset)
                .take(limit)
                .cloned()
                .collect(),
        })
    }

    fn get_artifact(
        &mut self,
        artifact_id: &str,
    ) -> Result<Option<ArtifactRecord>, ArtifactStoreError> {
        if !self.index.contains_key(artifact_id) {
            return Ok(None);
        }
        let mut persisted = self.read_persisted_artifact(artifact_id)?;
        persisted.record.last_accessed_at = SystemTime::now();
        self.persist_artifact(&persisted)?;
        Ok(Some(persisted.record))
    }

    fn refine_artifact(
        &mut self,
        source_artifact_id: &str,
        pattern: &str,
        mode: ArtifactRefineMode,
        ignore_case: bool,
        created_at: SystemTime,
    ) -> Result<ArtifactRecord, ArtifactStoreError> {
        let mut source = self.read_persisted_artifact(source_artifact_id)?;
        source.record.last_accessed_at = SystemTime::now();
        self.persist_artifact(&source)?;
        let filtered = refine_chunks(&source.chunks, pattern, mode, ignore_case)?;
        let template = derived_template(&source.record, pattern, mode, ignore_case, created_at);
        let persisted = finalize_persisted_artifact(template, filtered)?;
        let artifact_id = persisted.record.id.clone();
        self.write_with_limit(persisted, &[artifact_id.clone(), source.record.id.clone()])?;
        self.index
            .get(&artifact_id)
            .map(|entry| entry.record.clone())
            .ok_or_else(|| ArtifactStoreError::new("artifact missing after refine"))
    }

    fn load_existing(&mut self) -> Result<(), ArtifactStoreError> {
        self.index.clear();
        self.used_bytes = 0;
        for entry in fs::read_dir(&self.config.root)
            .map_err(|err| ArtifactStoreError::new(format!("list artifact root failed: {err}")))?
        {
            let entry = entry.map_err(|err| {
                ArtifactStoreError::new(format!("read artifact dir entry failed: {err}"))
            })?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let persisted = read_persisted_from_path(&path)?;
            let size = entry
                .metadata()
                .map_err(|err| {
                    ArtifactStoreError::new(format!(
                        "read artifact metadata failed for {}: {err}",
                        path.display()
                    ))
                })?
                .len();
            self.index.insert(
                persisted.record.id.clone(),
                ArtifactIndexEntry {
                    record: persisted.record,
                    persisted_bytes: size,
                },
            );
            self.used_bytes += size;
        }
        Ok(())
    }

    fn write_with_limit(
        &mut self,
        persisted: PersistedArtifact,
        protected_ids: &[String],
    ) -> Result<(), ArtifactStoreError> {
        let artifact_id = persisted.record.id.clone();
        self.persist_artifact(&persisted)?;
        if let Err(err) = self.enforce_limit(protected_ids) {
            let _ = self.remove_artifact(&artifact_id);
            return Err(err);
        }
        Ok(())
    }

    fn enforce_limit(&mut self, protected_ids: &[String]) -> Result<(), ArtifactStoreError> {
        if self.config.max_bytes == 0 {
            return Ok(());
        }
        let protected = protected_ids.iter().cloned().collect::<HashSet<_>>();
        while self.used_bytes > self.config.max_bytes {
            let Some(candidate_id) = self
                .index
                .values()
                .filter(|entry| !protected.contains(&entry.record.id))
                .min_by_key(|entry| entry.record.last_accessed_at)
                .map(|entry| entry.record.id.clone())
            else {
                return Err(ArtifactStoreError::new(
                    "artifact cache exceeded max_bytes and no evictable artifact remained",
                ));
            };
            self.remove_artifact(&candidate_id)?;
        }
        Ok(())
    }

    fn persist_artifact(
        &mut self,
        persisted: &PersistedArtifact,
    ) -> Result<(), ArtifactStoreError> {
        let bytes = serde_json::to_vec(persisted)
            .map_err(|err| ArtifactStoreError::new(format!("serialize artifact failed: {err}")))?;
        let path = self.path_for(&persisted.record.id);
        fs::write(&path, &bytes).map_err(|err| {
            ArtifactStoreError::new(format!("write artifact {} failed: {err}", path.display()))
        })?;
        let new_size = bytes.len() as u64;
        let old_size = self
            .index
            .get(&persisted.record.id)
            .map(|entry| entry.persisted_bytes)
            .unwrap_or(0);
        self.used_bytes = self.used_bytes + new_size - old_size;
        self.index.insert(
            persisted.record.id.clone(),
            ArtifactIndexEntry {
                record: persisted.record.clone(),
                persisted_bytes: new_size,
            },
        );
        Ok(())
    }

    fn remove_artifact(&mut self, artifact_id: &str) -> Result<(), ArtifactStoreError> {
        let path = self.path_for(artifact_id);
        if path.exists() {
            fs::remove_file(&path).map_err(|err| {
                ArtifactStoreError::new(format!("remove artifact {} failed: {err}", path.display()))
            })?;
        }
        if let Some(entry) = self.index.remove(artifact_id) {
            self.used_bytes = self.used_bytes.saturating_sub(entry.persisted_bytes);
        }
        Ok(())
    }

    fn read_persisted_artifact(
        &self,
        artifact_id: &str,
    ) -> Result<PersistedArtifact, ArtifactStoreError> {
        if !self.index.contains_key(artifact_id) {
            return Err(ArtifactStoreError::new(format!(
                "artifact not found: {artifact_id}"
            )));
        }
        read_persisted_from_path(&self.path_for(artifact_id))
    }

    fn path_for(&self, artifact_id: &str) -> PathBuf {
        self.config.root.join(format!("{artifact_id}.json"))
    }
}

#[derive(Clone)]
struct StoredArtifact {
    record: ArtifactRecord,
    chunks: Vec<String>,
    persisted_bytes: u64,
}

impl StoredArtifact {
    fn from_persisted(persisted: PersistedArtifact) -> Result<Self, ArtifactStoreError> {
        let persisted_bytes = serialized_len(&persisted)?;
        Ok(Self {
            record: persisted.record,
            chunks: persisted.chunks,
            persisted_bytes,
        })
    }
}

fn stored_from_persisted(
    persisted: PersistedArtifact,
) -> Result<StoredArtifact, ArtifactStoreError> {
    StoredArtifact::from_persisted(persisted)
}

#[derive(Clone)]
struct ArtifactIndexEntry {
    record: ArtifactRecord,
    persisted_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PersistedArtifact {
    record: ArtifactRecord,
    chunks: Vec<String>,
}

#[derive(Serialize)]
struct ArtifactIdentityManifest<'a> {
    kind: &'a ArtifactKind,
    logical_session_id: &'a str,
    transport_session_id: &'a Option<String>,
    channel_id: &'a Option<String>,
    session_id: &'a str,
    parent_id: &'a Option<String>,
    source_command: &'a Option<String>,
    filter: &'a Option<ArtifactFilter>,
    created_at_ms: u128,
    content_digest: &'a str,
}

fn finalize_persisted_artifact(
    template: ArtifactRecord,
    chunks: Vec<String>,
) -> Result<PersistedArtifact, ArtifactStoreError> {
    let content_digest = digest_chunks(&chunks);
    let byte_count = chunks
        .iter()
        .map(|line| line.as_bytes().len() as u64)
        .sum::<u64>();
    let line_count = chunks.len();
    let id = build_artifact_id(
        &template.kind,
        &template.logical_session_id,
        &template.transport_session_id,
        &template.channel_id,
        &template.session_id,
        &template.parent_id,
        &template.source_command,
        &template.filter,
        template.created_at,
        &content_digest,
    )?;
    Ok(PersistedArtifact {
        record: ArtifactRecord {
            id,
            content_digest,
            logical_session_id: template.logical_session_id,
            transport_session_id: template.transport_session_id,
            channel_id: template.channel_id,
            session_id: template.session_id,
            parent_id: template.parent_id,
            kind: template.kind,
            source_command: template.source_command,
            filter: template.filter,
            created_at: template.created_at,
            last_accessed_at: template.last_accessed_at,
            byte_count,
            line_count,
            summary: template.summary,
        },
        chunks,
    })
}

fn build_artifact_id(
    kind: &ArtifactKind,
    logical_session_id: &str,
    transport_session_id: &Option<String>,
    channel_id: &Option<String>,
    session_id: &str,
    parent_id: &Option<String>,
    source_command: &Option<String>,
    filter: &Option<ArtifactFilter>,
    created_at: SystemTime,
    content_digest: &str,
) -> Result<String, ArtifactStoreError> {
    let manifest = ArtifactIdentityManifest {
        kind,
        logical_session_id,
        transport_session_id,
        channel_id,
        session_id,
        parent_id,
        source_command,
        filter,
        created_at_ms: created_at
            .duration_since(UNIX_EPOCH)
            .map_err(|err| ArtifactStoreError::new(format!("invalid created_at: {err}")))?
            .as_millis(),
        content_digest,
    };
    let manifest_bytes = serde_json::to_vec(&manifest)
        .map_err(|err| ArtifactStoreError::new(format!("serialize manifest failed: {err}")))?;
    Ok(format!("art-{}", sha256_hex(&manifest_bytes)))
}

fn serialized_len(persisted: &PersistedArtifact) -> Result<u64, ArtifactStoreError> {
    serde_json::to_vec(persisted)
        .map(|bytes| bytes.len() as u64)
        .map_err(|err| ArtifactStoreError::new(format!("serialize artifact failed: {err}")))
}

fn read_persisted_from_path(path: &Path) -> Result<PersistedArtifact, ArtifactStoreError> {
    let bytes = fs::read(path).map_err(|err| {
        ArtifactStoreError::new(format!("read artifact {} failed: {err}", path.display()))
    })?;
    serde_json::from_slice(&bytes).map_err(|err| {
        ArtifactStoreError::new(format!("parse artifact {} failed: {err}", path.display()))
    })
}

fn digest_chunks(chunks: &[String]) -> String {
    let mut hasher = Sha256::new();
    for chunk in chunks {
        hasher.update(chunk.as_bytes());
        hasher.update(b"\n");
    }
    sha256_finalize(hasher)
}

fn sha256_hex(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    sha256_finalize(hasher)
}

fn sha256_finalize(hasher: Sha256) -> String {
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

fn derived_template(
    source: &ArtifactRecord,
    pattern: &str,
    mode: ArtifactRefineMode,
    ignore_case: bool,
    created_at: SystemTime,
) -> ArtifactRecord {
    let effective_mode = select_refine_mode(mode, pattern);
    let mode_label = match effective_mode {
        ArtifactRefineMode::Keyword => "keyword",
        ArtifactRefineMode::Regex => "regex",
        ArtifactRefineMode::Auto => "auto",
    };
    let case_label = if ignore_case { " (ignore_case)" } else { "" };
    ArtifactRecord {
        id: String::new(),
        content_digest: String::new(),
        logical_session_id: source.logical_session_id.clone(),
        transport_session_id: source.transport_session_id.clone(),
        channel_id: source.channel_id.clone(),
        session_id: source.session_id.clone(),
        parent_id: Some(source.id.clone()),
        kind: ArtifactKind::DerivedView,
        source_command: source.source_command.clone(),
        filter: Some(filter_for_mode(effective_mode, pattern)),
        created_at,
        last_accessed_at: created_at,
        byte_count: 0,
        line_count: 0,
        summary: format!("filtered by {mode_label}: {pattern}{case_label}"),
    }
}

fn filter_for_mode(mode: ArtifactRefineMode, pattern: &str) -> ArtifactFilter {
    match mode {
        ArtifactRefineMode::Keyword => ArtifactFilter {
            keyword: Some(pattern.to_string()),
            regex: None,
            line_start: None,
            line_end: None,
        },
        ArtifactRefineMode::Regex | ArtifactRefineMode::Auto => ArtifactFilter {
            keyword: None,
            regex: Some(pattern.to_string()),
            line_start: None,
            line_end: None,
        },
    }
}

fn refine_chunks(
    source_chunks: &[String],
    pattern: &str,
    mode: ArtifactRefineMode,
    ignore_case: bool,
) -> Result<Vec<String>, ArtifactStoreError> {
    let effective_mode = select_refine_mode(mode, pattern);
    let matcher =
        build_matcher(pattern, effective_mode, ignore_case).map_err(ArtifactStoreError::new)?;
    Ok(source_chunks
        .iter()
        .filter(|line| matcher.is_match(line))
        .cloned()
        .collect())
}

enum EffectiveLineMatcher {
    Keyword { needle: String, ignore_case: bool },
    Regex(Regex),
}

impl EffectiveLineMatcher {
    fn is_match(&self, line: &str) -> bool {
        match self {
            Self::Keyword {
                needle,
                ignore_case,
            } => {
                if *ignore_case {
                    line.to_ascii_lowercase().contains(needle)
                } else {
                    line.contains(needle)
                }
            }
            Self::Regex(regex) => regex.is_match(line),
        }
    }
}

fn build_matcher(
    pattern: &str,
    mode: ArtifactRefineMode,
    ignore_case: bool,
) -> Result<EffectiveLineMatcher, String> {
    match mode {
        ArtifactRefineMode::Keyword => {
            let needle = if ignore_case {
                pattern.to_ascii_lowercase()
            } else {
                pattern.to_string()
            };
            Ok(EffectiveLineMatcher::Keyword {
                needle,
                ignore_case,
            })
        }
        ArtifactRefineMode::Regex => RegexBuilder::new(pattern)
            .case_insensitive(ignore_case)
            .build()
            .map(EffectiveLineMatcher::Regex)
            .map_err(|err| format!("invalid regex pattern: {err}")),
        ArtifactRefineMode::Auto => Err("internal error: auto mode unresolved".to_string()),
    }
}

fn select_refine_mode(mode: ArtifactRefineMode, pattern: &str) -> ArtifactRefineMode {
    match mode {
        ArtifactRefineMode::Auto => {
            if looks_like_regex(pattern) {
                ArtifactRefineMode::Regex
            } else {
                ArtifactRefineMode::Keyword
            }
        }
        fixed => fixed,
    }
}

fn looks_like_regex(pattern: &str) -> bool {
    pattern.chars().any(|ch| {
        matches!(
            ch,
            '|' | '.' | '^' | '$' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '\\'
        )
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use bridgingio_domain::{ArtifactKind, ArtifactRecord};

    use super::{
        ArtifactCacheBackend, ArtifactEvictionPolicy, ArtifactReadResult, ArtifactRefineMode,
        ArtifactStore, ArtifactStoreConfig, InMemoryArtifactStore,
    };

    fn temp_dir(prefix: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path =
            std::path::PathBuf::from("/tmp").join(format!("bridgingio-artifacts-{prefix}-{stamp}"));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    fn raw_template(
        logical_session_id: &str,
        channel_id: Option<&str>,
        transport_session_id: Option<&str>,
        source_command: &str,
        created_at: SystemTime,
    ) -> ArtifactRecord {
        ArtifactRecord {
            id: String::new(),
            content_digest: String::new(),
            logical_session_id: logical_session_id.to_string(),
            transport_session_id: transport_session_id.map(ToString::to_string),
            channel_id: channel_id.map(ToString::to_string),
            session_id: logical_session_id.to_string(),
            parent_id: None,
            kind: ArtifactKind::RawCommandOutput,
            source_command: Some(source_command.to_string()),
            filter: None,
            created_at,
            last_accessed_at: created_at,
            byte_count: 0,
            line_count: 0,
            summary: "raw logs".to_string(),
        }
    }

    #[test]
    fn supports_parent_child_and_chunk_reads() {
        let mut store = InMemoryArtifactStore::default();
        let now = SystemTime::now();
        let raw = store.create_raw(
            "a-raw",
            "ls-1",
            Some("ch-1".into()),
            Some("ts-1".into()),
            "dmesg -w",
            "raw logs",
            now,
        );

        store.append_chunk(&raw.id, "INFO boot");
        store.append_chunk(&raw.id, "WARN thermal");
        store.append_chunk(&raw.id, "ERROR panic");

        let preview = store.read_chunks(&raw.id, 1, 1);
        assert_eq!(preview, vec!["WARN thermal".to_string()]);

        let derived = store
            .derive_with_keyword("a-derived", &raw.id, "ERROR", now)
            .expect("derive artifact");

        assert_eq!(derived.parent_id.as_deref(), Some("a-raw"));
        let filtered = store.read_chunks("a-derived", 0, 10);
        assert_eq!(filtered, vec!["ERROR panic".to_string()]);
    }

    #[test]
    fn supports_regex_like_refine_with_ignore_case() {
        let mut store = InMemoryArtifactStore::default();
        let now = SystemTime::now();
        let raw = store.create_raw(
            "a-raw",
            "ls-1",
            Some("ch-1".into()),
            Some("ts-1".into()),
            "logcat -b all",
            "raw logs",
            now,
        );
        store.append_chunk(&raw.id, "system_server: boot");
        store.append_chunk(&raw.id, "Netd: ready");
        store.append_chunk(&raw.id, "surfaceflinger: frame");

        let derived = store
            .derive_with_pattern(
                "a-derived-regex",
                &raw.id,
                "system_server|netd",
                ArtifactRefineMode::Regex,
                true,
                now,
            )
            .expect("valid regex")
            .expect("derived exists");

        assert_eq!(derived.parent_id.as_deref(), Some("a-raw"));
        assert_eq!(
            derived.filter.as_ref().and_then(|f| f.regex.as_deref()),
            Some("system_server|netd")
        );
        let filtered = store.read_chunks("a-derived-regex", 0, 10);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().any(|line| line.contains("system_server")));
        assert!(filtered.iter().any(|line| line.contains("Netd")));
    }

    #[test]
    fn auto_mode_detects_regex_or_keyword() {
        let mut store = InMemoryArtifactStore::default();
        let now = SystemTime::now();
        let raw = store.create_raw(
            "a-raw",
            "ls-1",
            Some("ch-1".into()),
            Some("ts-1".into()),
            "dmesg -w",
            "raw logs",
            now,
        );
        store.append_chunk(&raw.id, "ERROR panic");
        store.append_chunk(&raw.id, "WARN thermal");

        let auto_keyword = store
            .derive_with_pattern(
                "a-derived-auto-keyword",
                &raw.id,
                "ERROR",
                ArtifactRefineMode::Auto,
                false,
                now,
            )
            .expect("auto keyword")
            .expect("record");
        assert_eq!(
            auto_keyword
                .filter
                .as_ref()
                .and_then(|filter| filter.keyword.as_deref()),
            Some("ERROR")
        );

        let auto_regex = store
            .derive_with_pattern(
                "a-derived-auto-regex",
                &raw.id,
                "ERROR|WARN",
                ArtifactRefineMode::Auto,
                false,
                now,
            )
            .expect("auto regex")
            .expect("record");
        assert_eq!(
            auto_regex
                .filter
                .as_ref()
                .and_then(|filter| filter.regex.as_deref()),
            Some("ERROR|WARN")
        );
    }

    #[test]
    fn canonical_store_assigns_hash_like_id_and_content_digest() {
        let mut store = ArtifactStore::memory();
        let now = SystemTime::now();
        let artifact = store
            .ingest_raw_artifact(
                raw_template("ls-1", Some("ch-1"), Some("ts-1"), "printf 'hello\\n'", now),
                vec!["hello".into()],
            )
            .expect("ingest");
        assert!(artifact.id.starts_with("art-"));
        assert_eq!(artifact.id.len(), 68);
        assert_eq!(artifact.line_count, 1);
        assert!(!artifact.content_digest.is_empty());
    }

    #[test]
    fn filesystem_backend_persists_and_refines_after_restart() {
        let root = temp_dir("persistence");
        let config = ArtifactStoreConfig {
            backend: ArtifactCacheBackend::Filesystem,
            root: root.clone(),
            max_bytes: 1024 * 1024,
            eviction_policy: ArtifactEvictionPolicy::Lru,
        };
        let now = SystemTime::now();
        let source_id = {
            let mut store = ArtifactStore::new(config.clone()).expect("create store");
            let raw = store
                .ingest_raw_artifact(
                    raw_template("ls-1", Some("ch-1"), Some("ts-1"), "logcat -b all", now),
                    vec!["system_server: boot".into(), "netd: ready".into()],
                )
                .expect("ingest");
            raw.id
        };

        let mut restarted = ArtifactStore::new(config).expect("restart store");
        let persisted = restarted
            .read_artifact(&source_id, 0, 10)
            .expect("read after restart");
        assert_eq!(persisted.record.id, source_id);
        assert_eq!(persisted.total_chunks, 2);

        let derived = restarted
            .refine_artifact(
                &source_id,
                "netd",
                ArtifactRefineMode::Keyword,
                false,
                SystemTime::now(),
            )
            .expect("refine after restart");
        let derived_read = restarted
            .read_artifact(&derived.id, 0, 10)
            .expect("read derived");
        assert_eq!(derived_read.chunks, vec!["netd: ready".to_string()]);
    }

    #[test]
    fn filesystem_backend_evicts_lru_when_limit_is_exceeded() {
        let root = temp_dir("evict");
        let config = ArtifactStoreConfig {
            backend: ArtifactCacheBackend::Filesystem,
            root,
            max_bytes: 1500,
            eviction_policy: ArtifactEvictionPolicy::Lru,
        };
        let mut store = ArtifactStore::new(config).expect("create store");
        let now = SystemTime::now();
        let first = store
            .ingest_raw_artifact(
                raw_template("ls-1", Some("ch-1"), Some("ts-1"), "first", now),
                vec!["A".repeat(220)],
            )
            .expect("ingest first");
        let _ = store.read_artifact(&first.id, 0, 10).expect("touch first");
        let second = store
            .ingest_raw_artifact(
                raw_template("ls-1", Some("ch-2"), Some("ts-1"), "second", now),
                vec!["B".repeat(220)],
            )
            .expect("ingest second");
        let _ = store
            .read_artifact(&second.id, 0, 10)
            .expect("touch second");
        let third = store
            .ingest_raw_artifact(
                raw_template("ls-1", Some("ch-3"), Some("ts-1"), "third", now),
                vec!["C".repeat(220)],
            )
            .expect("ingest third");
        let usage = store.usage();
        assert!(usage.used_bytes <= usage.max_bytes);
        assert!(store.get_artifact(&third.id).expect("get third").is_some());
        assert!(store.get_artifact(&first.id).expect("get first").is_none());
    }

    #[test]
    fn stable_hash_lookup_returns_metadata_and_chunks() {
        let mut store = ArtifactStore::memory();
        let now = SystemTime::now();
        let raw = store
            .ingest_raw_artifact(
                raw_template("ls-lookup", Some("ch-9"), Some("ts-9"), "dmesg -w", now),
                vec!["INFO boot".into(), "ERROR panic".into()],
            )
            .expect("ingest");
        let ArtifactReadResult { record, chunks, .. } =
            store.read_artifact(&raw.id, 0, 20).expect("read by hash");
        assert_eq!(record.source_command.as_deref(), Some("dmesg -w"));
        assert_eq!(record.channel_id.as_deref(), Some("ch-9"));
        assert_eq!(chunks.len(), 2);
    }
}
