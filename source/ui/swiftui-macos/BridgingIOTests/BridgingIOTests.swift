import Foundation
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

    @Test func productNameIsConsistentAcrossEnglishAndChinese() throws {
        #expect(localized("branding.product_name", locale: "en") == "Bridging IO")
        #expect(localized("branding.product_name", locale: "zh-Hans") == "Bridging IO")
    }

    @Test func chineseCoreUiStringsExist() throws {
        #expect(localized("settings.title", locale: "zh-Hans") == "设置")
        #expect(localized("workspace.toolbar.new_target", locale: "zh-Hans") == "新建目标")
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
