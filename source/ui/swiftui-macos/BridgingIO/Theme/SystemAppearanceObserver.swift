import SwiftUI
import AppKit
import Combine

@MainActor
final class SystemAppearanceObserver: ObservableObject {
    @Published private(set) var colorScheme: ColorScheme
    private var cancellables: Set<AnyCancellable> = []

    init() {
        colorScheme = Self.readSystemColorScheme()

        let themeDidChange = DistributedNotificationCenter.default()
            .publisher(for: Notification.Name("AppleInterfaceThemeChangedNotification"))
        let appDidBecomeActive = NotificationCenter.default
            .publisher(for: NSApplication.didBecomeActiveNotification)

        themeDidChange
            .merge(with: appDidBecomeActive)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                self?.refresh()
            }
            .store(in: &cancellables)
    }

    func refresh() {
        let next = Self.readSystemColorScheme()
        if next != colorScheme {
            colorScheme = next
        }
    }

    private static func readSystemColorScheme() -> ColorScheme {
        let interfaceStyle = UserDefaults.standard
            .persistentDomain(forName: UserDefaults.globalDomain)?["AppleInterfaceStyle"] as? String
        return interfaceStyle?.lowercased() == "dark" ? .dark : .light
    }
}
