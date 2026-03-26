import Foundation
import SwiftUI
import Combine

@MainActor
final class TargetEditorViewModel: ObservableObject {
    @Published var draft: TargetProfileDraft
    @Published var validationMessage: String?

    let mode: TargetEditorMode

    init(context: TargetEditorContext) {
        mode = context.mode
        draft = context.draft
    }

    var title: String {
        mode.title
    }

    var actionTitle: String {
        mode == .create ? L10n.t("target_editor.action.create") : L10n.t("target_editor.action.save")
    }

    func applyConnectorOverride(for diagnosticID: String, path: String) {
        guard let index = draft.toolDiagnostics.firstIndex(where: { $0.id == diagnosticID }) else {
            return
        }

        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        draft.toolDiagnostics[index].overridePath = trimmed
        if trimmed.isEmpty {
            if draft.toolDiagnostics[index].sourceType == .userOverride {
                draft.toolDiagnostics[index].sourceType = .systemPath
            }
        } else {
            draft.toolDiagnostics[index].sourceType = .userOverride
            draft.toolDiagnostics[index].effectivePath = trimmed
        }
        draft.toolDiagnostics[index].lastChecked = .now
    }

    func validate() -> Bool {
        let name = draft.name.trimmingCharacters(in: .whitespacesAndNewlines)
        if name.isEmpty {
            validationMessage = L10n.t("validation.target_name_required")
            return false
        }

        let alias = draft.aliasForModel.trimmingCharacters(in: .whitespacesAndNewlines)
        if alias.isEmpty {
            validationMessage = L10n.t("validation.target_alias_required")
            return false
        }

        switch draft.kind {
        case .ssh:
            if draft.sshConfig.host.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                validationMessage = L10n.t("validation.ssh_host_required")
                return false
            }
            if draft.sshConfig.username.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                validationMessage = L10n.t("validation.ssh_username_required")
                return false
            }
        case .adb:
            if draft.adbConfig.serial.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                validationMessage = L10n.t("validation.adb_serial_required")
                return false
            }
        case .serial:
            if draft.serialConfig.devicePath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                validationMessage = L10n.t("validation.serial_device_path_required")
                return false
            }
        case .docker:
            if draft.dockerConfig.containerName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                validationMessage = L10n.t("validation.docker_container_required")
                return false
            }
        case .httpDebug:
            if draft.httpDebugConfig.baseURL.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                validationMessage = L10n.t("validation.http_base_url_required")
                return false
            }
        case .openGrok:
            if draft.openGrokConfig.endpoint.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                validationMessage = L10n.t("validation.opengrok_endpoint_required")
                return false
            }
        }

        validationMessage = nil
        return true
    }
}
