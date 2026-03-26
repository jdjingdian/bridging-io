import Testing
@testable import BridgingIO

@MainActor
struct BridgingIOTests {

    @Test func createTargetAddsProfileAndSelectsIt() throws {
        let viewModel = WorkspaceViewModel()
        let initialCount = viewModel.targets.count

        var draft = TargetProfileDraft()
        draft.name = "ci-host"
        draft.aliasForModel = "ci"
        draft.sshConfig.host = "10.2.0.9"
        draft.sshConfig.username = "runner"

        viewModel.saveTarget(draft: draft)

        #expect(viewModel.targets.count == initialCount + 1)
        #expect(viewModel.selectedTarget?.name == "ci-host")
    }

    @Test func approvalTransitionUpdatesStatus() throws {
        let viewModel = WorkspaceViewModel()
        guard let pending = viewModel.selectedApprovals.first(where: { $0.status == .pending }) else {
            Issue.record("Expected a pending approval in seed data")
            return
        }

        viewModel.approve(pending)
        let updated = viewModel.selectedApprovals.first(where: { $0.id == pending.id })

        #expect(updated?.status == .approved)
    }

    @Test func artifactHashLookupMatchesPrefix() throws {
        let viewModel = WorkspaceViewModel()
        viewModel.artifactLookupHash = "8cf4"
        viewModel.lookupArtifactByHash()

        #expect(viewModel.artifactLookupResult?.hash == "8cf4e2a1d7b2")
    }
}
