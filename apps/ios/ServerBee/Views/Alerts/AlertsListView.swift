import SwiftUI

struct AlertsListView: View {
    @Environment(AlertsViewModel.self) private var viewModel
    @Environment(AuthManager.self) private var authManager
    @Environment(\.apiClient) private var apiClient
    @State private var filter: AlertEventFilter = .all

    private var isAdmin: Bool { authManager.user?.role.lowercased() == "admin" }

    #if DEBUG
    @State private var debugShowConfig = false
    #endif

    var body: some View {
        // A real container (not `Group`) so `.task` is attached once: `Group`
        // applies it to each branch, restarting (and cancelling) the fetch on
        // every loading/empty/error switch.
        ZStack {
            if viewModel.isLoading && viewModel.events.isEmpty {
                ProgressView(String(localized: "Loading alerts..."))
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let message = viewModel.errorMessage, viewModel.events.isEmpty {
                errorView(message: message)
            } else if viewModel.events.isEmpty {
                ContentUnavailableView {
                    Label(String(localized: "No Alerts"), systemImage: "bell.slash")
                } description: {
                    Text(String(localized: "No alert events to display"))
                }
            } else {
                eventsList
            }
        }
        .background(Color(.systemGroupedBackground))
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
}

private extension AlertsListView {
    var eventsList: some View {
        let visible = viewModel.events(matching: filter)
        let firing = visible.filter { $0.status == .firing }
        let earlier = visible.filter { $0.status == .resolved }
        return List {
            Section {
                filterPicker
                    .listRowInsets(EdgeInsets())
                    .listRowBackground(Color.clear)
            }

            if !firing.isEmpty {
                Section(String(localized: "Firing now")) {
                    eventRows(firing)
                }
            }

            if !earlier.isEmpty {
                Section(String(localized: "Earlier")) {
                    eventRows(earlier)
                }
            }

            if firing.isEmpty, earlier.isEmpty {
                Section {
                    filteredEmptyView
                        .listRowBackground(Color.clear)
                }
            }
        }
        .listStyle(.insetGrouped)
    }

    var filterPicker: some View {
        Picker(String(localized: "Filter"), selection: $filter) {
            ForEach(AlertEventFilter.allCases) { option in
                Text(title(for: option)).tag(option)
            }
        }
        .pickerStyle(.segmented)
        .labelsHidden()
    }

    func title(for option: AlertEventFilter) -> String {
        switch option {
        case .all:
            return String(localized: "All")
        case .firing:
            let count = viewModel.firingCount
            return count > 0 ? String(localized: "Firing \(count)") : String(localized: "Firing")
        case .resolved:
            return String(localized: "Resolved")
        }
    }

    func eventRows(_ events: [MobileAlertEvent]) -> some View {
        ForEach(events) { event in
            // Typed to the Alerts stack's path element so the push is recorded
            // in (and restorable from) `ContentView.alertsPath`.
            NavigationLink(value: ServerDeepLink.alertDetail(alertKey: event.alertKey)) {
                AlertEventCardView(event: event)
            }
        }
    }

    var filteredEmptyView: some View {
        ContentUnavailableView {
            Label(
                filter == .firing ? String(localized: "No firing alerts") : String(localized: "No resolved alerts"),
                systemImage: filter == .firing ? "checkmark.circle" : "bell.slash"
            )
        }
    }

    func errorView(message: String) -> some View {
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
