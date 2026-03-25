use std::collections::HashMap;
use std::time::SystemTime;

use bridgingio_domain::{ArtifactFilter, ArtifactKind, ArtifactRecord};

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
            logical_session_id: logical_session_id.clone(),
            transport_session_id,
            channel_id,
            session_id: logical_session_id,
            parent_id: None,
            kind: ArtifactKind::RawCommandOutput,
            source_command: Some(source_command.into()),
            filter: None,
            created_at,
            summary: summary.into(),
        };
        self.chunks.insert(record.id.clone(), Vec::new());
        self.artifacts.insert(record.id.clone(), record.clone());
        record
    }

    pub fn append_chunk(&mut self, artifact_id: &str, chunk: impl Into<String>) {
        self.chunks
            .entry(artifact_id.to_string())
            .or_default()
            .push(chunk.into());
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
        let parent_id = parent_id.into();
        let keyword = keyword.into();
        let parent = self.artifacts.get(&parent_id)?;
        let parent_chunks = self.chunks.get(&parent_id).cloned().unwrap_or_default();

        let derived_id = id.into();
        let filtered: Vec<String> = parent_chunks
            .into_iter()
            .filter(|line| line.contains(&keyword))
            .collect();

        let record = ArtifactRecord {
            id: derived_id.clone(),
            logical_session_id: parent.logical_session_id.clone(),
            transport_session_id: parent.transport_session_id.clone(),
            channel_id: parent.channel_id.clone(),
            session_id: parent.session_id.clone(),
            parent_id: Some(parent_id),
            kind: ArtifactKind::DerivedView,
            source_command: parent.source_command.clone(),
            filter: Some(ArtifactFilter {
                keyword: Some(keyword.clone()),
                regex: None,
                line_start: None,
                line_end: None,
            }),
            created_at,
            summary: format!("filtered by keyword: {keyword}"),
        };

        self.chunks.insert(derived_id.clone(), filtered);
        self.artifacts.insert(derived_id, record.clone());
        Some(record)
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::InMemoryArtifactStore;

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
}
