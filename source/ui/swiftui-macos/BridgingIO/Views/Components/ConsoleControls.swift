import SwiftUI

enum ConsoleButtonTone {
    case primary
    case secondary
    case subtle
    case danger
}

struct ConsoleButtonStyle: ButtonStyle {
    let theme: ConsoleTheme
    let tone: ConsoleButtonTone

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(ConsoleTypography.body(12, weight: .semibold))
            .foregroundStyle(foregroundColor)
            .padding(.horizontal, 12)
            .padding(.vertical, 7)
            .background(backgroundFill(forPressedState: configuration.isPressed))
            .overlay {
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .stroke(borderColor(forPressedState: configuration.isPressed), lineWidth: 1)
            }
            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
            .scaleEffect(configuration.isPressed ? 0.985 : 1)
            .animation(.easeOut(duration: 0.12), value: configuration.isPressed)
    }

    private var foregroundColor: Color {
        switch tone {
        case .primary:
            return .white
        case .secondary:
            return theme.textPrimary
        case .subtle:
            return theme.textSecondary
        case .danger:
            return .white
        }
    }

    private func backgroundFill(forPressedState isPressed: Bool) -> AnyShapeStyle {
        switch tone {
        case .primary:
            let top = theme.info.opacity(isPressed ? 0.74 : 0.88)
            let bottom = theme.info.opacity(isPressed ? 0.58 : 0.72)
            return AnyShapeStyle(LinearGradient(colors: [top, bottom], startPoint: .top, endPoint: .bottom))
        case .secondary:
            return AnyShapeStyle(theme.panel.opacity(isPressed ? 0.82 : 1))
        case .subtle:
            return AnyShapeStyle(theme.elevated.opacity(isPressed ? 0.8 : 0.92))
        case .danger:
            let top = theme.danger.opacity(isPressed ? 0.75 : 0.9)
            let bottom = theme.danger.opacity(isPressed ? 0.58 : 0.72)
            return AnyShapeStyle(LinearGradient(colors: [top, bottom], startPoint: .top, endPoint: .bottom))
        }
    }

    private func borderColor(forPressedState isPressed: Bool) -> Color {
        switch tone {
        case .primary:
            return theme.info.opacity(isPressed ? 0.9 : 0.75)
        case .secondary:
            return theme.border.opacity(isPressed ? 0.86 : 1)
        case .subtle:
            return theme.border.opacity(isPressed ? 0.64 : 0.82)
        case .danger:
            return theme.danger.opacity(isPressed ? 0.92 : 0.82)
        }
    }
}

extension View {
    func consoleButton(theme: ConsoleTheme, tone: ConsoleButtonTone = .secondary) -> some View {
        buttonStyle(ConsoleButtonStyle(theme: theme, tone: tone))
    }
}

enum ConsoleInputSurface {
    case panel
    case elevated

    func fillColor(theme: ConsoleTheme) -> Color {
        switch self {
        case .panel:
            return theme.panel
        case .elevated:
            return theme.elevated
        }
    }
}

struct ConsoleInputFieldModifier: ViewModifier {
    let theme: ConsoleTheme
    let surface: ConsoleInputSurface

    func body(content: Content) -> some View {
        content
            .textFieldStyle(.plain)
            .font(ConsoleTypography.body(12))
            .foregroundStyle(theme.textPrimary)
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .background(surface.fillColor(theme: theme))
            .overlay {
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .stroke(theme.border, lineWidth: 1)
            }
            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
    }
}

extension View {
    func consoleInput(theme: ConsoleTheme, surface: ConsoleInputSurface = .panel) -> some View {
        modifier(ConsoleInputFieldModifier(theme: theme, surface: surface))
    }
}

struct ConsoleIconTextInput: View {
    let placeholder: String
    @Binding var text: String
    let icon: String
    let theme: ConsoleTheme
    var surface: ConsoleInputSurface = .panel

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: icon)
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(theme.muted)

            TextField(placeholder, text: $text)
                .textFieldStyle(.plain)
                .font(ConsoleTypography.body(12))
                .foregroundStyle(theme.textPrimary)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 7)
        .background(surface.fillColor(theme: theme))
        .overlay {
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .stroke(theme.border, lineWidth: 1)
        }
        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
    }
}

struct ConsoleSegmentedControl<Option: Hashable & Identifiable>: View {
    @Binding var selection: Option
    let options: [Option]
    let theme: ConsoleTheme
    let title: (Option) -> String

    var body: some View {
        HStack(spacing: 6) {
            ForEach(options, id: \.id) { option in
                let selected = option == selection

                Button {
                    selection = option
                } label: {
                    Text(title(option))
                        .font(ConsoleTypography.body(11, weight: .semibold))
                        .foregroundStyle(selected ? selectedTextColor : theme.textSecondary)
                        .lineLimit(1)
                        .padding(.horizontal, 6)
                        .frame(maxWidth: .infinity, minHeight: 30, maxHeight: 30)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .contentShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
                .background(selected ? selectedBackground : AnyShapeStyle(.clear))
                .overlay {
                    RoundedRectangle(cornerRadius: 7, style: .continuous)
                        .stroke(selected ? selectedBorderColor : .clear, lineWidth: 1)
                }
                .clipShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
            }
        }
        .padding(4)
        .background(theme.elevated.opacity(0.78))
        .overlay {
            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .stroke(theme.border.opacity(0.92), lineWidth: 1)
        }
        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
    }

    private var selectedBackground: AnyShapeStyle {
        AnyShapeStyle(LinearGradient(
            colors: [theme.info.opacity(0.3), theme.info.opacity(0.2)],
            startPoint: .top,
            endPoint: .bottom
        ))
    }

    private var selectedTextColor: Color {
        theme.textPrimary
    }

    private var selectedBorderColor: Color {
        theme.info.opacity(0.58)
    }
}
