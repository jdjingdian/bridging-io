use std::collections::HashMap;
use std::time::SystemTime;

use bridgingio_domain::{
    build_logical_session_key, AccessScope, ApprovalRequestRecord, ArtifactRecord, AuditEvent,
    ChannelKind, ChannelRecord, ChannelStatus, EnvironmentFingerprint, LogicalSessionRecord,
    LogicalSessionStatus, SessionRecord, SessionReusePolicy, TargetKind, TargetProfile,
    TransportSessionRecord, TransportSessionStatus,
};

#[derive(Default)]
pub struct InMemoryMetadataStore {
    pub profiles: HashMap<String, TargetProfile>,
    pub sessions: HashMap<String, SessionRecord>,
    pub logical_sessions: HashMap<String, LogicalSessionRecord>,
    pub logical_session_keys: HashMap<String, String>,
    pub transport_sessions: HashMap<String, TransportSessionRecord>,
    pub channels: HashMap<String, ChannelRecord>,
    pub artifacts: HashMap<String, ArtifactRecord>,
    pub fingerprints: HashMap<String, EnvironmentFingerprint>,
    pub audit_events: Vec<AuditEvent>,
    pub approvals: HashMap<String, ApprovalRequestRecord>,
    next_logical_session_seq: u64,
    next_transport_session_seq: u64,
    next_channel_seq: u64,
}

impl InMemoryMetadataStore {
    pub fn upsert_profile(&mut self, profile: TargetProfile) {
        self.profiles.insert(profile.id.clone(), profile);
    }

    pub fn upsert_session(&mut self, session: SessionRecord) {
        self.sessions.insert(session.id.clone(), session);
    }

    pub fn upsert_logical_session(&mut self, session: LogicalSessionRecord) {
        self.logical_sessions
            .insert(session.logical_session_id.clone(), session);
    }

    pub fn upsert_transport_session(&mut self, session: TransportSessionRecord) {
        self.transport_sessions
            .insert(session.transport_session_id.clone(), session);
    }

    pub fn upsert_channel(&mut self, channel: ChannelRecord) {
        self.channels.insert(channel.channel_id.clone(), channel);
    }

    pub fn upsert_artifact(&mut self, artifact: ArtifactRecord) {
        self.artifacts.insert(artifact.id.clone(), artifact);
    }

    pub fn upsert_fingerprint(&mut self, session_id: impl Into<String>, fingerprint: EnvironmentFingerprint) {
        self.fingerprints.insert(session_id.into(), fingerprint);
    }

    pub fn append_audit(&mut self, event: AuditEvent) {
        self.audit_events.push(event);
    }

    pub fn upsert_approval(&mut self, request: ApprovalRequestRecord) {
        self.approvals.insert(request.id.clone(), request);
    }

    pub fn resolve_logical_session(
        &mut self,
        scope: &AccessScope,
        target_id: &str,
        reuse_policy: SessionReusePolicy,
        now: SystemTime,
    ) -> LogicalSessionRecord {
        let session_key = build_logical_session_key(scope, target_id, &scope.client_session_id);
        if !matches!(reuse_policy, SessionReusePolicy::AlwaysNew) {
            if let Some(existing_id) = self.logical_session_keys.get(&session_key).cloned() {
                if let Some(existing) = self.logical_sessions.get_mut(&existing_id) {
                    let should_reuse = match reuse_policy {
                        SessionReusePolicy::AlwaysNew => false,
                        SessionReusePolicy::ReuseIfAlive => existing.status.is_alive(),
                        SessionReusePolicy::ResumeOrCreate => true,
                    };

                    if should_reuse {
                        existing.last_activity_at = now;
                        existing.reuse_policy = reuse_policy.clone();
                        if matches!(reuse_policy, SessionReusePolicy::ResumeOrCreate)
                            && !existing.status.is_alive()
                        {
                            existing.status = LogicalSessionStatus::Active;
                            existing.closed_at = None;
                            existing.close_reason = None;
                        }
                        return existing.clone();
                    }
                }
            }
        }

        let logical_session_id = allocate_id(&mut self.next_logical_session_seq, "ls");
        let record = LogicalSessionRecord {
            logical_session_id: logical_session_id.clone(),
            session_key: session_key.clone(),
            scope_id: scope.scope_id.clone(),
            target_id: target_id.to_string(),
            reuse_policy,
            status: LogicalSessionStatus::Active,
            created_at: now,
            last_activity_at: now,
            closed_at: None,
            close_reason: None,
        };

        self.logical_session_keys.insert(session_key, logical_session_id);
        self.logical_sessions
            .insert(record.logical_session_id.clone(), record.clone());
        record
    }

    pub fn open_transport_session(
        &mut self,
        logical_session_id: &str,
        target_id: &str,
        connector_kind: TargetKind,
        resolved_executable_path: Option<String>,
        resolved_executable_source: Option<String>,
        now: SystemTime,
    ) -> TransportSessionRecord {
        let transport_session_id = allocate_id(&mut self.next_transport_session_seq, "ts");
        let record = TransportSessionRecord {
            transport_session_id: transport_session_id.clone(),
            logical_session_id: logical_session_id.to_string(),
            target_id: target_id.to_string(),
            connector_kind,
            status: TransportSessionStatus::Connected,
            created_at: now,
            last_activity_at: now,
            close_reason: None,
            resolved_executable_path,
            resolved_executable_source,
        };

        if let Some(logical) = self.logical_sessions.get_mut(logical_session_id) {
            logical.last_activity_at = now;
            if !logical.status.is_alive() {
                logical.status = LogicalSessionStatus::Active;
                logical.closed_at = None;
                logical.close_reason = None;
            }
        }
        self.transport_sessions.insert(transport_session_id, record.clone());
        record
    }

    pub fn open_channel(
        &mut self,
        logical_session_id: &str,
        transport_session_id: &str,
        target_id: &str,
        channel_kind: ChannelKind,
        display_name: Option<String>,
        now: SystemTime,
    ) -> ChannelRecord {
        let channel_id = allocate_id(&mut self.next_channel_seq, "ch");
        let record = ChannelRecord {
            channel_id: channel_id.clone(),
            logical_session_id: logical_session_id.to_string(),
            transport_session_id: transport_session_id.to_string(),
            target_id: target_id.to_string(),
            channel_kind,
            status: ChannelStatus::Active,
            display_name,
            working_directory: None,
            foreground_command: None,
            created_at: now,
            last_activity_at: now,
            close_reason: None,
        };

        if let Some(logical) = self.logical_sessions.get_mut(logical_session_id) {
            logical.last_activity_at = now;
        }
        self.channels.insert(channel_id, record.clone());
        record
    }

    pub fn update_channel_status(
        &mut self,
        channel_id: &str,
        status: ChannelStatus,
        close_reason: Option<String>,
        now: SystemTime,
    ) -> Option<ChannelRecord> {
        let channel = self.channels.get_mut(channel_id)?;
        channel.status = status;
        channel.last_activity_at = now;
        channel.close_reason = close_reason;
        Some(channel.clone())
    }

    pub fn channels_for_logical_session(&self, logical_session_id: &str) -> Vec<ChannelRecord> {
        let mut channels: Vec<_> = self
            .channels
            .values()
            .filter(|c| c.logical_session_id == logical_session_id)
            .cloned()
            .collect();
        channels.sort_by(|a, b| a.channel_id.cmp(&b.channel_id));
        channels
    }

    pub fn close_logical_session(
        &mut self,
        logical_session_id: &str,
        reason: impl Into<String>,
        now: SystemTime,
    ) -> Option<LogicalSessionRecord> {
        let session = self.logical_sessions.get_mut(logical_session_id)?;
        session.status = LogicalSessionStatus::Closed;
        session.last_activity_at = now;
        session.closed_at = Some(now);
        session.close_reason = Some(reason.into());
        Some(session.clone())
    }
}

fn allocate_id(counter: &mut u64, prefix: &str) -> String {
    *counter += 1;
    format!("{prefix}-{:06}", *counter)
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use bridgingio_domain::{
        AccessScope, ArtifactKind, ArtifactRecord, ChannelKind, ConnectionConfig, PolicyProfile,
        SessionRecord, SessionReusePolicy, TargetKind,
    };

    use super::InMemoryMetadataStore;

    #[test]
    fn stores_profile_session_and_artifact() {
        let mut store = InMemoryMetadataStore::default();
        store.upsert_profile(bridgingio_domain::TargetProfile {
            id: "t1".into(),
            name: "local".into(),
            kind: TargetKind::Adb,
            connection: ConnectionConfig::Adb {
                serial: Some("ABC".into()),
                transport: None,
            },
            credential_ref: None,
            default_policy: PolicyProfile::default(),
            notes: None,
            metadata: Default::default(),
        });

        let now = SystemTime::now();
        store.upsert_session(SessionRecord::new("s1", "t1", now));
        store.upsert_artifact(ArtifactRecord {
            id: "a1".into(),
            logical_session_id: "ls-1".into(),
            transport_session_id: Some("ts-1".into()),
            channel_id: Some("ch-1".into()),
            session_id: "s1".into(),
            parent_id: None,
            kind: ArtifactKind::RawCommandOutput,
            source_command: Some("dmesg -w".into()),
            filter: None,
            created_at: now,
            summary: "boot logs".into(),
        });

        assert_eq!(store.profiles.len(), 1);
        assert_eq!(store.sessions.len(), 1);
        assert_eq!(store.artifacts.len(), 1);
    }

    fn scope(agent_id: &str, client_session_id: &str) -> AccessScope {
        AccessScope {
            scope_id: format!("scope-{agent_id}"),
            workspace_id: "ws".into(),
            principal_id: "user".into(),
            agent_id: agent_id.into(),
            run_id: "run-1".into(),
            thread_id: Some("thread-1".into()),
            client_session_id: client_session_id.into(),
            origin: "mcp".into(),
            created_at: SystemTime::now(),
        }
    }

    #[test]
    fn isolates_logical_session_by_agent_scope() {
        let mut store = InMemoryMetadataStore::default();
        let now = SystemTime::now();

        let s1 = store.resolve_logical_session(
            &scope("agent-a", "client-1"),
            "target-1",
            SessionReusePolicy::ReuseIfAlive,
            now,
        );
        let s2 = store.resolve_logical_session(
            &scope("agent-b", "client-1"),
            "target-1",
            SessionReusePolicy::ReuseIfAlive,
            now,
        );

        assert_ne!(s1.logical_session_id, s2.logical_session_id);
    }

    #[test]
    fn resume_or_create_reopens_closed_logical_session() {
        let mut store = InMemoryMetadataStore::default();
        let now = SystemTime::now();
        let scope = scope("agent-a", "client-1");

        let created =
            store.resolve_logical_session(&scope, "target-1", SessionReusePolicy::AlwaysNew, now);
        {
            let record = store
                .logical_sessions
                .get_mut(&created.logical_session_id)
                .expect("logical session");
            record.status = bridgingio_domain::LogicalSessionStatus::Closed;
            record.closed_at = Some(now);
            record.close_reason = Some("network drop".into());
        }

        let resumed = store.resolve_logical_session(
            &scope,
            "target-1",
            SessionReusePolicy::ResumeOrCreate,
            now,
        );
        assert_eq!(created.logical_session_id, resumed.logical_session_id);
        assert!(resumed.status.is_alive());
        assert!(resumed.closed_at.is_none());
    }

    #[test]
    fn supports_multiple_channels_in_same_logical_session() {
        let mut store = InMemoryMetadataStore::default();
        let now = SystemTime::now();
        let logical = store.resolve_logical_session(
            &scope("agent-a", "client-7"),
            "target-1",
            SessionReusePolicy::ReuseIfAlive,
            now,
        );
        let transport = store.open_transport_session(
            &logical.logical_session_id,
            "target-1",
            TargetKind::Ssh,
            Some("/usr/bin/ssh".into()),
            Some("system_path".into()),
            now,
        );

        let command_channel = store.open_channel(
            &logical.logical_session_id,
            &transport.transport_session_id,
            "target-1",
            ChannelKind::OneShotExec,
            Some("command".into()),
            now,
        );
        let log_channel = store.open_channel(
            &logical.logical_session_id,
            &transport.transport_session_id,
            "target-1",
            ChannelKind::LogStream,
            Some("logs".into()),
            now,
        );

        assert_ne!(command_channel.channel_id, log_channel.channel_id);
        let channels = store.channels_for_logical_session(&logical.logical_session_id);
        assert_eq!(channels.len(), 2);
    }
}
