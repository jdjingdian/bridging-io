import SwiftUI

struct TargetProfileSheetView: View {
    @StateObject private var viewModel: TargetEditorViewModel
    @StateObject private var systemAppearance = SystemAppearanceObserver()

    let appearanceMode: AppearanceMode
    let onCancel: () -> Void
    let onSave: (TargetProfileDraft) -> Void

    init(
        viewModel: TargetEditorViewModel,
        appearanceMode: AppearanceMode,
        onCancel: @escaping () -> Void,
        onSave: @escaping (TargetProfileDraft) -> Void
    ) {
        _viewModel = StateObject(wrappedValue: viewModel)
        self.appearanceMode = appearanceMode
        self.onCancel = onCancel
        self.onSave = onSave
    }

    private var theme: ConsoleTheme {
        ConsoleTheme.resolve(mode: appearanceMode, system: systemAppearance.colorScheme)
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider().overlay(theme.border)

            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    kindTabs
                    generalSection
                    connectionSection
                    credentialSection
                    toolingSection
                    policySection
                }
                .padding(16)
            }
        }
        .background(theme.canvas)
        .preferredColorScheme(appearanceMode.colorSchemeOverride)
        .onChange(of: appearanceMode) { _, newMode in
            if newMode == .system {
                systemAppearance.refresh()
            }
        }
    }

    private var header: some View {
        HStack {
            VStack(alignment: .leading, spacing: 3) {
                Text(viewModel.title)
                    .font(ConsoleTypography.heading(17, weight: .bold))
                    .foregroundStyle(theme.textPrimary)

                Text("Structured target profile editor")
                    .font(ConsoleTypography.body(12))
                    .foregroundStyle(theme.muted)
            }

            Spacer()

            if let message = viewModel.validationMessage {
                Text(message)
                    .font(ConsoleTypography.body(12))
                    .foregroundStyle(theme.warning)
                    .lineLimit(1)
            }

            Button("Cancel") {
                onCancel()
            }
            .consoleButton(theme: theme, tone: .secondary)
            .accessibilityIdentifier("target-sheet-cancel")

            Button(viewModel.actionTitle) {
                guard viewModel.validate() else { return }
                onSave(viewModel.draft)
            }
            .consoleButton(theme: theme, tone: .primary)
            .accessibilityIdentifier("target-sheet-save")
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .background(theme.panel)
    }

    private var kindTabs: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Target Types")
                .font(ConsoleTypography.heading(12, weight: .semibold))
                .foregroundStyle(theme.textSecondary)

            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(TargetKind.allCases) { kind in
                        Button(kind.title) {
                            viewModel.draft.kind = kind
                        }
                        .buttonStyle(.plain)
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .background(viewModel.draft.kind == kind ? theme.info.opacity(0.14) : theme.panel)
                        .foregroundStyle(viewModel.draft.kind == kind ? theme.info : theme.textSecondary)
                        .overlay {
                            Capsule().stroke(viewModel.draft.kind == kind ? theme.info : theme.border, lineWidth: 1)
                        }
                        .clipShape(Capsule())
                        .accessibilityIdentifier("target-kind-\(kind.rawValue)")
                    }
                }
            }
        }
    }

    private var generalSection: some View {
        SheetCard(title: "General", theme: theme) {
            VStack(spacing: 10) {
                EditorField(title: "Name", text: $viewModel.draft.name, placeholder: "ops-prod", theme: theme)
                    .accessibilityIdentifier("field-target-name")

                EditorField(title: "Alias for model", text: $viewModel.draft.aliasForModel, placeholder: "prod-bastion", theme: theme)
                    .accessibilityIdentifier("field-target-alias")

                EditorField(title: "Notes", text: $viewModel.draft.notes, placeholder: "production jump host", theme: theme)
                    .accessibilityIdentifier("field-target-notes")
            }
        }
    }

    private var connectionSection: some View {
        SheetCard(title: "Connection", subtitle: helperTextForKind(viewModel.draft.kind), theme: theme) {
            switch viewModel.draft.kind {
            case .ssh:
                VStack(spacing: 10) {
                    EditorField(title: "Host", text: $viewModel.draft.sshConfig.host, placeholder: "10.0.0.8", theme: theme)
                        .accessibilityIdentifier("field-ssh-host")
                    NumericField(title: "Port", value: $viewModel.draft.sshConfig.port, theme: theme)
                    EditorField(title: "Username", text: $viewModel.draft.sshConfig.username, placeholder: "ops", theme: theme)
                        .accessibilityIdentifier("field-ssh-username")
                }
            case .adb:
                VStack(spacing: 10) {
                    EditorField(title: "Serial", text: $viewModel.draft.adbConfig.serial, placeholder: "ABC12345", theme: theme)
                        .accessibilityIdentifier("field-adb-serial")
                    EditorField(title: "Transport", text: $viewModel.draft.adbConfig.transport, placeholder: "usb", theme: theme)
                }
            case .serial:
                VStack(spacing: 10) {
                    EditorField(title: "Device path", text: $viewModel.draft.serialConfig.devicePath, placeholder: "/dev/tty.usbmodem0", theme: theme)
                    NumericField(title: "Baud rate", value: $viewModel.draft.serialConfig.baudRate, theme: theme)
                }
            case .docker:
                VStack(spacing: 10) {
                    EditorField(title: "Container", text: $viewModel.draft.dockerConfig.containerName, placeholder: "bridge-agent", theme: theme)
                    EditorField(title: "Context", text: $viewModel.draft.dockerConfig.context, placeholder: "default", theme: theme)
                    EditorField(title: "Shell preference", text: $viewModel.draft.dockerConfig.shellPreference, placeholder: "/bin/sh", theme: theme)
                }
            case .httpDebug:
                VStack(spacing: 10) {
                    EditorField(title: "Base URL", text: $viewModel.draft.httpDebugConfig.baseURL, placeholder: "https://api.example.com", theme: theme)
                    EditorField(title: "Auth reference", text: $viewModel.draft.httpDebugConfig.authReference, placeholder: "vault:http-auth:dev", theme: theme)
                    EditorField(title: "Environment", text: $viewModel.draft.httpDebugConfig.environmentPreset, placeholder: "dev", theme: theme)
                    EditorField(title: "Headers preset", text: $viewModel.draft.httpDebugConfig.headerPreset, placeholder: "json", theme: theme)
                }
            case .openGrok:
                VStack(spacing: 10) {
                    EditorField(title: "Endpoint", text: $viewModel.draft.openGrokConfig.endpoint, placeholder: "https://grok.example.com", theme: theme)
                    EditorField(title: "Repository scope", text: $viewModel.draft.openGrokConfig.repositoryScope, placeholder: "bridgingio/*", theme: theme)
                    EditorField(title: "Token reference", text: $viewModel.draft.openGrokConfig.tokenReference, placeholder: "vault:grok-token:default", theme: theme)
                    EditorField(title: "Query scope", text: $viewModel.draft.openGrokConfig.queryScope, placeholder: "default", theme: theme)
                }
            }
        }
    }

    private var credentialSection: some View {
        SheetCard(title: "Credential", theme: theme) {
            VStack(spacing: 10) {
                EditorField(title: "Credential reference", text: $viewModel.draft.credentialReference, placeholder: "vault:ssh-key:ops-prod", theme: theme)
                    .accessibilityIdentifier("field-credential-ref")

                HStack(spacing: 10) {
                    Button("Import") {}
                        .consoleButton(theme: theme, tone: .secondary)
                    Button("Reveal in Vault") {}
                        .consoleButton(theme: theme, tone: .subtle)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }

    private var toolingSection: some View {
        SheetCard(title: "Tooling", theme: theme) {
            VStack(alignment: .leading, spacing: 10) {
                ForEach(Array(viewModel.draft.toolDiagnostics.enumerated()), id: \.element.id) { index, diagnostic in
                    VStack(alignment: .leading, spacing: 6) {
                        HStack {
                            Text(diagnostic.connectorName.uppercased())
                                .font(ConsoleTypography.heading(12, weight: .semibold))
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
                                get: { viewModel.draft.toolDiagnostics[index].overridePath },
                                set: { viewModel.applyConnectorOverride(for: diagnostic.id, path: $0) }
                            )
                        )
                        .consoleInput(theme: theme, surface: .panel)
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

    private var policySection: some View {
        SheetCard(title: "Policy", theme: theme) {
            VStack(alignment: .leading, spacing: 8) {
                Toggle("Approve write", isOn: $viewModel.draft.policyDefaults.approveWrite)
                Toggle("Approve delete", isOn: $viewModel.draft.policyDefaults.approveDelete)
                Toggle("Approve sudo", isOn: $viewModel.draft.policyDefaults.approveSudo)
                Toggle("Approve sensitive read", isOn: $viewModel.draft.policyDefaults.approveSensitiveRead)
            }
            .toggleStyle(.checkbox)
            .font(ConsoleTypography.body(12))
            .foregroundStyle(theme.textSecondary)
        }
    }

    private func helperTextForKind(_ kind: TargetKind) -> String {
        switch kind {
        case .ssh:
            return "Host key and tool source diagnostics stay visible for SSH." 
        case .adb:
            return "Use serial and transport to avoid device routing ambiguity."
        case .serial:
            return "Validate path and baud-rate before opening channels."
        case .docker:
            return "Container and context should resolve before opening a shell."
        case .httpDebug:
            return "Auth uses references only; no secret values shown here."
        case .openGrok:
            return "Endpoint and repo scope define searchable index boundaries."
        }
    }
}

private struct SheetCard<Content: View>: View {
    let title: String
    var subtitle: String?
    let theme: ConsoleTheme
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title)
                .font(ConsoleTypography.heading(13, weight: .semibold))
                .foregroundStyle(theme.textPrimary)

            if let subtitle {
                Text(subtitle)
                    .font(ConsoleTypography.body(11))
                    .foregroundStyle(theme.muted)
            }

            content
        }
        .padding(12)
        .background(theme.panel)
        .overlay {
            RoundedRectangle(cornerRadius: 10)
                .stroke(theme.border, lineWidth: 1)
        }
        .clipShape(RoundedRectangle(cornerRadius: 10))
    }
}

private struct EditorField: View {
    let title: String
    @Binding var text: String
    let placeholder: String
    let theme: ConsoleTheme

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(ConsoleTypography.body(11))
                .foregroundStyle(theme.muted)

            TextField(placeholder, text: $text)
                .consoleInput(theme: theme, surface: .panel)
                .font(ConsoleTypography.body(12))
        }
    }
}

private struct NumericField: View {
    let title: String
    @Binding var value: Int
    let theme: ConsoleTheme

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(ConsoleTypography.body(11))
                .foregroundStyle(theme.muted)

            TextField(title, value: $value, format: .number)
                .consoleInput(theme: theme, surface: .panel)
                .font(ConsoleTypography.body(12))
        }
    }
}
