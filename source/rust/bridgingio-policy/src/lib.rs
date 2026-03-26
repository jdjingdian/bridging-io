use bridgingio_domain::PolicyProfile;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperationKind {
    Read,
    Write,
    Delete,
    Privileged,
    SensitiveRead,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolicyDecision {
    Allow,
    RequireApproval,
}

pub fn evaluate(policy: &PolicyProfile, operation: OperationKind) -> PolicyDecision {
    match operation {
        OperationKind::Read => PolicyDecision::Allow,
        OperationKind::Write if policy.require_approval_for_write => {
            PolicyDecision::RequireApproval
        }
        OperationKind::Delete if policy.require_approval_for_delete => {
            PolicyDecision::RequireApproval
        }
        OperationKind::Privileged if policy.require_approval_for_privileged => {
            PolicyDecision::RequireApproval
        }
        OperationKind::SensitiveRead if policy.require_approval_for_sensitive_read => {
            PolicyDecision::RequireApproval
        }
        _ => PolicyDecision::Allow,
    }
}

#[cfg(test)]
mod tests {
    use bridgingio_domain::PolicyProfile;

    use super::{evaluate, OperationKind, PolicyDecision};

    #[test]
    fn requires_approval_for_high_risk_operations() {
        let decision = evaluate(&PolicyProfile::default(), OperationKind::Privileged);
        assert_eq!(decision, PolicyDecision::RequireApproval);
    }

    #[test]
    fn allows_normal_read_without_prompt() {
        let decision = evaluate(&PolicyProfile::default(), OperationKind::Read);
        assert_eq!(decision, PolicyDecision::Allow);
    }
}
