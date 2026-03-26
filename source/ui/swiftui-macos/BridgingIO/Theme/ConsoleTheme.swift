import SwiftUI

enum AppearanceMode: String, CaseIterable, Identifiable {
    case system
    case light
    case dark

    var id: String { rawValue }

    var title: String {
        switch self {
        case .system:
            return L10n.t("appearance.mode.system")
        case .light:
            return L10n.t("appearance.mode.light")
        case .dark:
            return L10n.t("appearance.mode.dark")
        }
    }

    var colorSchemeOverride: ColorScheme? {
        switch self {
        case .system:
            return nil
        case .light:
            return .light
        case .dark:
            return .dark
        }
    }

}

struct ConsoleTheme {
    var canvas: Color
    var panel: Color
    var elevated: Color
    var border: Color
    var textPrimary: Color
    var textSecondary: Color
    var muted: Color
    var success: Color
    var info: Color
    var warning: Color
    var danger: Color

    static func resolve(mode: AppearanceMode, system: ColorScheme) -> ConsoleTheme {
        let scheme: ColorScheme
        switch mode {
        case .system:
            scheme = system
        case .light:
            scheme = .light
        case .dark:
            scheme = .dark
        }

        if scheme == .dark {
            return ConsoleTheme(
                canvas: Color(hex: "0F172A"),
                panel: Color(hex: "111827"),
                elevated: Color(hex: "1F2937"),
                border: Color(hex: "334155"),
                textPrimary: Color(hex: "F8FAFC"),
                textSecondary: Color(hex: "CBD5E1"),
                muted: Color(hex: "94A3B8"),
                success: Color(hex: "22C55E"),
                info: Color(hex: "38BDF8"),
                warning: Color(hex: "F59E0B"),
                danger: Color(hex: "EF4444")
            )
        }

        return ConsoleTheme(
            canvas: Color(hex: "F3F7FB"),
            panel: Color(hex: "FFFFFF"),
            elevated: Color(hex: "E8EEF5"),
            border: Color(hex: "C9D5E4"),
            textPrimary: Color(hex: "0F172A"),
            textSecondary: Color(hex: "334155"),
            muted: Color(hex: "64748B"),
            success: Color(hex: "15803D"),
            info: Color(hex: "0369A1"),
            warning: Color(hex: "B45309"),
            danger: Color(hex: "B91C1C")
        )
    }
}

enum ConsoleTypography {
    static func heading(_ size: CGFloat, weight: Font.Weight = .semibold) -> Font {
        Font.custom("JetBrains Mono", size: size).weight(weight)
    }

    static func body(_ size: CGFloat, weight: Font.Weight = .regular) -> Font {
        Font.custom("IBM Plex Sans", size: size).weight(weight)
    }
}

private extension Color {
    init(hex: String) {
        let cleaned = hex.replacingOccurrences(of: "#", with: "")
        let value = UInt64(cleaned, radix: 16) ?? 0
        let red = Double((value >> 16) & 0xFF) / 255.0
        let green = Double((value >> 8) & 0xFF) / 255.0
        let blue = Double(value & 0xFF) / 255.0
        self = Color(.sRGB, red: red, green: green, blue: blue, opacity: 1)
    }
}
