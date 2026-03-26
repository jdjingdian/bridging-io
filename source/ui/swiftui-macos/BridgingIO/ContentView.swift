import SwiftUI

struct ContentView: View {
    @StateObject private var viewModel: WorkspaceViewModel

    init(viewModel: WorkspaceViewModel? = nil) {
        if let viewModel {
            _viewModel = StateObject(wrappedValue: viewModel)
        } else if ProcessInfo.processInfo.arguments.contains("--use-fixture-data") {
            _viewModel = StateObject(
                wrappedValue: WorkspaceViewModel(dataSource: WorkspaceViewModel.fixtureDataSource())
            )
        } else {
            _viewModel = StateObject(wrappedValue: WorkspaceViewModel())
        }
    }

    var body: some View {
        WorkspaceConsoleView(viewModel: viewModel)
            .frame(minWidth: 1320, minHeight: 860)
    }
}

#Preview {
    ContentView(
        viewModel: WorkspaceViewModel(dataSource: WorkspaceViewModel.fixtureDataSource())
    )
}
