import SwiftUI

struct AlertsListView: View {
    @Environment(AlertsViewModel.self) private var viewModel
    @Environment(\.apiClient) private var apiClient

    var body: some View {
        Group {
            if viewModel.isLoading && viewModel.events.isEmpty {
                ProgressView(String(localized: "Loading alerts..."))
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
            if let apiClient {
                await viewModel.refresh(apiClient: apiClient)
            }
        }
        .task {
            if viewModel.events.isEmpty, let apiClient {
                await viewModel.fetchEvents(apiClient: apiClient)
            }
        }
    }
}
