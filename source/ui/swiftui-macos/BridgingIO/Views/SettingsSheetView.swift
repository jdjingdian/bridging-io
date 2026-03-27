import SwiftUI

struct SettingsSheetView: View {
    @ObservedObject var viewModel: WorkspaceViewModel
    @Environment(\.dismiss) private var dismiss
    @StateObject private var systemAppearance = SystemAppearanceObserver()

    private var theme: ConsoleTheme {
        ConsoleTheme.resolve(mode: viewModel.appearanceMode, system: systemAppearance.colorScheme)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                Text(L10n.t("settings.title"))
                    .font(ConsoleTypography.heading(18, weight: .bold))
                    .foregroundStyle(theme.textPrimary)
                Spacer()
                Button(L10n.t("settings.done")) {
                    dismiss()
                }
                .consoleButton(theme: theme, tone: .primary)
                .accessibilityIdentifier("settings-done")
            }

            VStack(alignment: .leading, spacing: 10) {
                Text(L10n.t("settings.appearance"))
                    .font(ConsoleTypography.heading(13, weight: .semibold))
                    .foregroundStyle(theme.textPrimary)

                ConsoleSegmentedControl(
                    selection: $viewModel.appearanceMode,
                    options: AppearanceMode.allCases,
                    theme: theme,
                    title: { $0.title }
                )
            }
            .padding(12)
            .background(theme.panel)
            .overlay {
                RoundedRectangle(cornerRadius: 10)
                    .stroke(theme.border, lineWidth: 1)
            }
            .clipShape(RoundedRectangle(cornerRadius: 10))

            VStack(alignment: .leading, spacing: 10) {
                Text("Model Plane")
                    .font(ConsoleTypography.heading(13, weight: .semibold))
                    .foregroundStyle(theme.textPrimary)

                VStack(alignment: .leading, spacing: 6) {
                    Text("Host")
                        .font(ConsoleTypography.body(12))
                        .foregroundStyle(theme.muted)
                    TextField("127.0.0.1", text: $viewModel.modelPlaneHost)
                        .consoleInput(theme: theme, surface: .panel)
                }

                VStack(alignment: .leading, spacing: 6) {
                    Text("Port")
                        .font(ConsoleTypography.body(12))
                        .foregroundStyle(theme.muted)
                    TextField(
                        "19718",
                        text: Binding(
                            get: { "\(viewModel.modelPlanePort)" },
                            set: { viewModel.modelPlanePort = Int($0) ?? viewModel.modelPlanePort }
                        )
                    )
                    .consoleInput(theme: theme, surface: .panel)
                }

                Text("Default: 127.0.0.1:19718. Saving will request managed core restart when needed.")
                    .font(ConsoleTypography.body(12))
                    .foregroundStyle(theme.muted)

                Button("Save Model Plane") {
                    viewModel.saveModelPlaneSettings()
                }
                .consoleButton(theme: theme, tone: .primary)
                .accessibilityIdentifier("settings-save-model-plane")
            }
            .padding(12)
            .background(theme.panel)
            .overlay {
                RoundedRectangle(cornerRadius: 10)
                    .stroke(theme.border, lineWidth: 1)
            }
            .clipShape(RoundedRectangle(cornerRadius: 10))

            VStack(alignment: .leading, spacing: 10) {
                Text("Tool Sources")
                    .font(ConsoleTypography.heading(13, weight: .semibold))
                    .foregroundStyle(theme.textPrimary)

                if viewModel.globalToolchains.isEmpty {
                    Text("No global tool overrides configured.")
                        .font(ConsoleTypography.body(12))
                        .foregroundStyle(theme.muted)
                } else {
                    ForEach(Array(viewModel.globalToolchains.enumerated()), id: \.element.id) { _, setting in
                        VStack(alignment: .leading, spacing: 8) {
                            Text(setting.command.uppercased())
                                .font(ConsoleTypography.heading(12, weight: .semibold))
                                .foregroundStyle(theme.textPrimary)

                            TextField(
                                "/path/to/\(setting.command)",
                                text: Binding(
                                    get: {
                                        viewModel.globalToolchains
                                            .first(where: { $0.command == setting.command })?
                                            .pathOverride ?? ""
                                    },
                                    set: { newValue in
                                        viewModel.updateGlobalToolchainDraft(
                                            command: setting.command,
                                            path: newValue
                                        )
                                    }
                                )
                            )
                            .consoleInput(theme: theme, surface: .panel)

                            Button("Save \(setting.command.uppercased()) Override") {
                                viewModel.saveGlobalToolchain(command: setting.command)
                            }
                            .consoleButton(theme: theme, tone: .secondary)
                            .accessibilityIdentifier("settings-save-tool-\(setting.command)")
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
            .padding(12)
            .background(theme.panel)
            .overlay {
                RoundedRectangle(cornerRadius: 10)
                    .stroke(theme.border, lineWidth: 1)
            }
            .clipShape(RoundedRectangle(cornerRadius: 10))

            VStack(alignment: .leading, spacing: 10) {
                Text(L10n.t("settings.artifact_cache"))
                    .font(ConsoleTypography.heading(13, weight: .semibold))
                    .foregroundStyle(theme.textPrimary)

                ConsoleSegmentedControl(
                    selection: Binding(
                        get: { viewModel.cacheSettings.backend },
                        set: { viewModel.updateCacheBackend($0) }
                    ),
                    options: ArtifactCacheBackend.allCases,
                    theme: theme,
                    title: { $0.title }
                )

                TextField(L10n.t("settings.cache.root_path_placeholder"), text: $viewModel.cacheSettings.rootPath)
                    .consoleInput(theme: theme, surface: .panel)

                HStack {
                    Stepper(L10n.f("settings.cache.max_value_format", viewModel.cacheSettings.maxCacheMB), value: $viewModel.cacheSettings.maxCacheMB, in: 256...8192, step: 128)
                    Spacer()
                    Text(L10n.f("settings.cache.used_value_format", viewModel.cacheSettings.usedCacheMB))
                        .font(ConsoleTypography.body(12))
                        .foregroundStyle(theme.muted)
                }

                HStack(spacing: 8) {
                    Button(L10n.t("settings.apply")) {
                        viewModel.applyCacheSettings()
                    }
                    .consoleButton(theme: theme, tone: .primary)

                    Button(L10n.t("settings.clear_cache")) {
                        viewModel.clearArtifactCache()
                    }
                    .consoleButton(theme: theme, tone: .secondary)
                }

                if viewModel.cacheSettingsNeedRestart {
                    Text(L10n.t("settings.cache.restart_hint"))
                        .font(ConsoleTypography.body(12))
                        .foregroundStyle(theme.warning)
                }
            }
            .padding(12)
            .background(theme.panel)
            .overlay {
                RoundedRectangle(cornerRadius: 10)
                    .stroke(theme.border, lineWidth: 1)
            }
            .clipShape(RoundedRectangle(cornerRadius: 10))

            Spacer()
        }
        .padding(16)
        .background(theme.canvas)
        .preferredColorScheme(viewModel.appearanceMode.colorSchemeOverride)
        .onChange(of: viewModel.appearanceMode) { _, newMode in
            if newMode == .system {
                systemAppearance.refresh()
            }
        }
    }
}
