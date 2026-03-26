import Foundation

/// Unified localization gateway for all user-visible copy in the macOS app.
///
/// Why this file exists:
/// - Keep one stable entry point instead of scattering NSLocalizedString/String(localized:) calls.
/// - Make lint/review rules straightforward: display text must resolve through `L10n`.
/// - Centralize formatted-string handling so parameterized copy stays consistent.
///
/// Usage convention:
/// - Static copy: `L10n.t("some.key")`
/// - Formatted copy: `L10n.f("some.key", arg1, arg2, ...)`
///
/// Guardrail:
/// - New user-visible strings should not be hardcoded in views/view-models.
/// - Add keys in Localizable resources and access them through this type.
enum L10n {
    static func t(_ key: String) -> String {
        let sentinel = "__missing__\(key)__"
        let localized = NSLocalizedString(key, tableName: "Localizable", bundle: .main, value: sentinel, comment: "")
        if localized != sentinel {
            return localized
        }

        if let englishPath = Bundle.main.path(forResource: "en", ofType: "lproj"),
           let englishBundle = Bundle(path: englishPath) {
            return NSLocalizedString(key, tableName: "Localizable", bundle: englishBundle, value: key, comment: "")
        }

        return key
    }

    static func f(_ key: String, _ arguments: CVarArg...) -> String {
        format(key, arguments: arguments)
    }

    static func format(_ key: String, arguments: [CVarArg]) -> String {
        let template = t(key)
        return String(format: template, locale: Locale.current, arguments: arguments)
    }
}
