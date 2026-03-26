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

                Text(L10n.t("target_sheet.subtitle.structured_editor"))
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

            Button(L10n.t("target_sheet.cancel")) {
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
            Text(L10n.t("target_sheet.target_types"))
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
        SheetCard(title: L10n.t("target_sheet.section.general"), theme: theme) {
            VStack(spacing: 10) {
                EditorField(title: L10n.t("target_sheet.field.name"), text: $viewModel.draft.name, placeholder: L10n.t("target_sheet.placeholder.name"), theme: theme)
                    .accessibilityIdentifier("field-target-name")

                EditorField(title: L10n.t("target_sheet.field.alias_for_model"), text: $viewModel.draft.aliasForModel, placeholder: L10n.t("target_sheet.placeholder.alias_for_model"), theme: theme)
                    .accessibilityIdentifier("field-target-alias")

                EditorField(title: L10n.t("target_sheet.field.notes"), text: $viewModel.draft.notes, placeholder: L10n.t("target_sheet.placeholder.notes"), theme: theme)
                    .accessibilityIdentifier("field-target-notes")
            }
        }
    }

    private var connectionSection: some View {
        SheetCard(title: L10n.t("target_sheet.section.connection"), subtitle: helperTextForKind(viewModel.draft.kind), theme: theme) {
            switch viewModel.draft.kind {
            case .ssh:
                VStack(spacing: 10) {
                    EditorField(title: L10n.t("target_sheet.field.host"), text: $viewModel.draft.sshConfig.host, placeholder: L10n.t("target_sheet.placeholder.host"), theme: theme)
                        .accessibilityIdentifier("field-ssh-host")
                    NumericField(title: L10n.t("target_sheet.field.port"), value: $viewModel.draft.sshConfig.port, theme: theme)
                    EditorField(title: L10n.t("target_sheet.field.username"), text: $viewModel.draft.sshConfig.username, placeholder: L10n.t("target_sheet.placeholder.username"), theme: theme)
                        .accessibilityIdentifier("field-ssh-username")
                }
            case .adb:
                VStack(spacing: 10) {
                    EditorField(title: L10n.t("target_sheet.field.serial"), text: $viewModel.draft.adbConfig.serial, placeholder: L10n.t("target_sheet.placeholder.serial"), theme: theme)
                        .accessibilityIdentifier("field-adb-serial")
                    EditorField(title: L10n.t("target_sheet.field.transport"), text: $viewModel.draft.adbConfig.transport, placeholder: L10n.t("target_sheet.placeholder.transport"), theme: theme)
                }
            case .serial:
                VStack(spacing: 10) {
                    EditorField(title: L10n.t("target_sheet.field.device_path"), text: $viewModel.draft.serialConfig.devicePath, placeholder: L10n.t("target_sheet.placeholder.device_path"), theme: theme)
                    NumericField(title: L10n.t("target_sheet.field.baud_rate"), value: $viewModel.draft.serialConfig.baudRate, theme: theme)
                }
            case .docker:
                VStack(spacing: 10) {
                    EditorField(title: L10n.t("target_sheet.field.container"), text: $viewModel.draft.dockerConfig.containerName, placeholder: L10n.t("target_sheet.placeholder.container"), theme: theme)
                    EditorField(title: L10n.t("target_sheet.field.context"), text: $viewModel.draft.dockerConfig.context, placeholder: L10n.t("target_sheet.placeholder.context"), theme: theme)
                    EditorField(title: L10n.t("target_sheet.field.shell_preference"), text: $viewModel.draft.dockerConfig.shellPreference, placeholder: L10n.t("target_sheet.placeholder.shell_preference"), theme: theme)
                }
            case .httpDebug:
                VStack(spacing: 10) {
                    EditorField(title: L10n.t("target_sheet.field.base_url"), text: $viewModel.draft.httpDebugConfig.baseURL, placeholder: L10n.t("target_sheet.placeholder.base_url"), theme: theme)
                    EditorField(title: L10n.t("target_sheet.field.auth_reference"), text: $viewModel.draft.httpDebugConfig.authReference, placeholder: L10n.t("target_sheet.placeholder.auth_reference"), theme: theme)
                    EditorField(title: L10n.t("target_sheet.field.environment"), text: $viewModel.draft.httpDebugConfig.environmentPreset, placeholder: L10n.t("target_sheet.placeholder.environment"), theme: theme)
                    EditorField(title: L10n.t("target_sheet.field.headers_preset"), text: $viewModel.draft.httpDebugConfig.headerPreset, placeholder: L10n.t("target_sheet.placeholder.headers_preset"), theme: theme)
                }
            case .openGrok:
                VStack(spacing: 10) {
                    EditorField(title: L10n.t("target_sheet.field.endpoint"), text: $viewModel.draft.openGrokConfig.endpoint, placeholder: L10n.t("target_sheet.placeholder.endpoint"), theme: theme)
                    EditorField(title: L10n.t("target_sheet.field.repository_scope"), text: $viewModel.draft.openGrokConfig.repositoryScope, placeholder: L10n.t("target_sheet.placeholder.repository_scope"), theme: theme)
                    EditorField(title: L10n.t("target_sheet.field.token_reference"), text: $viewModel.draft.openGrokConfig.tokenReference, placeholder: L10n.t("target_sheet.placeholder.token_reference"), theme: theme)
                    EditorField(title: L10n.t("target_sheet.field.query_scope"), text: $viewModel.draft.openGrokConfig.queryScope, placeholder: L10n.t("target_sheet.placeholder.query_scope"), theme: theme)
                }
            }
        }
    }

    private var credentialSection: some View {
        SheetCard(title: L10n.t("target_sheet.section.credential"), theme: theme) {
            VStack(spacing: 10) {
                EditorField(title: L10n.t("target_sheet.field.credential_reference"), text: $viewModel.draft.credentialReference, placeholder: L10n.t("target_sheet.placeholder.credential_reference"), theme: theme)
                    .accessibilityIdentifier("field-credential-ref")

                HStack(spacing: 10) {
                    Button(L10n.t("target_sheet.import")) {}
                        .consoleButton(theme: theme, tone: .secondary)
                    Button(L10n.t("target_sheet.reveal_in_vault")) {}
                        .consoleButton(theme: theme, tone: .subtle)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }

    private var toolingSection: some View {
        SheetCard(title: L10n.t("target_sheet.section.tooling"), theme: theme) {
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

                        Text(L10n.f("workspace.tool.effective_path_format", diagnostic.effectivePath))
                            .font(ConsoleTypography.body(12))
                            .foregroundStyle(theme.textSecondary)

                        TextField(
                            L10n.t("workspace.tool.override_path_placeholder"),
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
        SheetCard(title: L10n.t("target_sheet.section.policy"), theme: theme) {
            VStack(alignment: .leading, spacing: 8) {
                Toggle(L10n.t("target_sheet.policy.approve_write"), isOn: $viewModel.draft.policyDefaults.approveWrite)
                Toggle(L10n.t("target_sheet.policy.approve_delete"), isOn: $viewModel.draft.policyDefaults.approveDelete)
                Toggle(L10n.t("target_sheet.policy.approve_sudo"), isOn: $viewModel.draft.policyDefaults.approveSudo)
                Toggle(L10n.t("target_sheet.policy.approve_sensitive_read"), isOn: $viewModel.draft.policyDefaults.approveSensitiveRead)
            }
            .toggleStyle(.checkbox)
            .font(ConsoleTypography.body(12))
            .foregroundStyle(theme.textSecondary)
        }
    }

    private func helperTextForKind(_ kind: TargetKind) -> String {
        switch kind {
        case .ssh:
            return L10n.t("target_sheet.helper.ssh")
        case .adb:
            return L10n.t("target_sheet.helper.adb")
        case .serial:
            return L10n.t("target_sheet.helper.serial")
        case .docker:
            return L10n.t("target_sheet.helper.docker")
        case .httpDebug:
            return L10n.t("target_sheet.helper.http")
        case .openGrok:
            return L10n.t("target_sheet.helper.opengrok")
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
