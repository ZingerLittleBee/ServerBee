import SwiftUI

struct AlertsListView: View {
    @Environment(AlertsViewModel.self) private var viewModel
    @Environment(\.apiClient) private var apiClient

    var body: some View {
        Group {
            if viewModel.isLoading && viewModel.events.isEmpty {
                ProgressView(String(localized: "Loading alerts..."))
            } else if let message = viewModel.errorMessage, viewModel.events.isEmpty {
                errorView(message: message)
            } else if viewModel.events.isEmpty {
                ContentUnavailableView {
                    Label(String(localized: "No Alerts"), systemImage: "bell.slash")
                } description: {
                    Text(String(localized: "No alert events to display"))
                }
            } else {
                List {
                    ForEach(viewModel.events) { event in
                        NavigationLink(value: event.alertKey) {
                            AlertEventCardView(event: event)
                        }
                    }
                }
                .listStyle(.plain)
            }
        }
        .navigationTitle(String(localized: "Alerts"))
        .navigationDestination(for: String.self) { alertKey in
            AlertDetailView(alertKey: alertKey)
        }
        .refreshable {
            await viewModel.refresh(apiClient: apiClient)
        }
        .task {
            if viewModel.events.isEmpty {
                await viewModel.fetchEvents(apiClient: apiClient)
            }
        }
    }

    private func errorView(message: String) -> some View {
        ContentUnavailableView {
            Label(String(localized: "Couldn't load alerts"), systemImage: "exclamationmark.triangle")
        } description: {
            Text(message)
        } actions: {
            Button(String(localized: "Try again")) {
                Task {
                    await viewModel.fetchEvents(apiClient: apiClient)
                }
            }
            .buttonStyle(.borderedProminent)
        }
    }
}
