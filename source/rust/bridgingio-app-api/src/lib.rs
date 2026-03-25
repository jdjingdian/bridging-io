use std::time::SystemTime;

use bridgingio_domain::{
    ApprovalRequestRecord, ArtifactRecord, SessionRecord, SessionReusePolicy, TargetProfile,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiRequestContext {
    pub agent_id: String,
    pub run_id: String,
    pub client_session_id: String,
    pub reuse_policy: SessionReusePolicy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppCommand {
    ListTargets,
    OpenSession { target_id: String },
    Execute {
        session_id: String,
        command: String,
        stream: bool,
    },
    ReadArtifact {
        artifact_id: String,
        offset: usize,
        limit: usize,
    },
    RequestApproval {
        request: ApprovalRequestRecord,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiRequest {
    pub request_id: String,
    pub context: ApiRequestContext,
    pub command: AppCommand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelineEntry {
    pub id: String,
    pub session_id: String,
    pub command_preview: String,
    pub status: String,
    pub artifact_id: Option<String>,
    pub created_at: SystemTime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiEvent {
    SessionStateChanged {
        session: SessionRecord,
    },
    CommandTimelineEntry {
        entry: TimelineEntry,
    },
    ApprovalUpdated {
        request: ApprovalRequestRecord,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiErrorCode {
    NotFound,
    PermissionDenied,
    ValidationFailed,
    DependencyUnavailable,
    Internal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiError {
    pub code: ApiErrorCode,
    pub message: String,
    pub retriable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiResponse {
    Accepted {
        request_id: String,
    },
    Targets {
        request_id: String,
        items: Vec<TargetProfile>,
    },
    Session {
        request_id: String,
        session: SessionRecord,
    },
    Execution {
        request_id: String,
        artifact: ArtifactRecord,
    },
    Artifact {
        request_id: String,
        artifact: ArtifactRecord,
    },
    Approval {
        request_id: String,
        request: ApprovalRequestRecord,
    },
    Error {
        request_id: String,
        error: ApiError,
    },
}

#[cfg(test)]
mod tests {
    use bridgingio_domain::SessionReusePolicy;

    use super::{ApiRequest, ApiRequestContext, AppCommand};

    #[test]
    fn builds_execute_request_shape() {
        let request = ApiRequest {
            request_id: "req-1".into(),
            context: ApiRequestContext {
                agent_id: "agent-a".into(),
                run_id: "run-1".into(),
                client_session_id: "client-1".into(),
                reuse_policy: SessionReusePolicy::ReuseIfAlive,
            },
            command: AppCommand::Execute {
                session_id: "session-1".into(),
                command: "dmesg -w".into(),
                stream: true,
            },
        };
        assert!(matches!(request.command, AppCommand::Execute { .. }));
    }
}
