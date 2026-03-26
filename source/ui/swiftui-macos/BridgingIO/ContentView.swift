import SwiftUI

struct ContentView: View {
    @StateObject private var viewModel = WorkspaceViewModel()

    var body: some View {
        WorkspaceConsoleView(viewModel: viewModel)
            .frame(minWidth: 1320, minHeight: 860)
    }
}

#Preview {
    ContentView()
}
