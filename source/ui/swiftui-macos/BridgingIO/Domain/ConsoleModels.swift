import Foundation

enum TargetKind: String, CaseIterable, Identifiable, Codable {
    case ssh
    case adb
    case serial
    case docker
    case httpDebug
    case openGrok

    var id: String { rawValue }

    var title: String {
        switch self {
        case .ssh:
            return "SSH"
        case .adb:
            return "ADB"
        case .serial:
            return "Serial"
        case .docker:
            return "Docker"
        case .httpDebug:
            return "HTTP"
        case .openGrok:
            return "OpenGrok"
        }
    }

    var symbolName: String {
        switch self {
        case .ssh:
            return "network"
        case .adb:
            return "iphone"
        case .serial:
            return "cable.connector"
        case .docker:
            return "shippingbox"
        case .httpDebug:
            return "globe"
        case .openGrok:
            return "magnifyingglass"
        }
    }
}

enum TargetConnectionState: String, CaseIterable, Codable {
    case connected
    case idle
    case degraded
    case disconnected

    var title: String {
        rawValue.capitalized
    }
}

enum SessionState: String, CaseIterable, Codable {
    case active
    case waiting
    case degraded
    case closed

    var title: String {
        rawValue.capitalized
    }
}

enum CommandExecutionState: String, CaseIterable, Codable {
    case success
    case streaming
    case waitingApproval
    case failed

    var title: String {
        switch self {
        case .waitingApproval:
            return "Approval"
        default:
            return rawValue.capitalized
        }
    }
}

enum ApprovalStatus: String, CaseIterable, Codable {
    case pending
    case approved
    case rejected
    case failed

    var title: String {
        rawValue.capitalized
    }
}

enum ToolSourceType: String, CaseIterable, Codable {
    case userOverride
    case systemPath
    case bundledFallback

    var title: String {
        switch self {
        case .userOverride:
            return "User Override"
        case .systemPath:
            return "System PATH"
        case .bundledFallback:
            return "Bundled Fallback"
        }
    }
}

enum ArtifactCacheBackend: String, CaseIterable, Identifiable, Codable {
    case memory
    case filesystem

    var id: String { rawValue }

    var title: String {
        rawValue.capitalized
    }
}

enum ArtifactEvictionPolicy: String, CaseIterable, Identifiable, Codable {
    case lru
    case fifo

    var id: String { rawValue }

    var title: String {
        rawValue.uppercased()
    }
}

enum TargetFilter: String, CaseIterable, Identifiable {
    case all
    case ssh
    case adb

    var id: String { rawValue }

    var title: String {
        switch self {
        case .all:
            return "All"
        case .ssh:
            return "SSH"
        case .adb:
            return "ADB"
        }
    }

    func matches(_ target: TargetProfile) -> Bool {
        switch self {
        case .all:
            return true
        case .ssh:
            return target.kind == .ssh
        case .adb:
            return target.kind == .adb
        }
    }
}

enum WorkspaceCenterPanel: String, CaseIterable, Identifiable {
    case timeline
    case transcript

    var id: String { rawValue }

    var title: String {
        rawValue.capitalized
    }
}

struct CapabilitySummary: Identifiable, Equatable {
    let id: String
    let title: String
}

struct EnvironmentFingerprint: Equatable {
    var osVersion: String
    var architecture: String
    var shell: String
    var detectedTools: [String]
}

struct SessionSummary: Equatable {
    var logicalSessionID: String
    var channelID: String
    var state: SessionState
    var lastHeartbeat: Date
    var fingerprint: EnvironmentFingerprint
}

struct ToolSourceDiagnostic: Identifiable, Equatable {
    let id: String
    var connectorName: String
    var sourceType: ToolSourceType
    var effectivePath: String
    var overridePath: String
    var lastChecked: Date
}

struct TargetPolicyDefaults: Equatable {
    var approveWrite: Bool
    var approveDelete: Bool
    var approveSudo: Bool
    var approveSensitiveRead: Bool

    static let `default` = TargetPolicyDefaults(
        approveWrite: true,
        approveDelete: true,
        approveSudo: true,
        approveSensitiveRead: true
    )
}

struct SSHConnectionConfig: Equatable {
    var host: String
    var port: Int
    var username: String

    static let empty = SSHConnectionConfig(host: "", port: 22, username: "")
}

struct ADBConnectionConfig: Equatable {
    var serial: String
    var transport: String

    static let empty = ADBConnectionConfig(serial: "", transport: "usb")
}

struct SerialConnectionConfig: Equatable {
    var devicePath: String
    var baudRate: Int

    static let empty = SerialConnectionConfig(devicePath: "/dev/tty.usbmodem0", baudRate: 115200)
}

struct DockerConnectionConfig: Equatable {
    var containerName: String
    var context: String
    var shellPreference: String

    static let empty = DockerConnectionConfig(containerName: "", context: "default", shellPreference: "/bin/sh")
}

struct HTTPDebugConnectionConfig: Equatable {
    var baseURL: String
    var authReference: String
    var environmentPreset: String
    var headerPreset: String

    static let empty = HTTPDebugConnectionConfig(baseURL: "https://", authReference: "", environmentPreset: "dev", headerPreset: "json")
}

struct OpenGrokConnectionConfig: Equatable {
    var endpoint: String
    var repositoryScope: String
    var tokenReference: String
    var queryScope: String

    static let empty = OpenGrokConnectionConfig(endpoint: "https://", repositoryScope: "", tokenReference: "", queryScope: "default")
}

struct TargetProfile: Identifiable, Equatable {
    var id: UUID
    var name: String
    var kind: TargetKind
    var aliasForModel: String
    var notes: String
    var credentialReference: String
    var policyDefaults: TargetPolicyDefaults
    var sshConfig: SSHConnectionConfig
    var adbConfig: ADBConnectionConfig
    var serialConfig: SerialConnectionConfig
    var dockerConfig: DockerConnectionConfig
    var httpDebugConfig: HTTPDebugConnectionConfig
    var openGrokConfig: OpenGrokConnectionConfig
    var connectionState: TargetConnectionState
    var lastActivity: Date
    var sessionSummary: SessionSummary
    var capabilities: [CapabilitySummary]
    var toolDiagnostics: [ToolSourceDiagnostic]

    var accessibilityID: String {
        "target-row-\(name.lowercased().replacingOccurrences(of: " ", with: "-"))"
    }
}

struct ArtifactReference: Identifiable, Equatable {
    let id: String
    let hash: String
    let label: String
}

struct CommandTimelineItem: Identifiable, Equatable {
    let id: UUID
    var targetID: UUID
    var sessionID: String
    var channelID: String
    var timestamp: Date
    var command: String
    var summary: String
    var state: CommandExecutionState
    var stdoutPreview: String
    var stderrPreview: String
    var exitStatus: Int
    var artifacts: [ArtifactReference]
}

struct ArtifactRecord: Identifiable, Equatable {
    var id: String { hash }
    var hash: String
    var contentDigest: String
    var sourceCommand: String
    var summary: String
    var sessionID: String
    var channelID: String
    var parentHashes: [String]
    var derivedHashes: [String]
    var textContent: String

    var lineCount: Int {
        max(1, textContent.split(separator: "\n").count)
    }
}

struct ApprovalRequestItem: Identifiable, Equatable {
    var id: UUID
    var targetID: UUID
    var commandSummary: String
    var reason: String
    var createdAt: Date
    var status: ApprovalStatus
}

enum ShellLineRole: String, Codable {
    case prompt
    case input
    case output
    case info
}

struct ShellLine: Identifiable, Equatable {
    var id: UUID
    var role: ShellLineRole
    var content: String
    var timestamp: Date
}

struct ShellChannel: Identifiable, Equatable {
    var id: String
    var targetID: UUID
    var title: String
    var prompt: String
    var isClosed: Bool
    var lines: [ShellLine]
}

struct ArtifactCacheSettings: Equatable {
    var backend: ArtifactCacheBackend
    var rootPath: String
    var maxCacheMB: Int
    var evictionPolicy: ArtifactEvictionPolicy
    var usedCacheMB: Int

    static let `default` = ArtifactCacheSettings(
        backend: .filesystem,
        rootPath: "~/Library/Application Support/BridgingIO/artifacts",
        maxCacheMB: 2048,
        evictionPolicy: .lru,
        usedCacheMB: 612
    )
}

struct TargetProfileDraft: Equatable {
    var existingID: UUID?
    var name: String
    var kind: TargetKind
    var aliasForModel: String
    var notes: String
    var credentialReference: String
    var policyDefaults: TargetPolicyDefaults
    var sshConfig: SSHConnectionConfig
    var adbConfig: ADBConnectionConfig
    var serialConfig: SerialConnectionConfig
    var dockerConfig: DockerConnectionConfig
    var httpDebugConfig: HTTPDebugConnectionConfig
    var openGrokConfig: OpenGrokConnectionConfig
    var toolDiagnostics: [ToolSourceDiagnostic]

    init(profile: TargetProfile? = nil) {
        self.existingID = profile?.id
        self.name = profile?.name ?? ""
        self.kind = profile?.kind ?? .ssh
        self.aliasForModel = profile?.aliasForModel ?? ""
        self.notes = profile?.notes ?? ""
        self.credentialReference = profile?.credentialReference ?? "vault:ssh-key:"
        self.policyDefaults = profile?.policyDefaults ?? .default
        self.sshConfig = profile?.sshConfig ?? .empty
        self.adbConfig = profile?.adbConfig ?? .empty
        self.serialConfig = profile?.serialConfig ?? .empty
        self.dockerConfig = profile?.dockerConfig ?? .empty
        self.httpDebugConfig = profile?.httpDebugConfig ?? .empty
        self.openGrokConfig = profile?.openGrokConfig ?? .empty
        self.toolDiagnostics = profile?.toolDiagnostics ?? [
            ToolSourceDiagnostic(
                id: "ssh",
                connectorName: "ssh",
                sourceType: .systemPath,
                effectivePath: "/usr/bin/ssh",
                overridePath: "",
                lastChecked: .now
            ),
            ToolSourceDiagnostic(
                id: "adb",
                connectorName: "adb",
                sourceType: .bundledFallback,
                effectivePath: "BridgingIO bundle",
                overridePath: "",
                lastChecked: .now
            )
        ]
    }
}

enum TargetEditorMode {
    case create
    case edit

    var title: String {
        switch self {
        case .create:
            return "New Target"
        case .edit:
            return "Edit Target"
        }
    }
}

struct TargetEditorContext: Identifiable {
    let id = UUID()
    var mode: TargetEditorMode
    var draft: TargetProfileDraft
}
