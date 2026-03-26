import Foundation
import SwiftUI
import Combine

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
    @Published var targetEditorContext: TargetEditorContext?
    @Published var isShowingSettingsSheet: Bool = false

    @Published private(set) var targets: [TargetProfile]
    @Published private(set) var timeline: [CommandTimelineItem]
    @Published private(set) var artifacts: [ArtifactRecord]
    @Published private(set) var approvals: [ApprovalRequestItem]
    @Published private(set) var shellChannels: [ShellChannel]

    private var appliedCacheBackend: ArtifactCacheBackend

    init() {
        let seed = WorkspaceViewModel.makeSeed()
        targets = seed.targets
        timeline = seed.timeline
        artifacts = seed.artifacts
        approvals = seed.approvals
        shellChannels = seed.shellChannels
        cacheSettings = seed.cacheSettings
        appliedCacheBackend = seed.cacheSettings.backend
        selectedTargetID = seed.targets.first?.id
        if let firstHash = seed.artifacts.first?.hash {
            selectedArtifactHash = firstHash
            artifactLookupHash = firstHash
            artifactLookupResult = seed.artifacts.first
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
        targetEditorContext = TargetEditorContext(mode: .create, draft: TargetProfileDraft())
    }

    func openEditTargetSheet(for target: TargetProfile) {
        targetEditorContext = TargetEditorContext(mode: .edit, draft: TargetProfileDraft(profile: target))
    }

    func saveTarget(draft: TargetProfileDraft) {
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
        guard let selectedTargetID,
              let targetIndex = targets.firstIndex(where: { $0.id == selectedTargetID }),
              let diagIndex = targets[targetIndex].toolDiagnostics.firstIndex(where: { $0.id == connectorID }) else {
            return
        }

        let trimmed = newPath.trimmingCharacters(in: .whitespacesAndNewlines)
        targets[targetIndex].toolDiagnostics[diagIndex].overridePath = trimmed
        if trimmed.isEmpty {
            targets[targetIndex].toolDiagnostics[diagIndex].sourceType = .systemPath
        } else {
            targets[targetIndex].toolDiagnostics[diagIndex].sourceType = .userOverride
            targets[targetIndex].toolDiagnostics[diagIndex].effectivePath = trimmed
        }
        targets[targetIndex].toolDiagnostics[diagIndex].lastChecked = .now
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
        guard let index = approvals.firstIndex(where: { $0.id == request.id }) else { return }
        approvals[index].status = .approved

        if let timelineIndex = timeline.firstIndex(where: {
            $0.targetID == request.targetID && $0.command.contains(request.commandSummary)
        }) {
            timeline[timelineIndex].state = .success
            timeline[timelineIndex].summary = "Operator approved, command completed successfully."
            timeline[timelineIndex].exitStatus = 0
        }
    }

    func reject(_ request: ApprovalRequestItem) {
        guard let index = approvals.firstIndex(where: { $0.id == request.id }) else { return }
        approvals[index].status = .rejected

        if let timelineIndex = timeline.firstIndex(where: {
            $0.targetID == request.targetID && $0.command.contains(request.commandSummary)
        }) {
            timeline[timelineIndex].state = .failed
            timeline[timelineIndex].summary = "Operator rejected request, execution was blocked."
            timeline[timelineIndex].exitStatus = 126
        }
    }

    func openChannel() {
        guard let selectedTargetID else { return }
        let newChannel = ShellChannel(
            id: "ch-\(Int.random(in: 100...999))",
            targetID: selectedTargetID,
            title: "Interactive shell",
            prompt: "ops@host$",
            isClosed: false,
            lines: [
                ShellLine(id: UUID(), role: .info, content: "Channel opened and attached to logical session.", timestamp: .now)
            ]
        )
        shellChannels.insert(newChannel, at: 0)
        centerPanel = .transcript
    }

    func sendShellInput() {
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
                content: "Executed \"\(input)\" on \(targets.first(where: { $0.id == targetID })?.name ?? "target")",
                timestamp: .now
            )
        )
        shellInput = ""
    }

    func interruptChannel() {
        guard let targetID = selectedTargetID,
              let index = shellChannels.firstIndex(where: { $0.targetID == targetID }),
              !shellChannels[index].isClosed else {
            return
        }

        shellChannels[index].lines.append(
            ShellLine(id: UUID(), role: .info, content: "SIGINT sent to active process.", timestamp: .now)
        )
    }

    func closeChannel() {
        guard let targetID = selectedTargetID,
              let index = shellChannels.firstIndex(where: { $0.targetID == targetID }),
              !shellChannels[index].isClosed else {
            return
        }

        shellChannels[index].isClosed = true
        shellChannels[index].lines.append(
            ShellLine(id: UUID(), role: .info, content: "Channel closed by operator.", timestamp: .now)
        )
    }

    func updateCacheBackend(_ backend: ArtifactCacheBackend) {
        cacheSettings.backend = backend
        cacheSettingsNeedRestart = cacheSettings.backend != appliedCacheBackend
    }

    func applyCacheSettings() {
        appliedCacheBackend = cacheSettings.backend
        cacheSettingsNeedRestart = false
    }

    func clearArtifactCache() {
        cacheSettings.usedCacheMB = 0
    }

    private func buildProfile(from draft: TargetProfileDraft, id: UUID, createdAt: Date) -> TargetProfile {
        TargetProfile(
            id: id,
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
                CapabilitySummary(id: "terminal", title: "terminal"),
                CapabilitySummary(id: "artifacts", title: "artifacts"),
                CapabilitySummary(id: "approvals", title: "approvals")
            ],
            toolDiagnostics: draft.toolDiagnostics
        )
    }

    private static func makeSeed() -> (
        targets: [TargetProfile],
        timeline: [CommandTimelineItem],
        artifacts: [ArtifactRecord],
        approvals: [ApprovalRequestItem],
        shellChannels: [ShellChannel],
        cacheSettings: ArtifactCacheSettings
    ) {
        let now = Date()
        let opsID = UUID(uuidString: "4F5D6A8A-53AB-4F47-9A3E-4EA49FE4E1B1") ?? UUID()
        let pixelID = UUID(uuidString: "C2CA0E7F-8D16-47CB-B2E9-42DBAB53E874") ?? UUID()
        let labID = UUID(uuidString: "BA6F0462-797A-4CE6-872F-37E029A7CC7C") ?? UUID()

        let targets = [
            TargetProfile(
                id: opsID,
                name: "ops-prod",
                kind: .ssh,
                aliasForModel: "prod",
                notes: "Production bastion host",
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
                    CapabilitySummary(id: "terminal", title: "terminal"),
                    CapabilitySummary(id: "git", title: "git"),
                    CapabilitySummary(id: "artifacts", title: "artifacts"),
                    CapabilitySummary(id: "approvals", title: "approvals")
                ],
                toolDiagnostics: [
                    ToolSourceDiagnostic(
                        id: "ssh",
                        connectorName: "ssh",
                        sourceType: .systemPath,
                        effectivePath: "/usr/bin/ssh",
                        overridePath: "",
                        lastChecked: now.addingTimeInterval(-30)
                    ),
                    ToolSourceDiagnostic(
                        id: "adb",
                        connectorName: "adb",
                        sourceType: .bundledFallback,
                        effectivePath: "BridgingIO bundle",
                        overridePath: "",
                        lastChecked: now.addingTimeInterval(-30)
                    )
                ]
            ),
            TargetProfile(
                id: pixelID,
                name: "pixel-8",
                kind: .adb,
                aliasForModel: "android-main",
                notes: "QA device",
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
                    CapabilitySummary(id: "terminal", title: "terminal"),
                    CapabilitySummary(id: "artifacts", title: "artifacts")
                ],
                toolDiagnostics: [
                    ToolSourceDiagnostic(
                        id: "adb",
                        connectorName: "adb",
                        sourceType: .bundledFallback,
                        effectivePath: "BridgingIO bundle",
                        overridePath: "",
                        lastChecked: now.addingTimeInterval(-90)
                    )
                ]
            ),
            TargetProfile(
                id: labID,
                name: "lab-host",
                kind: .ssh,
                aliasForModel: "lab",
                notes: "Staging runner host",
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
                    CapabilitySummary(id: "terminal", title: "terminal"),
                    CapabilitySummary(id: "artifacts", title: "artifacts")
                ],
                toolDiagnostics: [
                    ToolSourceDiagnostic(
                        id: "ssh",
                        connectorName: "ssh",
                        sourceType: .systemPath,
                        effectivePath: "/usr/bin/ssh",
                        overridePath: "",
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
                summary: "Repository clean with one local change in docs/.",
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
                summary: "Streaming logs with warnings around payment sync.",
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
                summary: "Waiting operator approval for delete operation.",
                state: .waitingApproval,
                stdoutPreview: "",
                stderrPreview: "Operation classified as delete + production target.",
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
                summary: "Collected environment fingerprint for Android device.",
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
                summary: "Status output snapshot",
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
                summary: "Log slice with warnings",
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
                summary: "Approval context for delete command",
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
                summary: "ADB fingerprint output",
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
                reason: "Delete on production target requires operator confirmation.",
                createdAt: now.addingTimeInterval(-31),
                status: .pending
            )
        ]

        let shellChannels = [
            ShellChannel(
                id: "ch-main",
                targetID: opsID,
                title: "Interactive shell",
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
                title: "ADB shell",
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
            cacheSettings: .default
        )
    }
}
