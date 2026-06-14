import SwiftUI

struct AlertsListView: View {
    @Environment(AlertsViewModel.self) private var viewModel
    @Environment(AuthManager.self) private var authManager
    @Environment(\.apiClient) private var apiClient

    private var isAdmin: Bool { authManager.user?.role.lowercased() == "admin" }

    #if DEBUG
    @State private var debugShowConfig = false
    #endif

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
        .toolbar {
            if isAdmin {
                ToolbarItem(placement: .topBarTrailing) {
                    NavigationLink {
                        AlertConfigView()
                    } label: {
                        Label(String(localized: "Alert config"), systemImage: "slider.horizontal.3")
                    }
                }
            }
        }
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
            #if DEBUG
            if isAdmin, UITestSupport.autoPresent == "alert-config" { debugShowConfig = true }
            #endif
        }
        #if DEBUG
        .navigationDestination(isPresented: $debugShowConfig) {
            AlertConfigView()
        }
        #endif
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
