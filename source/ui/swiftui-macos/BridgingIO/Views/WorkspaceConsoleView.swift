import SwiftUI

struct WorkspaceConsoleView: View {
    @ObservedObject var viewModel: WorkspaceViewModel
    @StateObject private var systemAppearance = SystemAppearanceObserver()

    private var theme: ConsoleTheme {
        ConsoleTheme.resolve(mode: viewModel.appearanceMode, system: systemAppearance.colorScheme)
    }

    var body: some View {
        VStack(spacing: 0) {
            workspaceToolbar
            Divider().overlay(theme.border)

            VSplitView {
                HSplitView {
                    targetSidebar
                        .frame(minWidth: 260, idealWidth: 280, maxWidth: 300)
                    centerPanel
                        .frame(minWidth: 720, maxWidth: .infinity)
                    rightRail
                        .frame(minWidth: 320, idealWidth: 340, maxWidth: 360)
                }

                artifactDrawer
                    .frame(minHeight: viewModel.artifactDrawerExpanded ? 240 : 44)
            }
        }
        .background(theme.canvas)
        .preferredColorScheme(viewModel.appearanceMode.colorSchemeOverride)
        .onChange(of: viewModel.appearanceMode) { _, newMode in
            if newMode == .system {
                systemAppearance.refresh()
            }
        }
        .sheet(item: $viewModel.targetEditorContext) { context in
            TargetProfileSheetView(
                viewModel: TargetEditorViewModel(context: context),
                appearanceMode: viewModel.appearanceMode,
                onCancel: { viewModel.targetEditorContext = nil },
                onSave: { viewModel.saveTarget(draft: $0) }
            )
            .frame(minWidth: 840, minHeight: 680)
        }
        .sheet(isPresented: $viewModel.isShowingSettingsSheet) {
            SettingsSheetView(viewModel: viewModel)
                .frame(minWidth: 560, minHeight: 380)
        }
    }

    private var workspaceToolbar: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(L10n.t("branding.product_name"))
                    .font(ConsoleTypography.heading(17, weight: .bold))
                    .foregroundStyle(theme.textPrimary)
                Text(L10n.f("workspace.toolbar.workspace_format", "local-operator"))
                    .font(ConsoleTypography.body(12))
                    .foregroundStyle(theme.muted)
                Text(viewModel.connectionStatusText)
                    .font(ConsoleTypography.body(11))
                    .foregroundStyle(connectionStatusColor)
                    .accessibilityIdentifier("workspace-connection-status")
            }

            Spacer(minLength: 12)

            ConsoleIconTextInput(
                placeholder: L10n.t("workspace.search.placeholder"),
                text: $viewModel.searchQuery,
                icon: "magnifyingglass",
                theme: theme,
                surface: .elevated
            )
            .frame(width: 320)

            ConsoleSegmentedControl(
                selection: $viewModel.appearanceMode,
                options: AppearanceMode.allCases,
                theme: theme,
                title: { $0.title }
            )
            .frame(width: 170)
            .accessibilityIdentifier("appearance-picker")

            Button {
                viewModel.openCreateTargetSheet()
            } label: {
                Label(L10n.t("workspace.toolbar.new_target"), systemImage: "plus")
            }
            .consoleButton(theme: theme, tone: .primary)
            .accessibilityIdentifier("toolbar-new-target")

            Button {
                viewModel.openChannel()
            } label: {
                Label(L10n.t("workspace.toolbar.open_session"), systemImage: "terminal")
            }
            .consoleButton(theme: theme, tone: .secondary)
            .accessibilityIdentifier("toolbar-open-session")

            Button {
                viewModel.isShowingSettingsSheet = true
            } label: {
                Label(L10n.t("workspace.toolbar.settings"), systemImage: "gearshape")
            }
            .consoleButton(theme: theme, tone: .subtle)
            .accessibilityIdentifier("toolbar-open-settings")
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(theme.panel)
    }

    private var targetSidebar: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(L10n.t("workspace.sidebar.targets"))
                .font(ConsoleTypography.heading(14, weight: .semibold))
                .foregroundStyle(theme.textPrimary)

            ConsoleSegmentedControl(
                selection: $viewModel.targetFilter,
                options: TargetFilter.allCases,
                theme: theme,
                title: { $0.title }
            )
            .accessibilityIdentifier("target-filter-picker")

            ScrollView {
                LazyVStack(spacing: 8) {
                    ForEach(viewModel.filteredTargets) { target in
                        TargetRowView(
                            target: target,
                            theme: theme,
                            isSelected: target.id == viewModel.selectedTargetID,
                            onTap: { viewModel.selectTarget(target) },
                            onEdit: { viewModel.openEditTargetSheet(for: target) }
                        )
                    }
                }
                .padding(.top, 2)
            }

            Button {
                viewModel.openCreateTargetSheet()
            } label: {
                Label(L10n.t("workspace.sidebar.create_target"), systemImage: "plus.circle")
                    .frame(maxWidth: .infinity)
            }
            .consoleButton(theme: theme, tone: .primary)
            .accessibilityIdentifier("sidebar-create-target")

            Button {
                if let target = viewModel.selectedTarget {
                    viewModel.openEditTargetSheet(for: target)
                }
            } label: {
                Label(L10n.t("workspace.sidebar.edit_selected"), systemImage: "square.and.pencil")
                    .frame(maxWidth: .infinity)
            }
            .consoleButton(theme: theme, tone: .secondary)
            .disabled(viewModel.selectedTarget == nil)
            .accessibilityIdentifier("sidebar-edit-target")

            Button {
                viewModel.isShowingSettingsSheet = true
            } label: {
                Label(L10n.t("workspace.sidebar.open_settings"), systemImage: "slider.horizontal.3")
                    .frame(maxWidth: .infinity)
            }
            .consoleButton(theme: theme, tone: .subtle)
        }
        .padding(14)
        .background(theme.panel)
    }

    private var centerPanel: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                VStack(alignment: .leading, spacing: 4) {
                    Text(L10n.t("workspace.center.timeline"))
                        .font(ConsoleTypography.heading(14, weight: .semibold))
                        .foregroundStyle(theme.textPrimary)
                    if let target = viewModel.selectedTarget {
                        Text(L10n.f("workspace.center.session_summary_format", target.sessionSummary.logicalSessionID, target.name, target.sessionSummary.state.title))
                            .font(ConsoleTypography.body(12))
                            .foregroundStyle(theme.muted)
                            .accessibilityIdentifier("center-session-summary")
                    } else {
                        Text(L10n.t("workspace.center.select_target_hint"))
                            .font(ConsoleTypography.body(12))
                            .foregroundStyle(theme.muted)
                    }
                }

                Spacer(minLength: 12)

                ConsoleSegmentedControl(
                    selection: $viewModel.centerPanel,
                    options: WorkspaceCenterPanel.allCases,
                    theme: theme,
                    title: { $0.title }
                )
                .frame(width: 200)
                .accessibilityIdentifier("center-panel-picker")
            }

            Group {
                if viewModel.coreConnectionState == .connected {
                    if viewModel.centerPanel == .timeline {
                        timelineView
                    } else {
                        shellTranscriptView
                    }
                } else {
                    connectionStateView
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .padding(14)
        .background(theme.elevated)
    }

    private var timelineView: some View {
        ScrollView {
            VStack(spacing: 10) {
                if viewModel.selectedTimelineItems.isEmpty {
                    EmptyStateView(
                        title: L10n.t("workspace.empty.timeline.title"),
                        message: L10n.t("workspace.empty.timeline.message"),
                        theme: theme
                    )
                } else {
                    ForEach(viewModel.selectedTimelineItems) { item in
                        TimelineCardView(
                            item: item,
                            theme: theme,
                            onArtifactTap: { viewModel.selectArtifact(hash: $0) }
                        )
                        .accessibilityIdentifier("timeline-card-\(item.id.uuidString)")
                    }
                }
            }
            .padding(.vertical, 2)
        }
        .accessibilityIdentifier("timeline-scroll")
    }

    private var shellTranscriptView: some View {
        VStack(spacing: 10) {
            HStack {
                Text(L10n.t("workspace.transcript.title"))
                    .font(ConsoleTypography.heading(13, weight: .semibold))
                    .foregroundStyle(theme.textPrimary)
                Spacer()
                Button {
                    viewModel.openChannel()
                } label: {
                    Label(L10n.t("workspace.transcript.open_channel"), systemImage: "plus")
                }
                .consoleButton(theme: theme, tone: .secondary)
                .accessibilityIdentifier("transcript-open-channel")
            }

            if let channel = viewModel.selectedChannel {
                ScrollView {
                    VStack(alignment: .leading, spacing: 8) {
                        ForEach(channel.lines) { line in
                            Text(line.content)
                                .font(ConsoleTypography.heading(12, weight: .regular))
                                .foregroundStyle(colorForShellLine(line.role))
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                    }
                    .padding(12)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(theme.panel)
                .overlay {
                    RoundedRectangle(cornerRadius: 10)
                        .stroke(theme.border, lineWidth: 1)
                }
                .clipShape(RoundedRectangle(cornerRadius: 10))

                HStack(spacing: 8) {
                    TextField(L10n.t("workspace.transcript.input_placeholder"), text: $viewModel.shellInput)
                        .consoleInput(theme: theme, surface: .panel)
                        .accessibilityIdentifier("transcript-input")

                    Button(L10n.t("workspace.transcript.send")) {
                        viewModel.sendShellInput()
                    }
                    .consoleButton(theme: theme, tone: .primary)
                    .accessibilityIdentifier("transcript-send")

                    Button(L10n.t("workspace.transcript.interrupt")) {
                        viewModel.interruptChannel()
                    }
                    .consoleButton(theme: theme, tone: .subtle)
                    .accessibilityIdentifier("transcript-interrupt")

                    Button(L10n.t("workspace.transcript.close")) {
                        viewModel.closeChannel()
                    }
                    .consoleButton(theme: theme, tone: .secondary)
                    .accessibilityIdentifier("transcript-close")
                }
            } else {
                EmptyStateView(
                    title: L10n.t("workspace.empty.channel.title"),
                    message: L10n.t("workspace.empty.channel.message"),
                    theme: theme
                )
            }
        }
    }

    private var rightRail: some View {
        ScrollView {
            VStack(spacing: 10) {
                sessionCard
                capabilitiesCard
                approvalQueueCard
                toolSourceCard
                artifactHashLookupCard
                cacheSettingsCard
            }
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .background(theme.panel)
    }

    private var sessionCard: some View {
        ConsoleCard(title: L10n.t("workspace.card.session_state"), theme: theme) {
            if let target = viewModel.selectedTarget {
                VStack(alignment: .leading, spacing: 8) {
                    HStack {
                        Text(target.connectionState.title)
                            .font(ConsoleTypography.heading(13, weight: .semibold))
                        Spacer()
                        StatusBadgeView(
                            text: target.sessionSummary.state.title,
                            tone: toneForSessionState(target.sessionSummary.state),
                            theme: theme
                        )
                    }

                    Text(L10n.f("workspace.session.os_arch_format", target.sessionSummary.fingerprint.osVersion, target.sessionSummary.fingerprint.architecture))
                        .font(ConsoleTypography.body(12))
                        .foregroundStyle(theme.textSecondary)

                    Text(L10n.f("workspace.session.shell_format", target.sessionSummary.fingerprint.shell))
                        .font(ConsoleTypography.body(12))
                        .foregroundStyle(theme.muted)

                    Text(L10n.f("workspace.session.last_active_format", target.lastActivity.formatted(date: .omitted, time: .shortened)))
                        .font(ConsoleTypography.body(12))
                        .foregroundStyle(theme.muted)
                        .accessibilityIdentifier("session-last-active")
                }
            } else {
                EmptyStateInline(theme: theme, message: L10n.t("workspace.empty.select_target"))
            }
        }
    }

    private var capabilitiesCard: some View {
        ConsoleCard(title: L10n.t("workspace.card.capabilities"), theme: theme) {
            if let target = viewModel.selectedTarget {
                FlowLayout(spacing: 8, lineSpacing: 8) {
                    ForEach(target.capabilities) { capability in
                        CapabilityChipView(title: capability.title, theme: theme)
                    }
                }
            } else {
                EmptyStateInline(theme: theme, message: L10n.t("workspace.empty.no_capability_data"))
            }
        }
    }

    private var approvalQueueCard: some View {
        ConsoleCard(title: L10n.t("workspace.card.approval_queue"), theme: theme) {
            if viewModel.selectedApprovals.isEmpty {
                EmptyStateInline(theme: theme, message: L10n.t("workspace.empty.no_pending_approvals"))
            } else {
                VStack(alignment: .leading, spacing: 10) {
                    ForEach(viewModel.selectedApprovals) { approval in
                        VStack(alignment: .leading, spacing: 8) {
                            HStack {
                                Text(approval.commandSummary)
                                    .font(ConsoleTypography.body(13, weight: .medium))
                                    .foregroundStyle(theme.textPrimary)
                                Spacer()
                                StatusBadgeView(
                                    text: approval.status.title,
                                    tone: toneForApprovalStatus(approval.status),
                                    theme: theme
                                )
                            }

                            Text(approval.reason)
                                .font(ConsoleTypography.body(12))
                                .foregroundStyle(theme.textSecondary)

                            if approval.status == .pending {
                                HStack(spacing: 8) {
                                    Button(L10n.t("workspace.approval.approve")) {
                                        viewModel.approve(approval)
                                    }
                                    .consoleButton(theme: theme, tone: .primary)
                                    .accessibilityIdentifier("approval-approve")

                                    Button(L10n.t("workspace.approval.reject")) {
                                        viewModel.reject(approval)
                                    }
                                    .consoleButton(theme: theme, tone: .danger)
                                    .accessibilityIdentifier("approval-reject")
                                }
                            }
                        }
                        .padding(8)
                        .background(theme.elevated)
                        .overlay {
                            RoundedRectangle(cornerRadius: 8)
                                .stroke(theme.border, lineWidth: 1)
                        }
                        .clipShape(RoundedRectangle(cornerRadius: 8))
                    }
                }
            }
        }
    }

    private var toolSourceCard: some View {
        ConsoleCard(title: L10n.t("workspace.card.tool_sources"), theme: theme) {
            if let target = viewModel.selectedTarget {
                VStack(alignment: .leading, spacing: 10) {
                    ForEach(target.toolDiagnostics) { diagnostic in
                        VStack(alignment: .leading, spacing: 6) {
                            HStack {
                                Text(diagnostic.connectorName.uppercased())
                                    .font(ConsoleTypography.heading(12))
                                    .foregroundStyle(theme.textPrimary)
                                Spacer()
                                Text(diagnostic.sourceType.title)
                                    .font(ConsoleTypography.body(11))
                                    .foregroundStyle(theme.muted)
                            }

                            Text(L10n.f("workspace.tool.effective_path_format", diagnostic.effectivePath))
                                .font(ConsoleTypography.body(12))
                                .foregroundStyle(theme.textSecondary)

                            if !diagnostic.overridePath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                                Text("Target override: \(diagnostic.overridePath)")
                                    .font(ConsoleTypography.body(11))
                                    .foregroundStyle(theme.textSecondary)
                            }
                            if !diagnostic.globalOverridePath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                                Text("Global override: \(diagnostic.globalOverridePath)")
                                    .font(ConsoleTypography.body(11))
                                    .foregroundStyle(theme.textSecondary)
                            }
                            if !diagnostic.effectiveScope.isEmpty {
                                Text("Effective scope: \(diagnostic.effectiveScope)")
                                    .font(ConsoleTypography.body(11))
                                    .foregroundStyle(theme.muted)
                            }
                        }
                        .padding(8)
                        .background(theme.elevated)
                        .overlay {
                            RoundedRectangle(cornerRadius: 8)
                                .stroke(theme.border, lineWidth: 1)
                        }
                        .clipShape(RoundedRectangle(cornerRadius: 8))
                    }
                }
            } else {
                EmptyStateInline(theme: theme, message: L10n.t("workspace.empty.no_diagnostics"))
            }
        }
    }

    private var artifactHashLookupCard: some View {
        ConsoleCard(title: L10n.t("workspace.card.artifact_hash_lookup"), theme: theme) {
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    TextField(L10n.t("workspace.artifact.lookup_placeholder"), text: $viewModel.artifactLookupHash)
                        .consoleInput(theme: theme, surface: .panel)
                        .accessibilityIdentifier("artifact-hash-input")

                    Button(L10n.t("workspace.artifact.lookup_action")) {
                        viewModel.lookupArtifactByHash()
                    }
                    .consoleButton(theme: theme, tone: .primary)
                    .accessibilityIdentifier("artifact-hash-lookup")
                }

                if let result = viewModel.artifactLookupResult {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(L10n.f("workspace.artifact.hash_format", result.hash))
                            .accessibilityIdentifier("artifact-hash-result")
                        Text(L10n.f("workspace.artifact.session_channel_format", result.sessionID, result.channelID))
                        Text(L10n.f("workspace.artifact.source_format", result.sourceCommand))
                    }
                    .font(ConsoleTypography.body(11))
                    .foregroundStyle(theme.textSecondary)
                } else {
                    Text(L10n.t("workspace.artifact.no_match"))
                        .font(ConsoleTypography.body(11))
                        .foregroundStyle(theme.muted)
                }
            }
        }
    }

    private var cacheSettingsCard: some View {
        ConsoleCard(title: L10n.t("workspace.card.artifact_cache"), theme: theme) {
            VStack(alignment: .leading, spacing: 10) {
                ConsoleSegmentedControl(
                    selection: Binding(
                        get: { viewModel.cacheSettings.backend },
                        set: { viewModel.updateCacheBackend($0) }
                    ),
                    options: ArtifactCacheBackend.allCases,
                    theme: theme,
                    title: { $0.title }
                )
                .accessibilityIdentifier("cache-backend-picker")

                TextField(L10n.t("workspace.cache.root_path_placeholder"), text: $viewModel.cacheSettings.rootPath)
                    .consoleInput(theme: theme, surface: .panel)

                HStack {
                    Text(L10n.t("workspace.cache.max_cache"))
                        .font(ConsoleTypography.body(12))
                        .foregroundStyle(theme.textSecondary)
                    Spacer()
                    Stepper(
                        value: $viewModel.cacheSettings.maxCacheMB,
                        in: 256...8192,
                        step: 128
                    ) {
                        Text(L10n.f("workspace.cache.max_cache_value_format", viewModel.cacheSettings.maxCacheMB))
                            .font(ConsoleTypography.body(12))
                            .foregroundStyle(theme.textPrimary)
                    }
                }

                ConsoleSegmentedControl(
                    selection: $viewModel.cacheSettings.evictionPolicy,
                    options: ArtifactEvictionPolicy.allCases,
                    theme: theme,
                    title: { $0.title }
                )

                ProgressView(
                    value: Double(viewModel.cacheSettings.usedCacheMB),
                    total: Double(max(viewModel.cacheSettings.maxCacheMB, 1))
                )
                .tint(theme.info)

                Text(L10n.f("workspace.cache.used_value_format", viewModel.cacheSettings.usedCacheMB))
                    .font(ConsoleTypography.body(11))
                    .foregroundStyle(theme.muted)

                if viewModel.cacheSettingsNeedRestart {
                    Text(L10n.t("workspace.cache.restart_required"))
                        .font(ConsoleTypography.body(11))
                        .foregroundStyle(theme.warning)
                        .accessibilityIdentifier("cache-restart-warning")
                }

                HStack(spacing: 8) {
                    Button(L10n.t("workspace.cache.apply")) {
                        viewModel.applyCacheSettings()
                    }
                    .consoleButton(theme: theme, tone: .primary)
                    .accessibilityIdentifier("cache-apply")

                    Button(L10n.t("workspace.cache.clear")) {
                        viewModel.clearArtifactCache()
                    }
                    .consoleButton(theme: theme, tone: .secondary)
                    .accessibilityIdentifier("cache-clear")
                }
            }
        }
    }

    private var artifactDrawer: some View {
        ArtifactDrawerView(viewModel: viewModel, theme: theme)
    }

    private func toneForSessionState(_ state: SessionState) -> BadgeTone {
        switch state {
        case .active:
            return .success
        case .waiting:
            return .info
        case .degraded:
            return .warning
        case .closed:
            return .danger
        }
    }

    private func toneForApprovalStatus(_ status: ApprovalStatus) -> BadgeTone {
        switch status {
        case .pending:
            return .warning
        case .approved:
            return .success
        case .rejected, .failed:
            return .danger
        }
    }

    private func colorForShellLine(_ role: ShellLineRole) -> Color {
        switch role {
        case .prompt:
            return theme.info
        case .input:
            return theme.textPrimary
        case .output:
            return theme.textSecondary
        case .info:
            return theme.muted
        }
    }

    private var connectionStatusColor: Color {
        switch viewModel.coreConnectionState {
        case .connected:
            return theme.success
        case .startingCore, .attachingUI, .savingChanges, .restartRequired, .restartingCore:
            return theme.warning
        case .needsRuntimeRoot, .runtimeRootUnavailable, .attachFailed, .restartFailed:
            return theme.danger
        }
    }

    private var connectionStateView: some View {
        VStack(spacing: 14) {
            EmptyStateView(
                title: connectionStateTitle,
                message: connectionStateMessage,
                theme: theme
            )
            if connectionNeedsRuntimeRootSelection {
                Button("Select Runtime Root") {
                    viewModel.chooseRuntimeRoot()
                }
                .consoleButton(theme: theme, tone: .primary)
                .accessibilityIdentifier("select-runtime-root")
            }
            if case .restartFailed = viewModel.coreConnectionState {
                Button("Retry Restart") {
                    viewModel.retryManagedConnection()
                }
                .consoleButton(theme: theme, tone: .secondary)
            }
        }
    }

    private var connectionStateTitle: String {
        switch viewModel.coreConnectionState {
        case .needsRuntimeRoot:
            return "Runtime root required"
        case .runtimeRootUnavailable:
            return "Runtime root unavailable"
        case .startingCore:
            return L10n.t("workspace.connection.starting_title")
        case .attachingUI:
            return L10n.t("workspace.connection.waiting_title")
        case .savingChanges:
            return "Saving changes"
        case .restartRequired:
            return "Restart required"
        case .restartingCore:
            return "Restarting core"
        case .restartFailed:
            return "Core restart failed"
        case .attachFailed:
            return L10n.t("workspace.connection.failed_title")
        case .connected:
            return ""
        }
    }

    private var connectionStateMessage: String {
        switch viewModel.coreConnectionState {
        case .needsRuntimeRoot:
            return "Choose a runtime root directory before starting bundled managed core."
        case .runtimeRootUnavailable(let path):
            return "Saved runtime root is unavailable: \(path)"
        case .startingCore:
            return L10n.t("workspace.connection.starting_message")
        case .attachingUI:
            return L10n.t("workspace.connection.waiting_message")
        case .savingChanges:
            return "Submitting changes to core-owned settings."
        case .restartRequired:
            return "The latest change requires a managed core restart."
        case .restartingCore:
            return "Restarting core and re-attaching UI..."
        case .restartFailed(let message):
            return message
        case .attachFailed(let message):
            return L10n.f("workspace.connection.failed_format", message)
        case .connected:
            return ""
        }
    }

    private var connectionNeedsRuntimeRootSelection: Bool {
        switch viewModel.coreConnectionState {
        case .needsRuntimeRoot, .runtimeRootUnavailable:
            return true
        default:
            return false
        }
    }
}

private struct TargetRowView: View {
    let target: TargetProfile
    let theme: ConsoleTheme
    let isSelected: Bool
    let onTap: () -> Void
    let onEdit: () -> Void

    var body: some View {
        Button(action: onTap) {
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Label(target.name, systemImage: target.kind.symbolName)
                        .font(ConsoleTypography.heading(13, weight: .medium))
                        .foregroundStyle(theme.textPrimary)
                        .lineLimit(1)
                    Spacer()
                    Text(target.kind.title)
                        .font(ConsoleTypography.body(11))
                        .foregroundStyle(theme.muted)
                        .lineLimit(1)
                }

                HStack {
                    Circle()
                        .fill(stateColor)
                        .frame(width: 8, height: 8)
                    Text(target.connectionState.title)
                        .font(ConsoleTypography.body(11))
                        .foregroundStyle(theme.textSecondary)
                    Spacer()
                    Text(L10n.f("workspace.target.alias_format", target.aliasForModel))
                        .font(ConsoleTypography.body(11))
                        .foregroundStyle(theme.muted)
                }

                Text(target.lastActivity.formatted(date: .omitted, time: .shortened))
                    .font(ConsoleTypography.body(10))
                    .foregroundStyle(theme.muted)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .padding(10)
            .background(isSelected ? theme.elevated : theme.panel)
            .overlay {
                RoundedRectangle(cornerRadius: 10)
                    .stroke(isSelected ? theme.info : theme.border, lineWidth: isSelected ? 1.5 : 1)
            }
            .clipShape(RoundedRectangle(cornerRadius: 10))
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier(target.accessibilityID)
        .contextMenu {
            Button(L10n.t("workspace.target.context.edit_target")) {
                onEdit()
            }
        }
    }

    private var stateColor: Color {
        switch target.connectionState {
        case .connected:
            return theme.success
        case .idle:
            return theme.info
        case .degraded:
            return theme.warning
        case .disconnected:
            return theme.danger
        }
    }
}

private struct TimelineCardView: View {
    let item: CommandTimelineItem
    let theme: ConsoleTheme
    let onArtifactTap: (String) -> Void

    @State private var isExpanded = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text(item.timestamp.formatted(date: .omitted, time: .standard))
                    .font(ConsoleTypography.body(11))
                    .foregroundStyle(theme.muted)

                Text(item.command)
                    .font(ConsoleTypography.heading(13, weight: .medium))
                    .foregroundStyle(theme.textPrimary)
                    .lineLimit(1)

                Spacer(minLength: 8)

                StatusBadgeView(text: item.state.title, tone: tone, theme: theme)
            }

            Text(item.summary)
                .font(ConsoleTypography.body(12))
                .foregroundStyle(theme.textSecondary)
                .lineLimit(isExpanded ? nil : 2)

            if isExpanded {
                VStack(alignment: .leading, spacing: 6) {
                    if !item.stdoutPreview.isEmpty {
                        Text(L10n.t("workspace.timeline.stdout"))
                            .font(ConsoleTypography.heading(11))
                            .foregroundStyle(theme.muted)
                        Text(item.stdoutPreview)
                            .font(ConsoleTypography.heading(11, weight: .regular))
                            .foregroundStyle(theme.textSecondary)
                    }

                    if !item.stderrPreview.isEmpty {
                        Text(L10n.t("workspace.timeline.stderr"))
                            .font(ConsoleTypography.heading(11))
                            .foregroundStyle(theme.warning)
                        Text(item.stderrPreview)
                            .font(ConsoleTypography.heading(11, weight: .regular))
                            .foregroundStyle(theme.warning)
                    }
                }
            }

            HStack(spacing: 8) {
                Text(L10n.f("workspace.timeline.exit_format", item.exitStatus))
                    .font(ConsoleTypography.body(11))
                    .foregroundStyle(theme.muted)

                ForEach(item.artifacts) { artifact in
                    Button(artifact.label) {
                        onArtifactTap(artifact.hash)
                    }
                    .buttonStyle(.link)
                }

                Spacer()

                Button(isExpanded ? L10n.t("workspace.action.collapse") : L10n.t("workspace.action.expand")) {
                    withAnimation(.easeInOut(duration: 0.18)) {
                        isExpanded.toggle()
                    }
                }
                .buttonStyle(.link)
            }
        }
        .padding(12)
        .background(theme.panel)
        .overlay {
            RoundedRectangle(cornerRadius: 10)
                .stroke(theme.border, lineWidth: 1)
        }
        .clipShape(RoundedRectangle(cornerRadius: 10))
    }

    private var tone: BadgeTone {
        switch item.state {
        case .success:
            return .success
        case .streaming:
            return .info
        case .waitingApproval:
            return .warning
        case .failed:
            return .danger
        }
    }
}

private struct ArtifactDrawerView: View {
    @ObservedObject var viewModel: WorkspaceViewModel
    let theme: ConsoleTheme

    var body: some View {
        VStack(spacing: 0) {
                HStack {
                Text(L10n.t("workspace.drawer.title"))
                    .font(ConsoleTypography.heading(13, weight: .semibold))
                    .foregroundStyle(theme.textPrimary)

                Spacer()

                if let artifact = viewModel.selectedArtifact {
                    Text(artifact.hash)
                        .font(ConsoleTypography.heading(11, weight: .regular))
                        .foregroundStyle(theme.info)
                }

                Button(viewModel.artifactDrawerExpanded ? L10n.t("workspace.action.collapse") : L10n.t("workspace.action.expand")) {
                    withAnimation(.easeInOut(duration: 0.18)) {
                        viewModel.artifactDrawerExpanded.toggle()
                    }
                }
                .consoleButton(theme: theme, tone: .subtle)
                .accessibilityIdentifier("artifact-drawer-toggle")
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 9)
            .background(theme.panel)

            if viewModel.artifactDrawerExpanded {
                Divider().overlay(theme.border)

                HStack(alignment: .top, spacing: 14) {
                    VStack(alignment: .leading, spacing: 8) {
                        Text(L10n.t("workspace.drawer.preview"))
                            .font(ConsoleTypography.heading(12, weight: .semibold))
                            .foregroundStyle(theme.textPrimary)

                        ScrollView {
                            Text(viewModel.refinedArtifactText.isEmpty ? L10n.t("workspace.drawer.preview_empty") : viewModel.refinedArtifactText)
                                .font(ConsoleTypography.heading(11, weight: .regular))
                                .foregroundStyle(theme.textSecondary)
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        .frame(maxHeight: .infinity)
                        .padding(10)
                        .background(theme.elevated)
                        .overlay {
                            RoundedRectangle(cornerRadius: 8)
                                .stroke(theme.border, lineWidth: 1)
                        }
                        .clipShape(RoundedRectangle(cornerRadius: 8))
                        .accessibilityIdentifier("artifact-preview")
                    }

                    VStack(alignment: .leading, spacing: 10) {
                        Text(L10n.t("workspace.drawer.refine"))
                            .font(ConsoleTypography.heading(12, weight: .semibold))
                            .foregroundStyle(theme.textPrimary)

                        TextField(L10n.t("workspace.drawer.keyword_placeholder"), text: $viewModel.refineKeyword)
                            .consoleInput(theme: theme, surface: .elevated)

                        HStack {
                            Stepper(L10n.f("workspace.drawer.range_from_format", viewModel.refineRangeStart), value: $viewModel.refineRangeStart, in: 1...9999)
                            Stepper(L10n.f("workspace.drawer.range_to_format", viewModel.refineRangeEnd), value: $viewModel.refineRangeEnd, in: 1...9999)
                        }
                        .font(ConsoleTypography.body(11))
                        .foregroundStyle(theme.textSecondary)

                        if let artifact = viewModel.selectedArtifact {
                            VStack(alignment: .leading, spacing: 4) {
                                Text(L10n.f("workspace.drawer.source_format", artifact.sourceCommand))
                                Text(L10n.f("workspace.drawer.session_channel_format", artifact.sessionID, artifact.channelID))
                                if !artifact.parentHashes.isEmpty {
                                    Text(L10n.f("workspace.drawer.parents_format", artifact.parentHashes.joined(separator: ", ")))
                                }
                                if !artifact.derivedHashes.isEmpty {
                                    Text(L10n.f("workspace.drawer.derived_format", artifact.derivedHashes.joined(separator: ", ")))
                                }
                            }
                            .font(ConsoleTypography.body(11))
                            .foregroundStyle(theme.muted)
                        }
                    }
                    .frame(width: 330)
                }
                .padding(14)
                .background(theme.panel)
            }
        }
        .overlay(alignment: .top) {
            Rectangle()
                .fill(theme.border)
                .frame(height: 1)
        }
    }
}

private struct ConsoleCard<Content: View>: View {
    let title: String
    let theme: ConsoleTheme
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title)
                .font(ConsoleTypography.heading(13, weight: .semibold))
                .foregroundStyle(theme.textPrimary)
            content
        }
        .padding(12)
        .background(theme.elevated)
        .overlay {
            RoundedRectangle(cornerRadius: 10)
                .stroke(theme.border, lineWidth: 1)
        }
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct CapabilityChipView: View {
    let title: String
    let theme: ConsoleTheme

    var body: some View {
        Text(title)
            .font(ConsoleTypography.heading(11, weight: .regular))
            .foregroundStyle(theme.textPrimary)
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(theme.panel)
            .overlay {
                Capsule()
                    .stroke(theme.border, lineWidth: 1)
            }
            .clipShape(Capsule())
    }
}

private struct EmptyStateView: View {
    let title: String
    let message: String
    let theme: ConsoleTheme

    var body: some View {
        VStack(spacing: 8) {
            Text(title)
                .font(ConsoleTypography.heading(13, weight: .semibold))
                .foregroundStyle(theme.textPrimary)
            Text(message)
                .font(ConsoleTypography.body(12))
                .foregroundStyle(theme.muted)
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity, minHeight: 160)
        .padding(16)
        .background(theme.panel)
        .overlay {
            RoundedRectangle(cornerRadius: 10)
                .stroke(theme.border, lineWidth: 1)
        }
        .clipShape(RoundedRectangle(cornerRadius: 10))
    }
}

private struct EmptyStateInline: View {
    let theme: ConsoleTheme
    let message: String

    var body: some View {
        Text(message)
            .font(ConsoleTypography.body(12))
            .foregroundStyle(theme.muted)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

enum BadgeTone {
    case success
    case info
    case warning
    case danger
}

private struct StatusBadgeView: View {
    let text: String
    let tone: BadgeTone
    let theme: ConsoleTheme

    var body: some View {
        Text(text)
            .font(ConsoleTypography.body(10, weight: .semibold))
            .foregroundStyle(foregroundColor)
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(backgroundColor)
            .overlay {
                Capsule()
                    .stroke(borderColor, lineWidth: 1)
            }
            .clipShape(Capsule())
    }

    private var foregroundColor: Color {
        switch tone {
        case .success:
            return theme.success
        case .info:
            return theme.info
        case .warning:
            return theme.warning
        case .danger:
            return theme.danger
        }
    }

    private var borderColor: Color {
        foregroundColor.opacity(0.75)
    }

    private var backgroundColor: Color {
        foregroundColor.opacity(0.12)
    }
}

private struct FlowLayout<Content: View>: View {
    let spacing: CGFloat
    let lineSpacing: CGFloat
    @ViewBuilder var content: Content

    init(spacing: CGFloat = 8, lineSpacing: CGFloat = 8, @ViewBuilder content: () -> Content) {
        self.spacing = spacing
        self.lineSpacing = lineSpacing
        self.content = content()
    }

    var body: some View {
        if #available(macOS 13.0, *) {
            AnyLayout(FlowLayoutImpl(spacing: spacing, lineSpacing: lineSpacing)) {
                content
            }
        } else {
            VStack(alignment: .leading, spacing: lineSpacing) {
                content
            }
        }
    }
}

@available(macOS 13.0, *)
private struct FlowLayoutImpl: Layout {
    let spacing: CGFloat
    let lineSpacing: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let maxWidth = proposal.width ?? .greatestFiniteMagnitude
        var currentX: CGFloat = 0
        var currentY: CGFloat = 0
        var lineHeight: CGFloat = 0

        for view in subviews {
            let size = view.sizeThatFits(.unspecified)
            if currentX + size.width > maxWidth {
                currentX = 0
                currentY += lineHeight + lineSpacing
                lineHeight = 0
            }
            currentX += size.width + spacing
            lineHeight = max(lineHeight, size.height)
        }

        return CGSize(width: maxWidth, height: currentY + lineHeight)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var x = bounds.minX
        var y = bounds.minY
        var lineHeight: CGFloat = 0

        for view in subviews {
            let size = view.sizeThatFits(.unspecified)
            if x + size.width > bounds.maxX {
                x = bounds.minX
                y += lineHeight + lineSpacing
                lineHeight = 0
            }

            view.place(
                at: CGPoint(x: x, y: y),
                proposal: ProposedViewSize(width: size.width, height: size.height)
            )
            x += size.width + spacing
            lineHeight = max(lineHeight, size.height)
        }
    }
}
