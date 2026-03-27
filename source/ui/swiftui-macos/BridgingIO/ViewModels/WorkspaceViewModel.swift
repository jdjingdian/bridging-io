import Foundation
import SwiftUI
import Combine
import AppKit
import Darwin

enum WorkspaceCoreConnectionState: Equatable {
    case needsRuntimeRoot
    case runtimeRootUnavailable(String)
    case startingCore
    case attachingUI
    case attachFailed(String)
    case savingChanges
    case restartRequired
    case restartingCore
    case restartFailed(String)
    case connected
}

final class ManagedCoreWorkspaceDataSource: ManagedWorkspaceDataSource {
    private let lock = NSLock()
    private var process: Process?
    private var coreLogHandle: FileHandle?
    private var requestSequence: Int = 0
    private var targetIDMap: [String: UUID] = [:]
    private let uiInstanceID: String
    private var socketPath: String = ""
    private var coreLogPath: String = ""
    private var runtimeRootPath: String?
    private static let runtimeRootDefaultsKey = "bridgingio.runtime_root"

    init() {
        let shortID = String(UUID().uuidString.lowercased().prefix(8))
        uiInstanceID = "swiftui-\(shortID)"
        runtimeRootPath = UserDefaults.standard.string(forKey: Self.runtimeRootDefaultsKey)
    }

    func bootstrapSnapshot() throws -> WorkspaceSnapshot {
        lock.lock()
        defer { lock.unlock() }
        try ensureCoreStartedLocked()
        try attachUILocked()
        return try fetchBootstrapLocked()
    }

    func refreshSnapshot() throws -> WorkspaceSnapshot {
        lock.lock()
        defer { lock.unlock() }
        try ensureCoreStartedLocked()
        return try fetchBootstrapLocked()
    }

    func shutdown() {
        lock.lock()
        defer { lock.unlock() }
        _ = try? sendCommandLocked(command: "request_shutdown", fields: [:])
        if let process {
            if process.isRunning {
                process.terminate()
            }
            self.process = nil
        }
        try? coreLogHandle?.close()
        coreLogHandle = nil
        if !socketPath.isEmpty {
            try? FileManager.default.removeItem(atPath: socketPath)
        }
    }

    func hasRuntimeRootSelection() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return runtimeRootPath != nil
    }

    func setRuntimeRoot(path: String) {
        lock.lock()
        defer { lock.unlock() }
        runtimeRootPath = path
        UserDefaults.standard.set(path, forKey: Self.runtimeRootDefaultsKey)
    }

    func selectedRuntimeRoot() -> String? {
        lock.lock()
        defer { lock.unlock() }
        return runtimeRootPath
    }

    func controlledRestart() throws -> WorkspaceSnapshot {
        lock.lock()
        defer { lock.unlock() }
        stopCoreLocked()
        try ensureCoreStartedLocked()
        try attachUILocked()
        return try fetchBootstrapLocked()
    }

    private func ensureCoreStartedLocked() throws {
        if let process, process.isRunning {
            return
        }
        let runtimeRoot = try validatedRuntimeRootLocked()
        let stateDir = URL(fileURLWithPath: runtimeRoot, isDirectory: true)
            .appendingPathComponent("state", isDirectory: true)
        let logsDir = URL(fileURLWithPath: runtimeRoot, isDirectory: true)
            .appendingPathComponent("logs", isDirectory: true)
        try FileManager.default.createDirectory(at: stateDir, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: logsDir, withIntermediateDirectories: true)
        socketPath = try controlPlaneSocketPathLocked()
        coreLogPath = logsDir.appendingPathComponent("bridgingio-core.log").path

        stopCoreLocked()
        try? FileManager.default.removeItem(atPath: socketPath)
        let executable = try resolveCoreExecutablePath()
        FileManager.default.createFile(atPath: coreLogPath, contents: nil)
        let logHandle = try FileHandle(forWritingTo: URL(fileURLWithPath: coreLogPath))
        let coreProcess = Process()
        coreProcess.executableURL = URL(fileURLWithPath: executable)
        coreProcess.arguments = [
            "ui-managed-ephemeral",
            "--runtime-root",
            runtimeRoot,
            "--control-plane-socket-override",
            socketPath
        ]
        coreProcess.standardOutput = logHandle
        coreProcess.standardError = logHandle
        do {
            try coreProcess.run()
        } catch {
            try? logHandle.close()
            throw WorkspaceDataSourceError.transport(
                "start bridgingio-core failed: \(error.localizedDescription) (log: \(coreLogPath))"
            )
        }
        coreLogHandle = logHandle
        process = coreProcess
        if !waitForSocketReady(timeoutSeconds: 6.0) {
            let logTail = coreLogTailLocked()
            if logTail.isEmpty {
                throw WorkspaceDataSourceError.transport(
                    "core control-plane socket not ready (socket: \(socketPath), log: \(coreLogPath))"
                )
            }
            throw WorkspaceDataSourceError.transport(
                "core control-plane socket not ready (socket: \(socketPath), log: \(coreLogPath))\n\(logTail)"
            )
        }
    }

    private func stopCoreLocked() {
        if let running = process, running.isRunning {
            running.terminate()
        }
        process = nil
        try? coreLogHandle?.close()
        coreLogHandle = nil
        if !socketPath.isEmpty {
            try? FileManager.default.removeItem(atPath: socketPath)
        }
    }

    private func validatedRuntimeRootLocked() throws -> String {
        guard let runtimeRootPath,
              !runtimeRootPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw WorkspaceDataSourceError.needsRuntimeRoot
        }
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: runtimeRootPath, isDirectory: &isDirectory),
              isDirectory.boolValue else {
            throw WorkspaceDataSourceError.runtimeRootUnavailable(runtimeRootPath)
        }
        guard FileManager.default.isWritableFile(atPath: runtimeRootPath) else {
            throw WorkspaceDataSourceError.runtimeRootUnavailable(runtimeRootPath)
        }
        return runtimeRootPath
    }

    private func resolveCoreExecutablePath() throws -> String {
        let env = ProcessInfo.processInfo.environment
        if let configured = env["BRIDGINGIO_CORE_BIN"],
           FileManager.default.isExecutableFile(atPath: configured) {
            return configured
        }
        let candidates = [
            Bundle.main.bundleURL
                .appendingPathComponent("Contents")
                .appendingPathComponent("MacOS")
                .appendingPathComponent("bridgingio-core")
                .path,
            Bundle.main.bundleURL
                .deletingLastPathComponent()
                .appendingPathComponent("bridgingio-core")
                .path,
        ]
        if let path = candidates.first(where: { FileManager.default.isExecutableFile(atPath: $0) }) {
            return path
        }
        throw WorkspaceDataSourceError.transport("cannot find bridgingio-core executable")
    }

    private func controlPlaneSocketPathLocked() throws -> String {
        let ipcDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("bridgingio-ipc", isDirectory: true)
        try FileManager.default.createDirectory(at: ipcDir, withIntermediateDirectories: true)
        let shortID = String(uiInstanceID.suffix(8))
        let candidate = ipcDir.appendingPathComponent("cp-\(shortID).sock").path
        let maxSocketPathBytes = MemoryLayout.size(ofValue: sockaddr_un().sun_path)
        if candidate.utf8CString.count > maxSocketPathBytes {
            throw WorkspaceDataSourceError.transport(
                "control-plane socket path too long: \(candidate)"
            )
        }
        return candidate
    }

    private func waitForSocketReady(timeoutSeconds: TimeInterval) -> Bool {
        let deadline = Date().addingTimeInterval(timeoutSeconds)
        while Date() < deadline {
            if FileManager.default.fileExists(atPath: socketPath) {
                return true
            }
            usleep(100_000)
        }
        return false
    }

    private func attachUILocked() throws {
        let response = try sendCommandLocked(
            command: "attach_ui",
            fields: [
                "ui_instance_id": uiInstanceID,
                "ui_kind": "swiftui-macos"
            ]
        )
        if let kind = response["kind"], kind == "attached" {
            return
        }
        if let kind = response["kind"], kind == "not_ready" {
            throw WorkspaceDataSourceError.waitingForAttach
        }
        throw WorkspaceDataSourceError.protocolViolation("attach_ui failed")
    }

    private func fetchBootstrapLocked() throws -> WorkspaceSnapshot {
        let response = try sendCommandLocked(
            command: "get_bootstrap_state",
            fields: [
                "timeline_limit": "120",
                "artifact_limit": "120",
                "transcript_limit": "120"
            ]
        )
        let kind = response["kind"] ?? ""
        if kind == "not_ready" {
            throw WorkspaceDataSourceError.waitingForAttach
        }
        if kind == "error" {
            throw WorkspaceDataSourceError.transport(unescape(response["message"] ?? "unknown error"))
        }
        guard kind == "bootstrap", let payload = response["payload"] else {
            throw WorkspaceDataSourceError.protocolViolation("unexpected response kind: \(kind)")
        }
        return try decodeBootstrapPayload(unescape(payload))
    }

    func fetchProfileDraft(coreID: String) throws -> TargetProfileDraft {
        lock.lock()
        defer { lock.unlock() }
        try ensureCoreStartedLocked()
        let response = try sendCommandLocked(
            command: "get_profile",
            fields: ["target_id": coreID]
        )
        if response["kind"] == "error" {
            throw WorkspaceDataSourceError.transport(unescape(response["message"] ?? "get_profile failed"))
        }
        guard response["kind"] == "profile",
              let payload = response["payload"] else {
            throw WorkspaceDataSourceError.protocolViolation("unexpected get_profile response")
        }
        guard let data = unescape(payload).data(using: .utf8),
              let root = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw WorkspaceDataSourceError.protocolViolation("invalid profile payload")
        }
        return try parseDraftFromProfilePayload(root)
    }

    func upsertProfile(draft: TargetProfileDraft) throws -> String {
        lock.lock()
        defer { lock.unlock() }
        try ensureCoreStartedLocked()
        var fields: [String: String] = [
            "target_id": draft.existingCoreID ?? UUID().uuidString.lowercased(),
            "target_name": draft.name,
            "target_kind": targetKindLabel(for: draft.kind),
            "target_alias": draft.aliasForModel,
            "notes": draft.notes,
            "credential_ref": draft.credentialReference
        ]
        switch draft.kind {
        case .ssh:
            fields["ssh_host"] = draft.sshConfig.host
            fields["ssh_port"] = "\(draft.sshConfig.port)"
            fields["ssh_username"] = draft.sshConfig.username
        case .adb:
            fields["adb_serial"] = draft.adbConfig.serial
            fields["adb_transport"] = draft.adbConfig.transport
        case .serial:
            fields["serial_device"] = draft.serialConfig.devicePath
            fields["serial_baud"] = "\(draft.serialConfig.baudRate)"
        case .docker:
            fields["docker_container"] = draft.dockerConfig.containerName
            fields["docker_context"] = draft.dockerConfig.context
        case .httpDebug:
            fields["custom_description"] = "http_debug:\(draft.httpDebugConfig.baseURL)"
        case .openGrok:
            fields["custom_description"] = "opengrok:\(draft.openGrokConfig.endpoint)"
        }
        let relevantCommands = toolchainCommands(for: draft.kind)
        let toolEntries: [(command: String, path: String)] = draft.toolDiagnostics
            .filter { relevantCommands.contains($0.id) }
            .map {
                (
                    command: $0.id,
                    path: $0.overridePath.trimmingCharacters(in: .whitespacesAndNewlines)
                )
            }
            .sorted { $0.command < $1.command }
        fields["target_toolchain_count"] = "\(toolEntries.count)"
        for (index, entry) in toolEntries.enumerated() {
            fields["target_toolchain_\(index)_command"] = entry.command
            fields["target_toolchain_\(index)_path_override"] = entry.path
        }
        let response = try sendCommandLocked(command: "upsert_profile", fields: fields)
        return try parseApplyStrategyOrThrow(response: response)
    }

    func updateModelPlane(host: String, port: Int) throws -> String {
        lock.lock()
        defer { lock.unlock() }
        try ensureCoreStartedLocked()
        let response = try sendCommandLocked(
            command: "update_settings",
            fields: [
                "model_plane_host": host,
                "model_plane_port": "\(port)"
            ]
        )
        return try parseApplyStrategyOrThrow(response: response)
    }

    func updateArtifactCache(_ settings: ArtifactCacheSettings) throws -> String {
        lock.lock()
        defer { lock.unlock() }
        try ensureCoreStartedLocked()
        let response = try sendCommandLocked(
            command: "update_settings",
            fields: [
                "artifact_cache_backend": settings.backend.rawValue,
                "artifact_cache_root": settings.rootPath,
                "artifact_cache_max_bytes": "\(settings.maxCacheMB * 1024 * 1024)",
                "artifact_cache_eviction_policy": settings.evictionPolicy.rawValue
            ]
        )
        return try parseApplyStrategyOrThrow(response: response)
    }

    func updateToolOverride(connectorID: String, newPath: String) throws -> String {
        lock.lock()
        defer { lock.unlock() }
        try ensureCoreStartedLocked()
        let response = try sendCommandLocked(
            command: "update_settings",
            fields: [
                "tool_override_command": connectorID,
                "tool_override_path": newPath
            ]
        )
        return try parseApplyStrategyOrThrow(response: response)
    }

    func clearArtifactCache() throws -> String {
        lock.lock()
        defer { lock.unlock() }
        try ensureCoreStartedLocked()
        let response = try sendCommandLocked(command: "clear_artifact_cache", fields: [:])
        return try parseApplyStrategyOrThrow(response: response)
    }

    private func parseApplyStrategyOrThrow(response: [String: String]) throws -> String {
        let kind = response["kind"] ?? ""
        if kind == "error" {
            throw WorkspaceDataSourceError.transport(unescape(response["message"] ?? "unknown error"))
        }
        guard kind == "accepted" else {
            throw WorkspaceDataSourceError.protocolViolation("unexpected response kind: \(kind)")
        }
        return unescape(response["apply_strategy"] ?? "live_applied")
    }

    private func sendCommandLocked(command: String, fields: [String: String]) throws -> [String: String] {
        requestSequence += 1
        let requestID = "ui-\(requestSequence)"
        var pairs = [
            "request_id=\(requestID)",
            "agent_id=ui-agent",
            "run_id=ui-run",
            "client_session_id=ui-client",
            "reuse_policy=reuse_if_alive",
            "command=\(command)"
        ]
        for (key, value) in fields {
            pairs.append("\(key)=\(escape(value))")
        }
        let line = pairs.joined(separator: "|") + "\n"
        let responseLine = try sendLineToSocketLocked(line)
        return parseKVPairs(responseLine)
    }

    private func sendLineToSocketLocked(_ line: String) throws -> String {
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        if fd < 0 {
            throw WorkspaceDataSourceError.transport("create unix socket failed")
        }
        defer { Darwin.close(fd) }

        var address = sockaddr_un()
        address.sun_family = sa_family_t(AF_UNIX)
        let pathBytes = socketPath.utf8CString
        if pathBytes.count > MemoryLayout.size(ofValue: address.sun_path) {
            throw WorkspaceDataSourceError.transport("socket path too long")
        }
        withUnsafeMutableBytes(of: &address.sun_path) { rawBuffer in
            rawBuffer.initializeMemory(as: UInt8.self, repeating: 0)
            pathBytes.withUnsafeBytes { srcBuffer in
                rawBuffer.copyBytes(from: srcBuffer)
            }
        }
        let length = socklen_t(MemoryLayout<sa_family_t>.size + pathBytes.count)
        let connectResult = withUnsafePointer(to: &address) { ptr in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPtr in
                Darwin.connect(fd, sockaddrPtr, length)
            }
        }
        if connectResult != 0 {
            let logTail = coreLogTailLocked()
            if logTail.isEmpty {
                throw WorkspaceDataSourceError.transport(
                    "connect control-plane failed (socket: \(socketPath), log: \(coreLogPath))"
                )
            }
            throw WorkspaceDataSourceError.transport(
                "connect control-plane failed (socket: \(socketPath), log: \(coreLogPath))\n\(logTail)"
            )
        }

        let requestData = Array(line.utf8)
        let writeResult = requestData.withUnsafeBytes { bytes in
            Darwin.write(fd, bytes.baseAddress, bytes.count)
        }
        if writeResult < 0 {
            throw WorkspaceDataSourceError.transport("write control-plane request failed")
        }

        var buffer = [UInt8](repeating: 0, count: 1024)
        var responseData = Data()
        while true {
            let readCount = Darwin.read(fd, &buffer, buffer.count)
            if readCount <= 0 {
                break
            }
            responseData.append(buffer, count: Int(readCount))
            if buffer[..<Int(readCount)].contains(10) {
                break
            }
        }
        guard let text = String(data: responseData, encoding: .utf8)?
            .trimmingCharacters(in: .whitespacesAndNewlines),
              !text.isEmpty else {
            throw WorkspaceDataSourceError.transport("empty control-plane response")
        }
        return text
    }

    private func coreLogTailLocked(maxBytes: Int = 4096) -> String {
        guard let data = try? Data(contentsOf: URL(fileURLWithPath: coreLogPath)),
              !data.isEmpty else {
            return ""
        }
        let tail = data.suffix(maxBytes)
        return String(decoding: tail, as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func parseKVPairs(_ line: String) -> [String: String] {
        var map: [String: String] = [:]
        for token in line.split(separator: "|", omittingEmptySubsequences: true) {
            guard let idx = token.firstIndex(of: "=") else { continue }
            let key = String(token[..<idx])
            let value = String(token[token.index(after: idx)...])
            map[key] = value
        }
        return map
    }

    private func escape(_ value: String) -> String {
        value
            .replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "|", with: "\\p")
            .replacingOccurrences(of: "\n", with: "\\n")
    }

    private func unescape(_ value: String) -> String {
        value
            .replacingOccurrences(of: "\\n", with: "\n")
            .replacingOccurrences(of: "\\p", with: "|")
            .replacingOccurrences(of: "\\\\", with: "\\")
    }

    private func decodeBootstrapPayload(_ payload: String) throws -> WorkspaceSnapshot {
        guard let data = payload.data(using: .utf8),
              let root = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw WorkspaceDataSourceError.protocolViolation("invalid bootstrap payload")
        }

        let settingsPayload = root["settings"] as? [String: Any]
        let globalToolchains = decodeGlobalToolchains(settingsPayload)
        let globalToolchainByCommand = Dictionary(
            uniqueKeysWithValues: globalToolchains.map { ($0.command, $0.pathOverride) }
        )

        let sessionRows = root["sessions"] as? [[String: Any]] ?? []
        var sessionStateByID: [String: String] = [:]
        var targetBySessionID: [String: String] = [:]
        for row in sessionRows {
            if let sessionID = row["id"] as? String {
                sessionStateByID[sessionID] = row["state"] as? String ?? "waiting"
                targetBySessionID[sessionID] = row["target_id"] as? String ?? ""
            }
        }

        let transcriptRows = root["transcripts"] as? [[String: Any]] ?? []
        var transcriptByShellID: [String: [String]] = [:]
        for row in transcriptRows {
            let shellID = row["shell_id"] as? String ?? ""
            let lines = row["lines"] as? [String] ?? []
            transcriptByShellID[shellID] = lines
        }

        let shellRows = root["shell_channels"] as? [[String: Any]] ?? []
        var shellChannels: [ShellChannel] = []
        for row in shellRows {
            let coreTargetID = row["target_id"] as? String ?? ""
            guard !coreTargetID.isEmpty else { continue }
            let targetID = uuid(forCoreTargetID: coreTargetID)
            let shellID = row["shell_id"] as? String ?? UUID().uuidString
            let prompt = row["prompt"] as? String ?? "shell:$"
            let lines = (transcriptByShellID[shellID] ?? []).map { line in
                ShellLine(id: UUID(), role: .output, content: line, timestamp: .now)
            }
            let channel = ShellChannel(
                id: row["channel_id"] as? String ?? shellID,
                targetID: targetID,
                title: L10n.t("workspace.channel.title.interactive_shell"),
                prompt: prompt,
                isClosed: row["closed"] as? Bool ?? false,
                lines: lines
            )
            shellChannels.append(channel)
        }

        let diagnosticsRows = root["diagnostics"] as? [[String: Any]] ?? []
        var diagnosticsByTargetID: [String: [ToolSourceDiagnostic]] = [:]
        for row in diagnosticsRows {
            let command = row["command"] as? String ?? "tool"
            let targetID = row["target_id"] as? String ?? ""
            guard !targetID.isEmpty else { continue }
            let effectiveScope = (row["effective_scope"] as? String ?? "")
                .trimmingCharacters(in: .whitespacesAndNewlines)
            let selectedSource = (
                row["effective_source"] as? String
                ?? row["selected_source"] as? String
                ?? "system_path"
            ).lowercased()
            let sourceType: ToolSourceType
            if selectedSource.contains("bundled") || selectedSource.contains("builtin") {
                sourceType = .bundledFallback
            } else if selectedSource.contains("override") {
                sourceType = .userOverride
            } else {
                sourceType = .systemPath
            }
            let effectivePath = (row["effective_path"] as? String)
                ?? (row["selected_path"] as? String)
                ?? ""
            let targetOverridePath = (row["target_override_path"] as? String ?? "")
                .trimmingCharacters(in: .whitespacesAndNewlines)
            let globalOverridePath = (row["global_override_path"] as? String ?? "")
                .trimmingCharacters(in: .whitespacesAndNewlines)
            let diagnostic = ToolSourceDiagnostic(
                id: command,
                connectorName: command,
                sourceType: sourceType,
                effectivePath: effectivePath,
                overridePath: targetOverridePath,
                globalOverridePath: globalOverridePath,
                effectiveScope: effectiveScope,
                lastChecked: .now
            )
            diagnosticsByTargetID[targetID, default: []].append(diagnostic)
        }
        for (targetID, diagnostics) in diagnosticsByTargetID {
            diagnosticsByTargetID[targetID] = diagnostics.sorted { lhs, rhs in
                lhs.connectorName < rhs.connectorName
            }
        }

        let profileRows = root["profiles"] as? [[String: Any]] ?? []
        var profileByCoreID: [String: [String: Any]] = [:]
        for row in profileRows {
            if let id = row["id"] as? String {
                profileByCoreID[id] = row
            }
        }

        let targetRows = root["targets"] as? [[String: Any]] ?? []
        let targets: [TargetProfile] = targetRows.map { row in
            let coreID = row["id"] as? String ?? UUID().uuidString
            let targetID = uuid(forCoreTargetID: coreID)
            let sessionID = row["session_id"] as? String ?? ""
            let stateRaw = row["session_state"] as? String ?? sessionStateByID[sessionID] ?? "waiting"
            let sessionState = mapSessionState(stateRaw)
            let connectionState = mapConnectionState(stateRaw)
            let channelID = shellChannels.first(where: { $0.targetID == targetID })?.id ?? "channel-main"
            let capabilities = (row["capability_ids"] as? [String] ?? []).map { capabilityID in
                CapabilitySummary(id: capabilityID, title: capabilityTitle(for: capabilityID))
            }
            let profilePayload = profileByCoreID[coreID]
            let draft = profilePayload.flatMap { try? parseDraftFromProfilePayload($0) }
            let resolvedKind = draft?.kind ?? mapTargetKind(row["kind"] as? String)
            let resolvedName = {
                guard let draft else { return row["name"] as? String ?? coreID }
                let trimmed = draft.name.trimmingCharacters(in: .whitespacesAndNewlines)
                return trimmed.isEmpty ? (row["name"] as? String ?? coreID) : draft.name
            }()
            let resolvedAlias = {
                guard let draft else { return coreID }
                let trimmed = draft.aliasForModel.trimmingCharacters(in: .whitespacesAndNewlines)
                return trimmed.isEmpty ? coreID : draft.aliasForModel
            }()
            let fallbackDiagnostics = defaultToolDiagnostics(
                for: resolvedKind,
                globalToolchainByCommand: globalToolchainByCommand
            )
            let resolvedToolDiagnostics = diagnosticsByTargetID[coreID]
                ?? draft?.toolDiagnostics
                ?? fallbackDiagnostics
            return TargetProfile(
                id: targetID,
                coreID: coreID,
                name: resolvedName,
                kind: resolvedKind,
                aliasForModel: resolvedAlias,
                notes: draft?.notes ?? (row["notes"] as? String ?? ""),
                credentialReference: draft?.credentialReference ?? "vault://\(coreID)",
                policyDefaults: draft?.policyDefaults ?? .default,
                sshConfig: draft?.sshConfig ?? .empty,
                adbConfig: draft?.adbConfig ?? .empty,
                serialConfig: draft?.serialConfig ?? .empty,
                dockerConfig: draft?.dockerConfig ?? .empty,
                httpDebugConfig: draft?.httpDebugConfig ?? .empty,
                openGrokConfig: draft?.openGrokConfig ?? .empty,
                connectionState: connectionState,
                lastActivity: .now,
                sessionSummary: SessionSummary(
                    logicalSessionID: sessionID.isEmpty ? "session-none" : sessionID,
                    channelID: channelID,
                    state: sessionState,
                    lastHeartbeat: .now,
                    fingerprint: EnvironmentFingerprint(
                        osVersion: "unknown",
                        architecture: "unknown",
                        shell: "sh",
                        detectedTools: []
                    )
                ),
                capabilities: capabilities,
                toolDiagnostics: resolvedToolDiagnostics
            )
        }

        let artifactRows = root["artifacts"] as? [[String: Any]] ?? []
        let artifacts: [ArtifactRecord] = artifactRows.map { row in
            let hash = row["id"] as? String ?? UUID().uuidString
            return ArtifactRecord(
                hash: hash,
                contentDigest: row["content_digest"] as? String ?? "",
                sourceCommand: row["source_command"] as? String ?? "",
                summary: row["summary"] as? String ?? "",
                sessionID: row["session_id"] as? String ?? "",
                channelID: row["channel_id"] as? String ?? "",
                parentHashes: (row["parent_id"] as? String).map { [$0] } ?? [],
                derivedHashes: [],
                textContent: row["summary"] as? String ?? ""
            )
        }

        let timelineRows = root["timeline"] as? [[String: Any]] ?? []
        let timeline: [CommandTimelineItem] = timelineRows.compactMap { row in
            let coreTargetID = row["target_id"] as? String ?? ""
            let targetID = targetIDMap[coreTargetID] ?? targets.first?.id
            guard let targetID else { return nil }
            let artifactID = row["artifact_id"] as? String
            let references: [ArtifactReference] = artifactID.map { artifact in
                [ArtifactReference(id: artifact, hash: artifact, label: artifact)]
            } ?? []
            let timestamp = dateFromMillis(row["created_at_ms"])
            return CommandTimelineItem(
                id: UUID(),
                targetID: targetID,
                sessionID: row["session_id"] as? String ?? "",
                channelID: "",
                timestamp: timestamp,
                command: row["command_preview"] as? String ?? "",
                summary: row["status"] as? String ?? "",
                state: mapCommandState(row["status"] as? String),
                stdoutPreview: "",
                stderrPreview: "",
                exitStatus: 0,
                artifacts: references
            )
        }

        let approvalRows = root["approvals"] as? [[String: Any]] ?? []
        let approvals: [ApprovalRequestItem] = approvalRows.compactMap { row in
            let sessionID = row["session_id"] as? String ?? ""
            let targetCoreID = targetBySessionID[sessionID] ?? ""
            let targetID = targetIDMap[targetCoreID] ?? targets.first?.id
            guard let targetID else { return nil }
            return ApprovalRequestItem(
                id: UUID(),
                targetID: targetID,
                commandSummary: row["command_preview"] as? String ?? "",
                reason: row["reason"] as? String ?? "",
                createdAt: dateFromMillis(row["requested_at_ms"]),
                status: mapApprovalStatus(row["status"] as? String)
            )
        }

        let cacheSettings = decodeCacheSettings(settingsPayload)
        let modelPlane = decodeModelPlaneSettings(settingsPayload)
        let runtimeRoot = decodeRuntimeRootDiagnostics(root["runtime_root"] as? [String: Any])

        return WorkspaceSnapshot(
            targets: targets,
            timeline: timeline,
            artifacts: artifacts,
            approvals: approvals,
            shellChannels: shellChannels,
            cacheSettings: cacheSettings,
            modelPlaneHost: modelPlane.host,
            modelPlanePort: modelPlane.port,
            globalToolchains: globalToolchains,
            runtimeRootPath: runtimeRoot.path,
            runtimeRootAccessible: runtimeRoot.accessible
        )
    }

    private func decodeCacheSettings(_ settings: [String: Any]?) -> ArtifactCacheSettings {
        let artifactCache = settings?["artifact_cache"] as? [String: Any]
        let backendRaw = (artifactCache?["backend"] as? String ?? "filesystem").lowercased()
        let backend: ArtifactCacheBackend = backendRaw == "memory" ? .memory : .filesystem
        let evictionRaw = (artifactCache?["eviction_policy"] as? String ?? "lru").lowercased()
        let eviction: ArtifactEvictionPolicy = evictionRaw == "fifo" ? .fifo : .lru
        let usedBytes = artifactCache?["used_bytes"] as? Double ?? 0
        let maxBytes = artifactCache?["max_bytes"] as? Double ?? 0
        return ArtifactCacheSettings(
            backend: backend,
            rootPath: artifactCache?["root"] as? String ?? ArtifactCacheSettings.default.rootPath,
            maxCacheMB: max(256, Int(maxBytes / 1024 / 1024)),
            evictionPolicy: eviction,
            usedCacheMB: max(0, Int(usedBytes / 1024 / 1024))
        )
    }

    private func decodeGlobalToolchains(_ settings: [String: Any]?) -> [ToolchainSetting] {
        let rows = settings?["toolchains"] as? [[String: Any]] ?? []
        let entries = rows.compactMap { row -> ToolchainSetting? in
            let command = (row["command"] as? String ?? "")
                .trimmingCharacters(in: .whitespacesAndNewlines)
            guard !command.isEmpty else { return nil }
            let path = (row["path_override"] as? String ?? "")
                .trimmingCharacters(in: .whitespacesAndNewlines)
            return ToolchainSetting(command: command, pathOverride: path)
        }
        return entries.sorted { lhs, rhs in lhs.command < rhs.command }
    }

    private func defaultToolDiagnostics(
        for kind: TargetKind,
        globalToolchainByCommand: [String: String]
    ) -> [ToolSourceDiagnostic] {
        switch kind {
        case .ssh:
            let globalPath = globalToolchainByCommand["ssh"] ?? ""
            return [
                ToolSourceDiagnostic(
                    id: "ssh",
                    connectorName: "ssh",
                    sourceType: globalPath.isEmpty ? .systemPath : .userOverride,
                    effectivePath: globalPath.isEmpty ? "/usr/bin/ssh" : globalPath,
                    overridePath: "",
                    globalOverridePath: globalPath,
                    effectiveScope: globalPath.isEmpty ? "system_path" : "global_override",
                    lastChecked: .now
                )
            ]
        case .adb:
            let globalPath = globalToolchainByCommand["adb"] ?? ""
            return [
                ToolSourceDiagnostic(
                    id: "adb",
                    connectorName: "adb",
                    sourceType: globalPath.isEmpty ? .bundledFallback : .userOverride,
                    effectivePath: globalPath.isEmpty
                        ? L10n.t("seed.tool.effective_path.bridgingio_bundle")
                        : globalPath,
                    overridePath: "",
                    globalOverridePath: globalPath,
                    effectiveScope: globalPath.isEmpty ? "builtin_fallback" : "global_override",
                    lastChecked: .now
                )
            ]
        default:
            return []
        }
    }

    private func decodeModelPlaneSettings(_ settings: [String: Any]?) -> (host: String, port: Int) {
        let modelPlane = settings?["model_plane_http"] as? [String: Any]
        let host = modelPlane?["host"] as? String ?? "127.0.0.1"
        let portValue = modelPlane?["port"]
        if let port = portValue as? Int {
            return (host, port)
        }
        if let port = portValue as? Double {
            return (host, Int(port))
        }
        if let port = portValue as? String, let parsed = Int(port) {
            return (host, parsed)
        }
        return (host, 19718)
    }

    private func decodeRuntimeRootDiagnostics(_ root: [String: Any]?) -> (path: String?, accessible: Bool) {
        let path = root?["path"] as? String
        let accessible = root?["accessible"] as? Bool ?? true
        return (path, accessible)
    }

    private func parseDraftFromProfilePayload(_ payload: [String: Any]) throws -> TargetProfileDraft {
        var draft = TargetProfileDraft()
        draft.existingCoreID = payload["id"] as? String
        draft.name = payload["name"] as? String ?? ""
        draft.aliasForModel = payload["alias"] as? String ?? ""
        draft.notes = payload["notes"] as? String ?? ""
        draft.credentialReference = payload["credential_ref"] as? String ?? draft.credentialReference
        draft.kind = mapTargetKind(payload["kind"] as? String)

        let connection = payload["connection"] as? [String: Any] ?? [:]
        switch draft.kind {
        case .ssh:
            draft.sshConfig.host = connection["host"] as? String ?? ""
            if let port = connection["port"] as? Int {
                draft.sshConfig.port = port
            } else if let port = connection["port"] as? Double {
                draft.sshConfig.port = Int(port)
            }
            draft.sshConfig.username = connection["username"] as? String ?? ""
        case .adb:
            draft.adbConfig.serial = connection["serial"] as? String ?? ""
            draft.adbConfig.transport = connection["transport"] as? String ?? "usb"
        case .serial:
            draft.serialConfig.devicePath = connection["device"] as? String ?? draft.serialConfig.devicePath
            if let baud = connection["baud_rate"] as? Int {
                draft.serialConfig.baudRate = baud
            } else if let baud = connection["baud_rate"] as? Double {
                draft.serialConfig.baudRate = Int(baud)
            }
        case .docker:
            draft.dockerConfig.containerName = connection["container"] as? String ?? ""
            draft.dockerConfig.context = connection["context"] as? String ?? draft.dockerConfig.context
        case .httpDebug:
            let description = connection["description"] as? String ?? ""
            if let value = description.split(separator: ":", maxSplits: 1).dropFirst().first {
                draft.httpDebugConfig.baseURL = String(value)
            }
        case .openGrok:
            let description = connection["description"] as? String ?? ""
            if let value = description.split(separator: ":", maxSplits: 1).dropFirst().first {
                draft.openGrokConfig.endpoint = String(value)
            }
        }
        let toolchainRows = payload["toolchains"] as? [[String: Any]] ?? []
        var targetOverrides: [String: String] = [:]
        for row in toolchainRows {
            let command = (row["command"] as? String ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
            guard !command.isEmpty else { continue }
            let path = (row["path_override"] as? String ?? "")
                .trimmingCharacters(in: .whitespacesAndNewlines)
            targetOverrides[command] = path
        }
        draft.toolDiagnostics = ["ssh", "adb"]
            .map { command in
                ToolSourceDiagnostic(
                    id: command,
                    connectorName: command,
                    sourceType: .systemPath,
                    effectivePath: "",
                    overridePath: targetOverrides[command] ?? "",
                    lastChecked: .now
                )
            }
        return draft
    }

    private func toolchainCommands(for kind: TargetKind) -> [String] {
        switch kind {
        case .ssh:
            return ["ssh"]
        case .adb:
            return ["adb"]
        default:
            return []
        }
    }

    private func targetKindLabel(for kind: TargetKind) -> String {
        switch kind {
        case .ssh:
            return "ssh"
        case .adb:
            return "adb"
        case .serial:
            return "serial"
        case .docker:
            return "docker"
        case .httpDebug:
            return "http-debug"
        case .openGrok:
            return "open-grok"
        }
    }

    private func uuid(forCoreTargetID coreID: String) -> UUID {
        if let existing = targetIDMap[coreID] {
            return existing
        }
        let created = UUID(uuidString: coreID) ?? UUID()
        targetIDMap[coreID] = created
        return created
    }

    private func dateFromMillis(_ value: Any?) -> Date {
        if let millis = value as? Double {
            return Date(timeIntervalSince1970: millis / 1000.0)
        }
        if let millis = value as? Int {
            return Date(timeIntervalSince1970: Double(millis) / 1000.0)
        }
        return .now
    }

    private func mapTargetKind(_ raw: String?) -> TargetKind {
        switch (raw ?? "").lowercased() {
        case "ssh":
            return .ssh
        case "adb":
            return .adb
        case "serial":
            return .serial
        case "docker":
            return .docker
        case "http-debug", "httpdebug":
            return .httpDebug
        case "open-grok", "opengrok":
            return .openGrok
        default:
            return .ssh
        }
    }

    private func mapSessionState(_ raw: String) -> SessionState {
        switch raw.lowercased() {
        case "connected", "active":
            return .active
        case "degraded":
            return .degraded
        case "closed":
            return .closed
        default:
            return .waiting
        }
    }

    private func mapConnectionState(_ raw: String) -> TargetConnectionState {
        switch raw.lowercased() {
        case "connected", "active":
            return .connected
        case "degraded":
            return .degraded
        case "closed":
            return .disconnected
        default:
            return .idle
        }
    }

    private func mapCommandState(_ raw: String?) -> CommandExecutionState {
        switch (raw ?? "").lowercased() {
        case "success", "connected", "active":
            return .success
        case "waiting_approval":
            return .waitingApproval
        case "streaming":
            return .streaming
        default:
            return .failed
        }
    }

    private func mapApprovalStatus(_ raw: String?) -> ApprovalStatus {
        switch (raw ?? "").lowercased() {
        case "approved":
            return .approved
        case "denied", "rejected":
            return .rejected
        case "expired":
            return .failed
        default:
            return .pending
        }
    }

    private func capabilityTitle(for capabilityID: String) -> String {
        switch capabilityID {
        case "terminal.exec":
            return L10n.t("capability.terminal")
        case "artifact.reanalysis":
            return L10n.t("capability.artifacts")
        case "git.query":
            return L10n.t("capability.git")
        default:
            return capabilityID
        }
    }
}

enum WorkspaceDataSourceError: Error {
    case needsRuntimeRoot
    case runtimeRootUnavailable(String)
    case waitingForAttach
    case transport(String)
    case protocolViolation(String)
}

struct WorkspaceSnapshot {
    var targets: [TargetProfile]
    var timeline: [CommandTimelineItem]
    var artifacts: [ArtifactRecord]
    var approvals: [ApprovalRequestItem]
    var shellChannels: [ShellChannel]
    var cacheSettings: ArtifactCacheSettings
    var modelPlaneHost: String
    var modelPlanePort: Int
    var globalToolchains: [ToolchainSetting]
    var runtimeRootPath: String?
    var runtimeRootAccessible: Bool
}

protocol WorkspaceDataSource: AnyObject {
    nonisolated func bootstrapSnapshot() throws -> WorkspaceSnapshot
    nonisolated func refreshSnapshot() throws -> WorkspaceSnapshot
    nonisolated func shutdown()
}

protocol ManagedWorkspaceDataSource: WorkspaceDataSource {
    nonisolated func fetchProfileDraft(coreID: String) throws -> TargetProfileDraft
    nonisolated func upsertProfile(draft: TargetProfileDraft) throws -> String
    nonisolated func updateModelPlane(host: String, port: Int) throws -> String
    nonisolated func updateArtifactCache(_ settings: ArtifactCacheSettings) throws -> String
    nonisolated func updateToolOverride(connectorID: String, newPath: String) throws -> String
    nonisolated func clearArtifactCache() throws -> String
    nonisolated func setRuntimeRoot(path: String)
    nonisolated func controlledRestart() throws -> WorkspaceSnapshot
}

final class FixtureWorkspaceDataSource: WorkspaceDataSource {
    private let snapshot: WorkspaceSnapshot

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
}

@MainActor
final class WorkspaceViewModel: ObservableObject {
    @Published var appearanceMode: AppearanceMode = .system
    @Published var searchQuery: String = ""
    @Published var targetFilter: TargetFilter = .all
    @Published var selectedTargetID: UUID?
    @Published var centerPanel: WorkspaceCenterPanel = .timeline
    @Published var artifactDrawerExpanded: Bool = false
    @Published var selectedArtifactHash: String?
    @Published var artifactLookupHash: String = ""
    @Published var artifactLookupResult: ArtifactRecord?
    @Published var refineKeyword: String = ""
    @Published var refineRangeStart: Int = 1
    @Published var refineRangeEnd: Int = 80
    @Published var shellInput: String = ""
    @Published var cacheSettings: ArtifactCacheSettings = .default
    @Published var cacheSettingsNeedRestart: Bool = false
    @Published var modelPlaneHost: String = "127.0.0.1"
    @Published var modelPlanePort: Int = 19718
    @Published var globalToolchains: [ToolchainSetting] = []
    @Published var targetEditorContext: TargetEditorContext?
    @Published var isShowingSettingsSheet: Bool = false
    @Published private(set) var coreConnectionState: WorkspaceCoreConnectionState = .startingCore

    @Published private(set) var targets: [TargetProfile]
    @Published private(set) var timeline: [CommandTimelineItem]
    @Published private(set) var artifacts: [ArtifactRecord]
    @Published private(set) var approvals: [ApprovalRequestItem]
    @Published private(set) var shellChannels: [ShellChannel]

    private let dataSource: WorkspaceDataSource
    private let isFixtureDataSource: Bool
    private var refreshTimer: Timer?
    private var refreshInFlight = false
    private var appliedCacheBackend: ArtifactCacheBackend
    private var runtimeRootPath: String?

    convenience init() {
        self.init(dataSource: ManagedCoreWorkspaceDataSource())
    }

    init(dataSource: WorkspaceDataSource) {
        self.dataSource = dataSource
        isFixtureDataSource = dataSource is FixtureWorkspaceDataSource
        targets = []
        timeline = []
        artifacts = []
        approvals = []
        shellChannels = []
        appliedCacheBackend = .filesystem
        loadInitialSnapshot()
    }

    private var managedDataSource: (any ManagedWorkspaceDataSource)? {
        dataSource as? any ManagedWorkspaceDataSource
    }

    deinit {
        refreshTimer?.invalidate()
        dataSource.shutdown()
    }

    var connectionStatusText: String {
        switch coreConnectionState {
        case .needsRuntimeRoot:
            return "Select runtime root to start managed core"
        case .runtimeRootUnavailable:
            return "Saved runtime root unavailable"
        case .startingCore:
            return L10n.t("workspace.connection.starting")
        case .attachingUI:
            return L10n.t("workspace.connection.waiting")
        case .connected:
            return L10n.t("workspace.connection.connected")
        case .savingChanges:
            return "Saving changes..."
        case .restartRequired:
            return "Restart required to apply changes"
        case .restartingCore:
            return "Restarting managed core..."
        case .restartFailed(let message):
            return "Restart failed: \(message)"
        case .attachFailed(let message):
            return L10n.f("workspace.connection.failed_format", message)
        }
    }

    private func loadInitialSnapshot() {
        coreConnectionState = .startingCore
        if !(dataSource is ManagedCoreWorkspaceDataSource) {
            do {
                let snapshot = try dataSource.bootstrapSnapshot()
                applySnapshot(snapshot)
                coreConnectionState = .connected
            } catch WorkspaceDataSourceError.needsRuntimeRoot {
                coreConnectionState = .needsRuntimeRoot
            } catch WorkspaceDataSourceError.runtimeRootUnavailable(let path) {
                coreConnectionState = .runtimeRootUnavailable(path)
            } catch WorkspaceDataSourceError.waitingForAttach {
                coreConnectionState = .attachingUI
            } catch {
                coreConnectionState = .attachFailed(message(for: error))
            }
            return
        }
        let source = dataSource
        Task.detached(priority: .userInitiated) { [weak self] in
            guard let self else { return }
            do {
                let snapshot = try source.bootstrapSnapshot()
                await MainActor.run {
                    self.applySnapshot(snapshot)
                    self.coreConnectionState = .connected
                    self.startRefreshTimer()
                }
            } catch WorkspaceDataSourceError.needsRuntimeRoot {
                await MainActor.run {
                    self.coreConnectionState = .needsRuntimeRoot
                }
            } catch WorkspaceDataSourceError.runtimeRootUnavailable(let path) {
                await MainActor.run {
                    self.coreConnectionState = .runtimeRootUnavailable(path)
                }
            } catch WorkspaceDataSourceError.waitingForAttach {
                await MainActor.run {
                    self.coreConnectionState = .attachingUI
                    self.startRefreshTimer()
                }
            } catch {
                await MainActor.run {
                    self.coreConnectionState = .attachFailed(self.message(for: error))
                    self.startRefreshTimer()
                }
            }
        }
    }

    private func startRefreshTimer() {
        guard refreshTimer == nil else { return }
        refreshTimer = Timer.scheduledTimer(withTimeInterval: 0.8, repeats: true) { [weak self] _ in
            Task { @MainActor [weak self] in
                self?.refreshFromCore()
            }
        }
    }

    private func refreshFromCore() {
        guard !refreshInFlight else { return }
        refreshInFlight = true
        let source = dataSource
        Task.detached(priority: .utility) { [weak self] in
            guard let self else { return }
            defer {
                Task { @MainActor in
                    self.refreshInFlight = false
                }
            }
            do {
                let snapshot = try source.refreshSnapshot()
                await MainActor.run {
                    self.applySnapshot(snapshot)
                    self.coreConnectionState = .connected
                }
            } catch WorkspaceDataSourceError.needsRuntimeRoot {
                await MainActor.run {
                    self.coreConnectionState = .needsRuntimeRoot
                }
            } catch WorkspaceDataSourceError.runtimeRootUnavailable(let path) {
                await MainActor.run {
                    self.coreConnectionState = .runtimeRootUnavailable(path)
                }
            } catch WorkspaceDataSourceError.waitingForAttach {
                await MainActor.run {
                    self.coreConnectionState = .attachingUI
                }
            } catch {
                await MainActor.run {
                    self.coreConnectionState = .attachFailed(self.message(for: error))
                }
            }
        }
    }

    private func applySnapshot(_ snapshot: WorkspaceSnapshot) {
        targets = snapshot.targets
        timeline = snapshot.timeline
        artifacts = snapshot.artifacts
        approvals = snapshot.approvals
        shellChannels = snapshot.shellChannels
        cacheSettings = snapshot.cacheSettings
        modelPlaneHost = snapshot.modelPlaneHost
        modelPlanePort = snapshot.modelPlanePort
        globalToolchains = snapshot.globalToolchains
        runtimeRootPath = snapshot.runtimeRootPath
        if !snapshot.runtimeRootAccessible, let path = snapshot.runtimeRootPath {
            coreConnectionState = .runtimeRootUnavailable(path)
        }
        appliedCacheBackend = cacheSettings.backend

        if let selectedTargetID,
           !targets.contains(where: { $0.id == selectedTargetID }) {
            self.selectedTargetID = targets.first?.id
        } else if self.selectedTargetID == nil {
            self.selectedTargetID = targets.first?.id
        }

        if let selectedArtifactHash,
           !artifacts.contains(where: { $0.hash == selectedArtifactHash }) {
            self.selectedArtifactHash = artifacts.first?.hash
        } else if self.selectedArtifactHash == nil {
            self.selectedArtifactHash = artifacts.first?.hash
        }

        if let hash = selectedArtifactHash {
            artifactLookupHash = hash
        } else {
            artifactLookupHash = ""
        }
        artifactLookupResult = selectedArtifactHash.flatMap { hash in
            artifacts.first(where: { $0.hash == hash })
        }
    }

    private func message(for error: WorkspaceDataSourceError) -> String {
        switch error {
        case .needsRuntimeRoot:
            return "runtime root is required"
        case .runtimeRootUnavailable(let path):
            return "runtime root unavailable: \(path)"
        case .waitingForAttach:
            return L10n.t("workspace.connection.waiting")
        case .transport(let message):
            return message
        case .protocolViolation(let message):
            return message
        }
    }

    private func message(for error: Error) -> String {
        if let dataSourceError = error as? WorkspaceDataSourceError {
            return message(for: dataSourceError)
        }
        let nsError = error as NSError
        if let underlying = nsError.userInfo[NSUnderlyingErrorKey] as? WorkspaceDataSourceError {
            return message(for: underlying)
        }
        return error.localizedDescription
    }

    private func relevantToolchainCommands(for kind: TargetKind) -> Set<String> {
        switch kind {
        case .ssh:
            return ["ssh"]
        case .adb:
            return ["adb"]
        default:
            return []
        }
    }

    var filteredTargets: [TargetProfile] {
        targets
            .filter { targetFilter.matches($0) }
            .filter {
                if searchQuery.isEmpty { return true }
                return $0.name.localizedCaseInsensitiveContains(searchQuery)
                    || $0.aliasForModel.localizedCaseInsensitiveContains(searchQuery)
            }
    }

    var selectedTarget: TargetProfile? {
        guard let selectedTargetID else { return nil }
        return targets.first(where: { $0.id == selectedTargetID })
    }

    var selectedTimelineItems: [CommandTimelineItem] {
        guard let selectedTargetID else { return [] }
        return timeline
            .filter { $0.targetID == selectedTargetID }
            .sorted(by: { $0.timestamp > $1.timestamp })
    }

    var selectedApprovals: [ApprovalRequestItem] {
        guard let selectedTargetID else { return [] }
        return approvals
            .filter { $0.targetID == selectedTargetID }
            .sorted(by: { $0.createdAt > $1.createdAt })
    }

    var selectedArtifact: ArtifactRecord? {
        guard let selectedArtifactHash else { return nil }
        return artifacts.first(where: { $0.hash == selectedArtifactHash })
    }

    var selectedChannels: [ShellChannel] {
        guard let selectedTargetID else { return [] }
        return shellChannels.filter { $0.targetID == selectedTargetID }
    }

    var selectedChannel: ShellChannel? {
        selectedChannels.first
    }

    var refinedArtifactText: String {
        guard let artifact = selectedArtifact else { return "" }

        let lines = artifact.textContent.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
        let start = max(1, refineRangeStart)
        let end = max(start, refineRangeEnd)
        let boundedStart = min(start, lines.count)
        let boundedEnd = min(end, lines.count)

        let sliced = lines[(boundedStart - 1)..<boundedEnd]
        let filtered: [String]
        if refineKeyword.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            filtered = Array(sliced)
        } else {
            filtered = sliced.filter { $0.localizedCaseInsensitiveContains(refineKeyword) }
        }
        return filtered.joined(separator: "\n")
    }

    func selectTarget(_ target: TargetProfile) {
        selectedTargetID = target.id
        if let artifactHash = timeline.first(where: { $0.targetID == target.id })?.artifacts.first?.hash {
            selectedArtifactHash = artifactHash
            artifactLookupHash = artifactHash
            artifactLookupResult = artifacts.first(where: { $0.hash == artifactHash })
        }
    }

    func openCreateTargetSheet() {
        guard coreConnectionState == .connected || isFixtureDataSource else { return }
        targetEditorContext = TargetEditorContext(mode: .create, draft: TargetProfileDraft())
    }

    func openEditTargetSheet(for target: TargetProfile) {
        if isFixtureDataSource {
            targetEditorContext = TargetEditorContext(mode: .edit, draft: TargetProfileDraft(profile: target))
            return
        }
        guard let managed = managedDataSource else { return }
        do {
            var draft = try managed.fetchProfileDraft(coreID: target.coreID)
            let relevantCommands = relevantToolchainCommands(for: draft.kind)
            let diagnosticsFromTarget = target.toolDiagnostics
                .filter { relevantCommands.contains($0.id) }
            if !diagnosticsFromTarget.isEmpty {
                draft.toolDiagnostics = diagnosticsFromTarget
            } else {
                draft.toolDiagnostics = draft.toolDiagnostics
                    .filter { relevantCommands.contains($0.id) }
            }
            targetEditorContext = TargetEditorContext(mode: .edit, draft: draft)
        } catch {
            coreConnectionState = .attachFailed(message(for: error))
        }
    }

    func saveTarget(draft: TargetProfileDraft) {
        if !isFixtureDataSource {
            guard let managed = managedDataSource else { return }
            coreConnectionState = .savingChanges
            do {
                let strategy = try managed.upsertProfile(draft: draft)
                targetEditorContext = nil
                try applyManagedWriteStrategy(strategy, managed: managed)
            } catch {
                coreConnectionState = .attachFailed(message(for: error))
            }
            return
        }
        let now = Date()
        if let existingID = draft.existingID,
           let index = targets.firstIndex(where: { $0.id == existingID }) {
            targets[index] = buildProfile(from: draft, id: existingID, createdAt: targets[index].lastActivity)
            targets[index].lastActivity = now
            selectedTargetID = existingID
        } else {
            let newTarget = buildProfile(from: draft, id: UUID(), createdAt: now)
            targets.insert(newTarget, at: 0)
            selectedTargetID = newTarget.id
        }
        targetEditorContext = nil
    }

    func updateToolOverride(connectorID: String, newPath: String) {
        if !isFixtureDataSource {
            guard let managed = managedDataSource else { return }
            coreConnectionState = .savingChanges
            do {
                let strategy = try managed.updateToolOverride(connectorID: connectorID, newPath: newPath)
                try applyManagedWriteStrategy(strategy, managed: managed)
            } catch {
                coreConnectionState = .attachFailed(message(for: error))
            }
            return
        }
        let trimmed = newPath.trimmingCharacters(in: .whitespacesAndNewlines)
        if let index = globalToolchains.firstIndex(where: { $0.command == connectorID }) {
            globalToolchains[index].pathOverride = trimmed
        } else {
            globalToolchains.append(
                ToolchainSetting(command: connectorID, pathOverride: trimmed)
            )
        }

        for targetIndex in targets.indices {
            for diagIndex in targets[targetIndex].toolDiagnostics.indices where
                targets[targetIndex].toolDiagnostics[diagIndex].id == connectorID &&
                targets[targetIndex].toolDiagnostics[diagIndex].overridePath
                    .trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            {
                targets[targetIndex].toolDiagnostics[diagIndex].globalOverridePath = trimmed
                if trimmed.isEmpty {
                    targets[targetIndex].toolDiagnostics[diagIndex].sourceType =
                        targets[targetIndex].toolDiagnostics[diagIndex].id == "adb"
                        ? .bundledFallback
                        : .systemPath
                    targets[targetIndex].toolDiagnostics[diagIndex].effectiveScope =
                        targets[targetIndex].toolDiagnostics[diagIndex].id == "adb"
                        ? "builtin_fallback"
                        : "system_path"
                } else {
                    targets[targetIndex].toolDiagnostics[diagIndex].sourceType = .userOverride
                    targets[targetIndex].toolDiagnostics[diagIndex].effectivePath = trimmed
                    targets[targetIndex].toolDiagnostics[diagIndex].effectiveScope = "global_override"
                }
                targets[targetIndex].toolDiagnostics[diagIndex].lastChecked = .now
            }
        }
    }

    func updateGlobalToolchainDraft(command: String, path: String) {
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        if let index = globalToolchains.firstIndex(where: { $0.command == command }) {
            globalToolchains[index].pathOverride = trimmed
        } else {
            globalToolchains.append(ToolchainSetting(command: command, pathOverride: trimmed))
        }
    }

    func saveGlobalToolchain(command: String) {
        guard let setting = globalToolchains.first(where: { $0.command == command }) else {
            return
        }
        updateToolOverride(connectorID: setting.command, newPath: setting.pathOverride)
    }

    func selectArtifact(hash: String) {
        selectedArtifactHash = hash
        artifactLookupHash = hash
        artifactLookupResult = artifacts.first(where: { $0.hash == hash })
        artifactDrawerExpanded = true
    }

    func lookupArtifactByHash() {
        let query = artifactLookupHash.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else {
            artifactLookupResult = nil
            return
        }

        artifactLookupResult = artifacts.first(where: { $0.hash.hasPrefix(query) })
        if let hash = artifactLookupResult?.hash {
            selectedArtifactHash = hash
        }
    }

    func approve(_ request: ApprovalRequestItem) {
        guard isFixtureDataSource else { return }
        guard let index = approvals.firstIndex(where: { $0.id == request.id }) else { return }
        approvals[index].status = .approved

        if let timelineIndex = timeline.firstIndex(where: {
            $0.targetID == request.targetID && $0.command.contains(request.commandSummary)
        }) {
            timeline[timelineIndex].state = .success
            timeline[timelineIndex].summary = L10n.t("workspace.timeline.summary.operator_approved")
            timeline[timelineIndex].exitStatus = 0
        }
    }

    func reject(_ request: ApprovalRequestItem) {
        guard isFixtureDataSource else { return }
        guard let index = approvals.firstIndex(where: { $0.id == request.id }) else { return }
        approvals[index].status = .rejected

        if let timelineIndex = timeline.firstIndex(where: {
            $0.targetID == request.targetID && $0.command.contains(request.commandSummary)
        }) {
            timeline[timelineIndex].state = .failed
            timeline[timelineIndex].summary = L10n.t("workspace.timeline.summary.operator_rejected")
            timeline[timelineIndex].exitStatus = 126
        }
    }

    func openChannel() {
        guard isFixtureDataSource else { return }
        guard let selectedTargetID else { return }
        let newChannel = ShellChannel(
            id: "ch-\(Int.random(in: 100...999))",
            targetID: selectedTargetID,
            title: L10n.t("workspace.channel.title.interactive_shell"),
            prompt: "ops@host$",
            isClosed: false,
            lines: [
                ShellLine(id: UUID(), role: .info, content: L10n.t("workspace.channel.opened_attached"), timestamp: .now)
            ]
        )
        shellChannels.insert(newChannel, at: 0)
        centerPanel = .transcript
    }

    func sendShellInput() {
        guard isFixtureDataSource else { return }
        guard let targetID = selectedTargetID,
              let channelIndex = shellChannels.firstIndex(where: { $0.targetID == targetID }),
              !shellInput.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
              !shellChannels[channelIndex].isClosed else {
            return
        }

        let input = shellInput
        shellChannels[channelIndex].lines.append(
            ShellLine(id: UUID(), role: .input, content: "\(shellChannels[channelIndex].prompt) \(input)", timestamp: .now)
        )

        shellChannels[channelIndex].lines.append(
            ShellLine(
                id: UUID(),
                role: .output,
                content: L10n.f(
                    "workspace.channel.executed_on_format",
                    input,
                    targets.first(where: { $0.id == targetID })?.name ?? L10n.t("workspace.channel.default_target_name")
                ),
                timestamp: .now
            )
        )
        shellInput = ""
    }

    func interruptChannel() {
        guard isFixtureDataSource else { return }
        guard let targetID = selectedTargetID,
              let index = shellChannels.firstIndex(where: { $0.targetID == targetID }),
              !shellChannels[index].isClosed else {
            return
        }

        shellChannels[index].lines.append(
            ShellLine(id: UUID(), role: .info, content: L10n.t("workspace.channel.sigint_sent"), timestamp: .now)
        )
    }

    func closeChannel() {
        guard isFixtureDataSource else { return }
        guard let targetID = selectedTargetID,
              let index = shellChannels.firstIndex(where: { $0.targetID == targetID }),
              !shellChannels[index].isClosed else {
            return
        }

        shellChannels[index].isClosed = true
        shellChannels[index].lines.append(
            ShellLine(id: UUID(), role: .info, content: L10n.t("workspace.channel.closed_by_operator"), timestamp: .now)
        )
    }

    func updateCacheBackend(_ backend: ArtifactCacheBackend) {
        cacheSettings.backend = backend
        cacheSettingsNeedRestart = cacheSettings.backend != appliedCacheBackend
    }

    func applyCacheSettings() {
        if !isFixtureDataSource {
            guard let managed = managedDataSource else { return }
            coreConnectionState = .savingChanges
            do {
                let strategy = try managed.updateArtifactCache(cacheSettings)
                try applyManagedWriteStrategy(strategy, managed: managed)
            } catch {
                coreConnectionState = .attachFailed(message(for: error))
            }
            return
        }
        appliedCacheBackend = cacheSettings.backend
        cacheSettingsNeedRestart = false
    }

    func clearArtifactCache() {
        if !isFixtureDataSource {
            guard let managed = managedDataSource else { return }
            coreConnectionState = .savingChanges
            do {
                let strategy = try managed.clearArtifactCache()
                try applyManagedWriteStrategy(strategy, managed: managed)
            } catch {
                coreConnectionState = .attachFailed(message(for: error))
            }
            return
        }
        cacheSettings.usedCacheMB = 0
    }

    func saveModelPlaneSettings() {
        guard !isFixtureDataSource else { return }
        guard let managed = managedDataSource else { return }
        coreConnectionState = .savingChanges
        do {
            let strategy = try managed.updateModelPlane(host: modelPlaneHost, port: modelPlanePort)
            try applyManagedWriteStrategy(strategy, managed: managed)
        } catch {
            coreConnectionState = .attachFailed(message(for: error))
        }
    }

    func chooseRuntimeRoot() {
        guard let managed = managedDataSource else { return }
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        panel.prompt = "Select"
        panel.message = "Choose a runtime root folder for BridgingIO managed core."
        if panel.runModal() == .OK, let url = panel.url {
            managed.setRuntimeRoot(path: url.path)
            coreConnectionState = .startingCore
            loadInitialSnapshot()
        }
    }

    func retryManagedConnection() {
        guard !isFixtureDataSource else { return }
        coreConnectionState = .startingCore
        loadInitialSnapshot()
    }

    private func applyManagedWriteStrategy(
        _ strategyRaw: String,
        managed: any ManagedWorkspaceDataSource
    ) throws {
        let strategy = strategyRaw.lowercased()
        if strategy == "restart_required" {
            coreConnectionState = .restartRequired
            coreConnectionState = .restartingCore
            do {
                let snapshot = try managed.controlledRestart()
                applySnapshot(snapshot)
                coreConnectionState = .connected
            } catch {
                coreConnectionState = .restartFailed(message(for: error))
                return
            }
        } else {
            let snapshot = try managed.refreshSnapshot()
            applySnapshot(snapshot)
            coreConnectionState = .connected
        }
        cacheSettingsNeedRestart = false
    }

    private func buildProfile(from draft: TargetProfileDraft, id: UUID, createdAt: Date) -> TargetProfile {
        let diagnostics = draft.toolDiagnostics.filter {
            relevantToolchainCommands(for: draft.kind).contains($0.id)
        }
        return TargetProfile(
            id: id,
            coreID: draft.existingCoreID ?? id.uuidString.lowercased(),
            name: draft.name,
            kind: draft.kind,
            aliasForModel: draft.aliasForModel,
            notes: draft.notes,
            credentialReference: draft.credentialReference,
            policyDefaults: draft.policyDefaults,
            sshConfig: draft.sshConfig,
            adbConfig: draft.adbConfig,
            serialConfig: draft.serialConfig,
            dockerConfig: draft.dockerConfig,
            httpDebugConfig: draft.httpDebugConfig,
            openGrokConfig: draft.openGrokConfig,
            connectionState: .idle,
            lastActivity: createdAt,
            sessionSummary: SessionSummary(
                logicalSessionID: "session-\(id.uuidString.prefix(8))",
                channelID: "channel-main",
                state: .waiting,
                lastHeartbeat: .now,
                fingerprint: EnvironmentFingerprint(
                    osVersion: "macOS 15.2",
                    architecture: "arm64",
                    shell: "zsh",
                    detectedTools: ["ssh", "git", "python3"]
                )
            ),
            capabilities: [
                CapabilitySummary(id: "terminal", title: L10n.t("capability.terminal")),
                CapabilitySummary(id: "artifacts", title: L10n.t("capability.artifacts")),
                CapabilitySummary(id: "approvals", title: L10n.t("capability.approvals"))
            ],
            toolDiagnostics: diagnostics
        )
    }

    static func fixtureDataSource() -> WorkspaceDataSource {
        FixtureWorkspaceDataSource(snapshot: fixtureSnapshot())
    }

    static func fixtureSnapshot() -> WorkspaceSnapshot {
        let seed = makeSeed()
        return WorkspaceSnapshot(
            targets: seed.targets,
            timeline: seed.timeline,
            artifacts: seed.artifacts,
            approvals: seed.approvals,
            shellChannels: seed.shellChannels,
            cacheSettings: seed.cacheSettings,
            modelPlaneHost: "127.0.0.1",
            modelPlanePort: 19718,
            globalToolchains: seed.globalToolchains,
            runtimeRootPath: "~/Library/Application Support/BridgingIO",
            runtimeRootAccessible: true
        )
    }

    private static func makeSeed() -> (
        targets: [TargetProfile],
        timeline: [CommandTimelineItem],
        artifacts: [ArtifactRecord],
        approvals: [ApprovalRequestItem],
        shellChannels: [ShellChannel],
        cacheSettings: ArtifactCacheSettings,
        globalToolchains: [ToolchainSetting]
    ) {
        let now = Date()
        let opsID = UUID(uuidString: "4F5D6A8A-53AB-4F47-9A3E-4EA49FE4E1B1") ?? UUID()
        let pixelID = UUID(uuidString: "C2CA0E7F-8D16-47CB-B2E9-42DBAB53E874") ?? UUID()
        let labID = UUID(uuidString: "BA6F0462-797A-4CE6-872F-37E029A7CC7C") ?? UUID()

        let targets = [
            TargetProfile(
                id: opsID,
                coreID: "ops-prod",
                name: "ops-prod",
                kind: .ssh,
                aliasForModel: "prod",
                notes: L10n.t("seed.target.ops.notes"),
                credentialReference: "vault:ssh-key:ops-prod",
                policyDefaults: .default,
                sshConfig: SSHConnectionConfig(host: "10.0.0.8", port: 22, username: "ops"),
                adbConfig: .empty,
                serialConfig: .empty,
                dockerConfig: .empty,
                httpDebugConfig: .empty,
                openGrokConfig: .empty,
                connectionState: .connected,
                lastActivity: now.addingTimeInterval(-12),
                sessionSummary: SessionSummary(
                    logicalSessionID: "ls-ops-prod",
                    channelID: "ch-main",
                    state: .active,
                    lastHeartbeat: now.addingTimeInterval(-4),
                    fingerprint: EnvironmentFingerprint(
                        osVersion: "Ubuntu 24.04",
                        architecture: "x64",
                        shell: "zsh",
                        detectedTools: ["ssh", "git", "python3", "rg"]
                    )
                ),
                capabilities: [
                    CapabilitySummary(id: "terminal", title: L10n.t("capability.terminal")),
                    CapabilitySummary(id: "git", title: L10n.t("capability.git")),
                    CapabilitySummary(id: "artifacts", title: L10n.t("capability.artifacts")),
                    CapabilitySummary(id: "approvals", title: L10n.t("capability.approvals"))
                ],
                toolDiagnostics: [
                    ToolSourceDiagnostic(
                        id: "ssh",
                        connectorName: "ssh",
                        sourceType: .systemPath,
                        effectivePath: "/usr/bin/ssh",
                        overridePath: "",
                        globalOverridePath: "",
                        effectiveScope: "system_path",
                        lastChecked: now.addingTimeInterval(-30)
                    )
                ]
            ),
            TargetProfile(
                id: pixelID,
                coreID: "pixel-8",
                name: "pixel-8",
                kind: .adb,
                aliasForModel: "android-main",
                notes: L10n.t("seed.target.pixel.notes"),
                credentialReference: "vault:adb-key:pixel8",
                policyDefaults: .default,
                sshConfig: .empty,
                adbConfig: ADBConnectionConfig(serial: "ABC12345", transport: "usb"),
                serialConfig: .empty,
                dockerConfig: .empty,
                httpDebugConfig: .empty,
                openGrokConfig: .empty,
                connectionState: .idle,
                lastActivity: now.addingTimeInterval(-75),
                sessionSummary: SessionSummary(
                    logicalSessionID: "ls-pixel-8",
                    channelID: "ch-adb",
                    state: .waiting,
                    lastHeartbeat: now.addingTimeInterval(-75),
                    fingerprint: EnvironmentFingerprint(
                        osVersion: "Android 15",
                        architecture: "arm64",
                        shell: "sh",
                        detectedTools: ["adb", "logcat", "pm"]
                    )
                ),
                capabilities: [
                    CapabilitySummary(id: "terminal", title: L10n.t("capability.terminal")),
                    CapabilitySummary(id: "artifacts", title: L10n.t("capability.artifacts"))
                ],
                toolDiagnostics: [
                    ToolSourceDiagnostic(
                        id: "adb",
                        connectorName: "adb",
                        sourceType: .userOverride,
                        effectivePath: "/opt/homebrew/bin/adb",
                        overridePath: "",
                        globalOverridePath: "/opt/homebrew/bin/adb",
                        effectiveScope: "global_override",
                        lastChecked: now.addingTimeInterval(-90)
                    )
                ]
            ),
            TargetProfile(
                id: labID,
                coreID: "lab-host",
                name: "lab-host",
                kind: .ssh,
                aliasForModel: "lab",
                notes: L10n.t("seed.target.lab.notes"),
                credentialReference: "vault:ssh-key:lab-host",
                policyDefaults: .default,
                sshConfig: SSHConnectionConfig(host: "10.0.1.12", port: 22, username: "bridge"),
                adbConfig: .empty,
                serialConfig: .empty,
                dockerConfig: .empty,
                httpDebugConfig: .empty,
                openGrokConfig: .empty,
                connectionState: .degraded,
                lastActivity: now.addingTimeInterval(-130),
                sessionSummary: SessionSummary(
                    logicalSessionID: "ls-lab-host",
                    channelID: "ch-main",
                    state: .degraded,
                    lastHeartbeat: now.addingTimeInterval(-130),
                    fingerprint: EnvironmentFingerprint(
                        osVersion: "Ubuntu 22.04",
                        architecture: "x64",
                        shell: "bash",
                        detectedTools: ["ssh", "git"]
                    )
                ),
                capabilities: [
                    CapabilitySummary(id: "terminal", title: L10n.t("capability.terminal")),
                    CapabilitySummary(id: "artifacts", title: L10n.t("capability.artifacts"))
                ],
                toolDiagnostics: [
                    ToolSourceDiagnostic(
                        id: "ssh",
                        connectorName: "ssh",
                        sourceType: .systemPath,
                        effectivePath: "/usr/bin/ssh",
                        overridePath: "",
                        globalOverridePath: "",
                        effectiveScope: "system_path",
                        lastChecked: now.addingTimeInterval(-130)
                    )
                ]
            )
        ]

        let timeline = [
            CommandTimelineItem(
                id: UUID(),
                targetID: opsID,
                sessionID: "ls-ops-prod",
                channelID: "ch-main",
                timestamp: now.addingTimeInterval(-120),
                command: "git status",
                summary: L10n.t("seed.timeline.git_status.summary"),
                state: .success,
                stdoutPreview: "On branch main\\nnothing to commit, working tree clean",
                stderrPreview: "",
                exitStatus: 0,
                artifacts: [
                    ArtifactReference(id: "art-8cf4e2", hash: "8cf4e2a1d7b2", label: "art-8cf4e2")
                ]
            ),
            CommandTimelineItem(
                id: UUID(),
                targetID: opsID,
                sessionID: "ls-ops-prod",
                channelID: "ch-main",
                timestamp: now.addingTimeInterval(-70),
                command: "tail -n 200 /var/log/app.log",
                summary: L10n.t("seed.timeline.tail_log.summary"),
                state: .streaming,
                stdoutPreview: "[warn] payment sync retry in 3s...",
                stderrPreview: "",
                exitStatus: 0,
                artifacts: [
                    ArtifactReference(id: "art-0af921", hash: "0af9219be2d1", label: "art-0af921")
                ]
            ),
            CommandTimelineItem(
                id: UUID(),
                targetID: opsID,
                sessionID: "ls-ops-prod",
                channelID: "ch-main",
                timestamp: now.addingTimeInterval(-32),
                command: "rm release.apk",
                summary: L10n.t("seed.timeline.rm_release.summary"),
                state: .waitingApproval,
                stdoutPreview: "",
                stderrPreview: L10n.t("seed.timeline.rm_release.stderr"),
                exitStatus: 0,
                artifacts: [
                    ArtifactReference(id: "art-b7d332", hash: "b7d33219afc0", label: "art-b7d332")
                ]
            ),
            CommandTimelineItem(
                id: UUID(),
                targetID: pixelID,
                sessionID: "ls-pixel-8",
                channelID: "ch-adb",
                timestamp: now.addingTimeInterval(-160),
                command: "adb shell getprop ro.build.fingerprint",
                summary: L10n.t("seed.timeline.adb_fingerprint.summary"),
                state: .success,
                stdoutPreview: "google/pixel/pixel8:15/...",
                stderrPreview: "",
                exitStatus: 0,
                artifacts: [
                    ArtifactReference(id: "art-4d0a44", hash: "4d0a4457237a", label: "art-4d0a44")
                ]
            )
        ]

        let artifacts = [
            ArtifactRecord(
                hash: "8cf4e2a1d7b2",
                contentDigest: "sha256:5de6f9...",
                sourceCommand: "git status",
                summary: L10n.t("seed.artifact.status.summary"),
                sessionID: "ls-ops-prod",
                channelID: "ch-main",
                parentHashes: [],
                derivedHashes: ["0af9219be2d1"],
                textContent: "On branch main\\nnothing to commit, working tree clean"
            ),
            ArtifactRecord(
                hash: "0af9219be2d1",
                contentDigest: "sha256:ba9122...",
                sourceCommand: "tail -n 200 /var/log/app.log",
                summary: L10n.t("seed.artifact.log_slice.summary"),
                sessionID: "ls-ops-prod",
                channelID: "ch-main",
                parentHashes: ["8cf4e2a1d7b2"],
                derivedHashes: ["b7d33219afc0"],
                textContent: "[info] sync started\\n[warn] payment sync retry in 3s\\n[warn] payment sync retry in 6s\\n[info] sync recovered"
            ),
            ArtifactRecord(
                hash: "b7d33219afc0",
                contentDigest: "sha256:9f1ac3...",
                sourceCommand: "rm release.apk",
                summary: L10n.t("seed.artifact.approval_context.summary"),
                sessionID: "ls-ops-prod",
                channelID: "ch-main",
                parentHashes: ["0af9219be2d1"],
                derivedHashes: [],
                textContent: "target: ops-prod\\ncommand: rm release.apk\\npolicy: requires delete approval"
            ),
            ArtifactRecord(
                hash: "4d0a4457237a",
                contentDigest: "sha256:4b06d0...",
                sourceCommand: "adb shell getprop ro.build.fingerprint",
                summary: L10n.t("seed.artifact.adb_fingerprint.summary"),
                sessionID: "ls-pixel-8",
                channelID: "ch-adb",
                parentHashes: [],
                derivedHashes: [],
                textContent: "google/pixel8/pixel8:15/BP1A.240405.002/1234567:user/release-keys"
            )
        ]

        let approvals = [
            ApprovalRequestItem(
                id: UUID(),
                targetID: opsID,
                commandSummary: "rm release.apk",
                reason: L10n.t("seed.approval.delete_on_prod.reason"),
                createdAt: now.addingTimeInterval(-31),
                status: .pending
            )
        ]

        let shellChannels = [
            ShellChannel(
                id: "ch-main",
                targetID: opsID,
                title: L10n.t("workspace.channel.title.interactive_shell"),
                prompt: "ops@ops-prod$",
                isClosed: false,
                lines: [
                    ShellLine(id: UUID(), role: .prompt, content: "ops@ops-prod$ pwd", timestamp: now.addingTimeInterval(-24)),
                    ShellLine(id: UUID(), role: .output, content: "/srv/bridgingio", timestamp: now.addingTimeInterval(-23)),
                    ShellLine(id: UUID(), role: .prompt, content: "ops@ops-prod$ ls", timestamp: now.addingTimeInterval(-18)),
                    ShellLine(id: UUID(), role: .output, content: "bin\\nconfig\\nlogs", timestamp: now.addingTimeInterval(-17))
                ]
            ),
            ShellChannel(
                id: "ch-adb",
                targetID: pixelID,
                title: L10n.t("seed.channel.title.adb_shell"),
                prompt: "oriole:/ $",
                isClosed: false,
                lines: [
                    ShellLine(id: UUID(), role: .prompt, content: "oriole:/ $ getprop ro.product.model", timestamp: now.addingTimeInterval(-80)),
                    ShellLine(id: UUID(), role: .output, content: "Pixel 8", timestamp: now.addingTimeInterval(-79))
                ]
            )
        ]

        return (
            targets: targets,
            timeline: timeline,
            artifacts: artifacts,
            approvals: approvals,
            shellChannels: shellChannels,
            cacheSettings: .default,
            globalToolchains: [
                ToolchainSetting(command: "adb", pathOverride: "/opt/homebrew/bin/adb"),
                ToolchainSetting(command: "ssh", pathOverride: "")
            ]
        )
    }
}
