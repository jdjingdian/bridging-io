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
        mode == .create ? "Create Target" : "Save Changes"
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
            validationMessage = "Target name is required."
            return false
        }

        let alias = draft.aliasForModel.trimmingCharacters(in: .whitespacesAndNewlines)
        if alias.isEmpty {
            validationMessage = "Alias for model is required."
            return false
        }

        switch draft.kind {
        case .ssh:
            if draft.sshConfig.host.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                validationMessage = "SSH host is required."
                return false
            }
            if draft.sshConfig.username.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                validationMessage = "SSH username is required."
                return false
            }
        case .adb:
            if draft.adbConfig.serial.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                validationMessage = "ADB serial is required."
                return false
            }
        case .serial:
            if draft.serialConfig.devicePath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                validationMessage = "Serial device path is required."
                return false
            }
        case .docker:
            if draft.dockerConfig.containerName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                validationMessage = "Docker container name is required."
                return false
            }
        case .httpDebug:
            if draft.httpDebugConfig.baseURL.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                validationMessage = "HTTP base URL is required."
                return false
            }
        case .openGrok:
            if draft.openGrokConfig.endpoint.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                validationMessage = "OpenGrok endpoint is required."
                return false
            }
        }

        validationMessage = nil
        return true
    }
}
