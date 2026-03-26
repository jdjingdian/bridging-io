import Foundation
import Testing
@testable import BridgingIO

@MainActor
struct BridgingIOTests {

    @Test func createTargetAddsProfileAndSelectsIt() throws {
        let viewModel = WorkspaceViewModel(dataSource: WorkspaceViewModel.fixtureDataSource())
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
        let viewModel = WorkspaceViewModel(dataSource: WorkspaceViewModel.fixtureDataSource())
        guard let pending = viewModel.selectedApprovals.first(where: { $0.status == .pending }) else {
            Issue.record("Expected a pending approval in seed data")
            return
        }

        viewModel.approve(pending)
        let updated = viewModel.selectedApprovals.first(where: { $0.id == pending.id })

        #expect(updated?.status == .approved)
    }

    @Test func artifactHashLookupMatchesPrefix() throws {
        let viewModel = WorkspaceViewModel(dataSource: WorkspaceViewModel.fixtureDataSource())
        viewModel.artifactLookupHash = "8cf4"
        viewModel.lookupArtifactByHash()

        #expect(viewModel.artifactLookupResult?.hash == "8cf4e2a1d7b2")
    }

    @Test func productNameIsConsistentAcrossEnglishAndChinese() throws {
        #expect(localized("branding.product_name", locale: "en") == "Bridging IO")
        #expect(localized("branding.product_name", locale: "zh-Hans") == "Bridging IO")
    }

    @Test func chineseCoreUiStringsExist() throws {
        #expect(localized("settings.title", locale: "zh-Hans") == "设置")
        #expect(localized("workspace.toolbar.new_target", locale: "zh-Hans") == "新建目标")
    }

    @Test func fixtureDataSourceStartsConnectedState() throws {
        let viewModel = WorkspaceViewModel(dataSource: WorkspaceViewModel.fixtureDataSource())
        #expect(viewModel.coreConnectionState == .connected)
    }

    @Test func startupErrorStateIsShownWhenBootstrapFails() throws {
        let viewModel = WorkspaceViewModel(
            dataSource: FailingDataSource(error: .transport("socket unavailable"))
        )
        #expect(viewModel.coreConnectionState == .attachFailed("socket unavailable"))
    }

    @Test func startupNeedsRuntimeRootStateIsShown() throws {
        let viewModel = WorkspaceViewModel(
            dataSource: FailingDataSource(error: .needsRuntimeRoot)
        )
        #expect(viewModel.coreConnectionState == .needsRuntimeRoot)
    }

    @Test func startupRuntimeRootUnavailableStateIsShown() throws {
        let viewModel = WorkspaceViewModel(
            dataSource: FailingDataSource(error: .runtimeRootUnavailable("/tmp/missing"))
        )
        #expect(viewModel.coreConnectionState == .runtimeRootUnavailable("/tmp/missing"))
    }

    @Test func managedSaveTargetUsesCoreOwnedUpsert() throws {
        let dataSource = ManagedMockDataSource(snapshot: WorkspaceViewModel.fixtureSnapshot())
        let viewModel = WorkspaceViewModel(dataSource: dataSource)
        var draft = TargetProfileDraft()
        draft.name = "managed-ci-host"
        draft.aliasForModel = "managed-ci"
        draft.sshConfig.host = "10.9.0.22"
        draft.sshConfig.username = "runner"
        viewModel.targetEditorContext = TargetEditorContext(mode: .create, draft: draft)

        viewModel.saveTarget(draft: draft)

        #expect(dataSource.upsertProfileCallCount == 1)
        #expect(dataSource.lastUpsertDraft?.name == "managed-ci-host")
        #expect(viewModel.targetEditorContext == nil)
        #expect(viewModel.coreConnectionState == .connected)
    }

    @Test func managedModelPlaneSaveRestartRequiredPerformsControlledRestart() throws {
        let dataSource = ManagedMockDataSource(snapshot: WorkspaceViewModel.fixtureSnapshot())
        dataSource.updateModelPlaneStrategy = "restart_required"
        let viewModel = WorkspaceViewModel(dataSource: dataSource)
        viewModel.modelPlaneHost = "127.0.0.1"
        viewModel.modelPlanePort = 21000

        viewModel.saveModelPlaneSettings()

        #expect(dataSource.updateModelPlaneCallCount == 1)
        #expect(dataSource.controlledRestartCallCount == 1)
        #expect(viewModel.coreConnectionState == .connected)
    }

    @Test func managedModelPlaneRestartFailureKeepsRestartFailedState() throws {
        let dataSource = ManagedMockDataSource(snapshot: WorkspaceViewModel.fixtureSnapshot())
        dataSource.updateModelPlaneStrategy = "restart_required"
        dataSource.controlledRestartError = WorkspaceDataSourceError.transport("restart boom")
        let viewModel = WorkspaceViewModel(dataSource: dataSource)

        viewModel.saveModelPlaneSettings()

        #expect(dataSource.controlledRestartCallCount == 1)
        #expect(viewModel.coreConnectionState == .restartFailed("restart boom"))
    }

    private func localized(_ key: String, locale: String) -> String {
        let bundle = Bundle.main
        guard let path = bundle.path(forResource: locale, ofType: "lproj"),
              let localizedBundle = Bundle(path: path) else {
            return key
        }
        return NSLocalizedString(key, tableName: "Localizable", bundle: localizedBundle, value: key, comment: "")
    }
}

private final class FailingDataSource: WorkspaceDataSource {
    let error: WorkspaceDataSourceError

    init(error: WorkspaceDataSourceError) {
        self.error = error
    }

    func bootstrapSnapshot() throws -> WorkspaceSnapshot {
        throw error
    }

    func refreshSnapshot() throws -> WorkspaceSnapshot {
        throw error
    }

    func shutdown() {}
}

private final class ManagedMockDataSource: ManagedWorkspaceDataSource {
    var snapshot: WorkspaceSnapshot
    var updateModelPlaneStrategy = "live_applied"
    var updateArtifactCacheStrategy = "live_applied"
    var updateToolOverrideStrategy = "live_applied"
    var clearArtifactCacheStrategy = "live_applied"
    var upsertProfileStrategy = "live_applied"
    var controlledRestartError: WorkspaceDataSourceError?

    private(set) var updateModelPlaneCallCount = 0
    private(set) var controlledRestartCallCount = 0
    private(set) var upsertProfileCallCount = 0
    private(set) var lastUpsertDraft: TargetProfileDraft?
    private(set) var runtimeRootPath: String?

    init(snapshot: WorkspaceSnapshot) {
        self.snapshot = snapshot
    }

    func bootstrapSnapshot() throws -> WorkspaceSnapshot {
        snapshot
    }

    func refreshSnapshot() throws -> WorkspaceSnapshot {
        snapshot
    }

    func shutdown() {}

    func fetchProfileDraft(coreID: String) throws -> TargetProfileDraft {
        var draft = TargetProfileDraft()
        draft.existingCoreID = coreID
        draft.name = "target-\(coreID)"
        draft.aliasForModel = coreID
        return draft
    }

    func upsertProfile(draft: TargetProfileDraft) throws -> String {
        upsertProfileCallCount += 1
        lastUpsertDraft = draft
        return upsertProfileStrategy
    }

    func updateModelPlane(host: String, port: Int) throws -> String {
        updateModelPlaneCallCount += 1
        snapshot.modelPlaneHost = host
        snapshot.modelPlanePort = port
        return updateModelPlaneStrategy
    }

    func updateArtifactCache(_ settings: ArtifactCacheSettings) throws -> String {
        snapshot.cacheSettings = settings
        return updateArtifactCacheStrategy
    }

    func updateToolOverride(connectorID: String, newPath: String) throws -> String {
        _ = connectorID
        _ = newPath
        return updateToolOverrideStrategy
    }

    func clearArtifactCache() throws -> String {
        snapshot.cacheSettings.usedCacheMB = 0
        return clearArtifactCacheStrategy
    }

    func setRuntimeRoot(path: String) {
        runtimeRootPath = path
    }

    func controlledRestart() throws -> WorkspaceSnapshot {
        controlledRestartCallCount += 1
        if let error = controlledRestartError {
            throw error
        }
        return snapshot
    }
}
