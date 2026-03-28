import SwiftUI
import AppKit

@main
struct BridgingIOApp: App {
    @StateObject private var viewModel = WorkspaceViewModel()

    var body: some Scene {
        WindowGroup {
            ContentView(viewModel: viewModel)
                .onReceive(
                    NotificationCenter.default.publisher(for: NSApplication.willTerminateNotification)
                ) { _ in
                    viewModel.appWillTerminate()
                }
        }
    }
}
