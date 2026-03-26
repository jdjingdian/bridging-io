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
                Text("BridgingIO")
                    .font(ConsoleTypography.heading(17, weight: .bold))
                    .foregroundStyle(theme.textPrimary)
                Text("Workspace: local-operator")
                    .font(ConsoleTypography.body(12))
                    .foregroundStyle(theme.muted)
            }

            Spacer(minLength: 12)

            ConsoleIconTextInput(
                placeholder: "Search targets / commands",
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
                Label("New Target", systemImage: "plus")
            }
            .consoleButton(theme: theme, tone: .primary)
            .accessibilityIdentifier("toolbar-new-target")

            Button {
                viewModel.openChannel()
            } label: {
                Label("Open Session", systemImage: "terminal")
            }
            .consoleButton(theme: theme, tone: .secondary)
            .accessibilityIdentifier("toolbar-open-session")

            Button {
                viewModel.isShowingSettingsSheet = true
            } label: {
                Label("Settings", systemImage: "gearshape")
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
            Text("Targets")
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
                Label("Create Target", systemImage: "plus.circle")
                    .frame(maxWidth: .infinity)
            }
            .consoleButton(theme: theme, tone: .primary)
            .accessibilityIdentifier("sidebar-create-target")

            Button {
                if let target = viewModel.selectedTarget {
                    viewModel.openEditTargetSheet(for: target)
                }
            } label: {
                Label("Edit Selected", systemImage: "square.and.pencil")
                    .frame(maxWidth: .infinity)
            }
            .consoleButton(theme: theme, tone: .secondary)
            .disabled(viewModel.selectedTarget == nil)
            .accessibilityIdentifier("sidebar-edit-target")

            Button {
                viewModel.isShowingSettingsSheet = true
            } label: {
                Label("Open Settings", systemImage: "slider.horizontal.3")
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
                    Text("Timeline")
                        .font(ConsoleTypography.heading(14, weight: .semibold))
                        .foregroundStyle(theme.textPrimary)
                    if let target = viewModel.selectedTarget {
                        Text("Session: \(target.sessionSummary.logicalSessionID) • \(target.name) • \(target.sessionSummary.state.title)")
                            .font(ConsoleTypography.body(12))
                            .foregroundStyle(theme.muted)
                            .accessibilityIdentifier("center-session-summary")
                    } else {
                        Text("Select a target to inspect its logical session.")
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
                if viewModel.centerPanel == .timeline {
                    timelineView
                } else {
                    shellTranscriptView
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
                        title: "No Timeline Events",
                        message: "Select a target to inspect command cards and linked artifacts.",
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
                Text("Transcript")
                    .font(ConsoleTypography.heading(13, weight: .semibold))
                    .foregroundStyle(theme.textPrimary)
                Spacer()
                Button {
                    viewModel.openChannel()
                } label: {
                    Label("Open Channel", systemImage: "plus")
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
                    TextField("Send input to channel", text: $viewModel.shellInput)
                        .consoleInput(theme: theme, surface: .panel)
                        .accessibilityIdentifier("transcript-input")

                    Button("Send") {
                        viewModel.sendShellInput()
                    }
                    .consoleButton(theme: theme, tone: .primary)
                    .accessibilityIdentifier("transcript-send")

                    Button("Interrupt") {
                        viewModel.interruptChannel()
                    }
                    .consoleButton(theme: theme, tone: .subtle)
                    .accessibilityIdentifier("transcript-interrupt")

                    Button("Close") {
                        viewModel.closeChannel()
                    }
                    .consoleButton(theme: theme, tone: .secondary)
                    .accessibilityIdentifier("transcript-close")
                }
            } else {
                EmptyStateView(
                    title: "No Channel",
                    message: "Open a shell channel to review prompt/output and send input.",
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
        ConsoleCard(title: "Session State", theme: theme) {
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

                    Text("\(target.sessionSummary.fingerprint.osVersion) • \(target.sessionSummary.fingerprint.architecture)")
                        .font(ConsoleTypography.body(12))
                        .foregroundStyle(theme.textSecondary)

                    Text("shell: \(target.sessionSummary.fingerprint.shell)")
                        .font(ConsoleTypography.body(12))
                        .foregroundStyle(theme.muted)

                    Text("last active: \(target.lastActivity.formatted(date: .omitted, time: .shortened))")
                        .font(ConsoleTypography.body(12))
                        .foregroundStyle(theme.muted)
                        .accessibilityIdentifier("session-last-active")
                }
            } else {
                EmptyStateInline(theme: theme, message: "Select target")
            }
        }
    }

    private var capabilitiesCard: some View {
        ConsoleCard(title: "Capabilities", theme: theme) {
            if let target = viewModel.selectedTarget {
                FlowLayout(spacing: 8, lineSpacing: 8) {
                    ForEach(target.capabilities) { capability in
                        CapabilityChipView(title: capability.title, theme: theme)
                    }
                }
            } else {
                EmptyStateInline(theme: theme, message: "No capability data")
            }
        }
    }

    private var approvalQueueCard: some View {
        ConsoleCard(title: "Approval Queue", theme: theme) {
            if viewModel.selectedApprovals.isEmpty {
                EmptyStateInline(theme: theme, message: "No pending approvals")
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
                                    Button("Approve") {
                                        viewModel.approve(approval)
                                    }
                                    .consoleButton(theme: theme, tone: .primary)
                                    .accessibilityIdentifier("approval-approve")

                                    Button("Reject") {
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
        ConsoleCard(title: "Tool Sources", theme: theme) {
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

                            Text("effective: \(diagnostic.effectivePath)")
                                .font(ConsoleTypography.body(12))
                                .foregroundStyle(theme.textSecondary)

                            TextField(
                                "Override path",
                                text: Binding(
                                    get: { diagnostic.overridePath },
                                    set: { viewModel.updateToolOverride(connectorID: diagnostic.id, newPath: $0) }
                                )
                            )
                            .consoleInput(theme: theme, surface: .panel)
                            .accessibilityIdentifier("tool-override-\(diagnostic.id)")
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
                EmptyStateInline(theme: theme, message: "No diagnostics")
            }
        }
    }

    private var artifactHashLookupCard: some View {
        ConsoleCard(title: "Artifact Hash Lookup", theme: theme) {
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    TextField("artifact hash prefix", text: $viewModel.artifactLookupHash)
                        .consoleInput(theme: theme, surface: .panel)
                        .accessibilityIdentifier("artifact-hash-input")

                    Button("Lookup") {
                        viewModel.lookupArtifactByHash()
                    }
                    .consoleButton(theme: theme, tone: .primary)
                    .accessibilityIdentifier("artifact-hash-lookup")
                }

                if let result = viewModel.artifactLookupResult {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("hash: \(result.hash)")
                            .accessibilityIdentifier("artifact-hash-result")
                        Text("session: \(result.sessionID) • channel: \(result.channelID)")
                        Text("source: \(result.sourceCommand)")
                    }
                    .font(ConsoleTypography.body(11))
                    .foregroundStyle(theme.textSecondary)
                } else {
                    Text("No match")
                        .font(ConsoleTypography.body(11))
                        .foregroundStyle(theme.muted)
                }
            }
        }
    }

    private var cacheSettingsCard: some View {
        ConsoleCard(title: "Artifact Cache", theme: theme) {
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

                TextField("Root path", text: $viewModel.cacheSettings.rootPath)
                    .consoleInput(theme: theme, surface: .panel)

                HStack {
                    Text("Max cache")
                        .font(ConsoleTypography.body(12))
                        .foregroundStyle(theme.textSecondary)
                    Spacer()
                    Stepper(
                        value: $viewModel.cacheSettings.maxCacheMB,
                        in: 256...8192,
                        step: 128
                    ) {
                        Text("\(viewModel.cacheSettings.maxCacheMB) MB")
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

                Text("Used: \(viewModel.cacheSettings.usedCacheMB) MB")
                    .font(ConsoleTypography.body(11))
                    .foregroundStyle(theme.muted)

                if viewModel.cacheSettingsNeedRestart {
                    Text("Restart core required to apply backend switch.")
                        .font(ConsoleTypography.body(11))
                        .foregroundStyle(theme.warning)
                        .accessibilityIdentifier("cache-restart-warning")
                }

                HStack(spacing: 8) {
                    Button("Apply") {
                        viewModel.applyCacheSettings()
                    }
                    .consoleButton(theme: theme, tone: .primary)
                    .accessibilityIdentifier("cache-apply")

                    Button("Clear") {
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
                    Text("alias: \(target.aliasForModel)")
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
            Button("Edit Target") {
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
                        Text("stdout")
                            .font(ConsoleTypography.heading(11))
                            .foregroundStyle(theme.muted)
                        Text(item.stdoutPreview)
                            .font(ConsoleTypography.heading(11, weight: .regular))
                            .foregroundStyle(theme.textSecondary)
                    }

                    if !item.stderrPreview.isEmpty {
                        Text("stderr")
                            .font(ConsoleTypography.heading(11))
                            .foregroundStyle(theme.warning)
                        Text(item.stderrPreview)
                            .font(ConsoleTypography.heading(11, weight: .regular))
                            .foregroundStyle(theme.warning)
                    }
                }
            }

            HStack(spacing: 8) {
                Text("exit: \(item.exitStatus)")
                    .font(ConsoleTypography.body(11))
                    .foregroundStyle(theme.muted)

                ForEach(item.artifacts) { artifact in
                    Button(artifact.label) {
                        onArtifactTap(artifact.hash)
                    }
                    .buttonStyle(.link)
                }

                Spacer()

                Button(isExpanded ? "Collapse" : "Expand") {
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
                Text("Artifact Drawer")
                    .font(ConsoleTypography.heading(13, weight: .semibold))
                    .foregroundStyle(theme.textPrimary)

                Spacer()

                if let artifact = viewModel.selectedArtifact {
                    Text(artifact.hash)
                        .font(ConsoleTypography.heading(11, weight: .regular))
                        .foregroundStyle(theme.info)
                }

                Button(viewModel.artifactDrawerExpanded ? "Collapse" : "Expand") {
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
                        Text("Preview")
                            .font(ConsoleTypography.heading(12, weight: .semibold))
                            .foregroundStyle(theme.textPrimary)

                        ScrollView {
                            Text(viewModel.refinedArtifactText.isEmpty ? "Select an artifact to preview." : viewModel.refinedArtifactText)
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
                        Text("Refine")
                            .font(ConsoleTypography.heading(12, weight: .semibold))
                            .foregroundStyle(theme.textPrimary)

                        TextField("keyword", text: $viewModel.refineKeyword)
                            .consoleInput(theme: theme, surface: .elevated)

                        HStack {
                            Stepper("from \(viewModel.refineRangeStart)", value: $viewModel.refineRangeStart, in: 1...9999)
                            Stepper("to \(viewModel.refineRangeEnd)", value: $viewModel.refineRangeEnd, in: 1...9999)
                        }
                        .font(ConsoleTypography.body(11))
                        .foregroundStyle(theme.textSecondary)

                        if let artifact = viewModel.selectedArtifact {
                            VStack(alignment: .leading, spacing: 4) {
                                Text("source: \(artifact.sourceCommand)")
                                Text("session: \(artifact.sessionID) / channel: \(artifact.channelID)")
                                if !artifact.parentHashes.isEmpty {
                                    Text("parents: \(artifact.parentHashes.joined(separator: ", "))")
                                }
                                if !artifact.derivedHashes.isEmpty {
                                    Text("derived: \(artifact.derivedHashes.joined(separator: ", "))")
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
