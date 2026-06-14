import SwiftUI

/// Interactive traceroute runner presented as a sheet from the Network section.
///
/// Traceroute is member-allowed (a read-only network diagnostic) but it does
/// dispatch a task to the agent, so the run button is disabled when the server
/// is offline and the input is validated before sending.
struct TracerouteView: View {
    let serverId: String
    let serverOnline: Bool

    @Environment(\.apiClient) private var apiClient
    @Environment(\.dismiss) private var dismiss
    @State private var viewModel = TracerouteViewModel()
    @FocusState private var targetFocused: Bool

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 16) {
                    inputCard
                    if !serverOnline {
                        offlineNote
                    }
                    if let error = viewModel.errorMessage {
                        errorBanner(error)
                    }
                    if let snapshot = viewModel.snapshot {
                        TracerouteResultCard(snapshot: snapshot, isRunning: viewModel.isRunning)
                    } else if viewModel.isRunning {
                        runningCard
                    }
                    if !viewModel.history.isEmpty {
                        historyCard
                    }
                }
                .padding(16)
            }
            .background(Color(.systemGroupedBackground))
            .navigationTitle(String(localized: "Traceroute"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(String(localized: "Done")) { dismiss() }
                }
            }
            .task {
                await viewModel.loadHistory(serverId: serverId, apiClient: apiClient)
            }
        }
    }

    private var inputCard: some View {
        SectionCard {
            VStack(spacing: 12) {
                TextField(String(localized: "Host or IP"), text: $viewModel.target)
                    .textFieldStyle(.roundedBorder)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .keyboardType(.URL)
                    .focused($targetFocused)
                    .submitLabel(.go)
                    .onSubmit(runIfPossible)

                Picker(String(localized: "Protocol"), selection: $viewModel.protocolValue) {
                    ForEach(TraceProtocol.allCases) { p in
                        Text(p.label).tag(p)
                    }
                }
                .pickerStyle(.segmented)

                Button(action: runIfPossible) {
                    HStack {
                        if viewModel.isRunning {
                            ProgressView().controlSize(.small)
                        }
                        Text(viewModel.isRunning ? String(localized: "Running…") : String(localized: "Run"))
                            .frame(maxWidth: .infinity)
                    }
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
                .disabled(!canRun)
            }
        }
    }

    private var canRun: Bool {
        serverOnline
            && !viewModel.isRunning
            && !viewModel.target.trimmingCharacters(in: .whitespaces).isEmpty
    }

    private func runIfPossible() {
        guard canRun else { return }
        targetFocused = false
        Task { await viewModel.run(serverId: serverId, apiClient: apiClient) }
    }

    private var offlineNote: some View {
        Label(String(localized: "Server is offline — traceroute is unavailable"), systemImage: "wifi.slash")
            .font(.caption)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func errorBanner(_ message: String) -> some View {
        Label(message, systemImage: "exclamationmark.triangle.fill")
            .font(.subheadline)
            .foregroundStyle(Color.serverOffline)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(12)
            .background(Color.serverOffline.opacity(0.1))
            .clipShape(RoundedRectangle(cornerRadius: 10))
    }

    private var runningCard: some View {
        SectionCard {
            HStack(spacing: 12) {
                ProgressView()
                Text(String(localized: "Tracing route…"))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .center)
        }
    }

    private var historyCard: some View {
        SectionCard(String(localized: "Recent"), systemImage: "clock.arrow.circlepath") {
            VStack(spacing: 0) {
                ForEach(viewModel.history) { record in
                    Button {
                        viewModel.target = record.target
                        Task { await viewModel.open(serverId: serverId, requestId: record.requestId, apiClient: apiClient) }
                    } label: {
                        HStack {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(record.target)
                                    .font(.subheadline)
                                    .foregroundStyle(.primary)
                                Text(record.startedDate, style: .relative)
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            if record.hasError {
                                Image(systemName: "exclamationmark.triangle")
                                    .foregroundStyle(Color.warningAmber)
                            }
                            Text(String(localized: "\(record.hopCount) hops"))
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            Image(systemName: "chevron.right")
                                .font(.caption2)
                                .foregroundStyle(.tertiary)
                        }
                        .padding(.vertical, 8)
                    }
                    .buttonStyle(.plain)
                    if record.id != viewModel.history.last?.id {
                        Divider()
                    }
                }
            }
        }
    }
}

/// Renders the hop table for a traceroute snapshot.
struct TracerouteResultCard: View {
    let snapshot: TracerouteSnapshot
    let isRunning: Bool

    var body: some View {
        SectionCard {
            VStack(alignment: .leading, spacing: 10) {
                header
                Divider()
                ForEach(snapshot.hops) { hop in
                    hopRow(hop)
                    if hop.id != snapshot.hops.last?.id {
                        Divider().opacity(0.4)
                    }
                }
                if snapshot.hops.isEmpty {
                    if let error = snapshot.error {
                        Label(error, systemImage: "exclamationmark.triangle.fill")
                            .font(.caption)
                            .foregroundStyle(Color.serverOffline)
                    } else {
                        Text(String(localized: "Waiting for hops…"))
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }
        }
    }

    private var header: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(snapshot.target)
                    .font(.headline)
                Text("\(snapshot.protocolValue.uppercased()) · \(String(localized: "round \(snapshot.round)/\(snapshot.totalRounds))"))")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            if isRunning {
                ProgressView().controlSize(.small)
            } else if snapshot.error != nil {
                Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(Color.serverOffline)
            } else if snapshot.completed {
                Image(systemName: "checkmark.circle.fill").foregroundStyle(Color.serverOnline)
            }
        }
    }

    private func hopRow(_ hop: TracerouteHop) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Text("\(hop.hop)")
                .font(.subheadline.monospacedDigit().bold())
                .foregroundStyle(.secondary)
                .frame(width: 26, alignment: .trailing)

            VStack(alignment: .leading, spacing: 2) {
                if let ip = hop.primaryIP {
                    HStack(spacing: 6) {
                        Text(ip)
                            .font(.subheadline.monospaced())
                        if hop.extraIPCount > 0 {
                            Chip(text: "+\(hop.extraIPCount)", color: .secondary)
                        }
                    }
                    if let host = hop.hostname, host != ip {
                        Text(host)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                    if let asn = hop.asn, !asn.isEmpty {
                        Text(asn)
                            .font(.caption2)
                            .foregroundStyle(.tertiary)
                    }
                } else {
                    Text("*")
                        .font(.subheadline.monospaced())
                        .foregroundStyle(.tertiary)
                }
            }
            Spacer()
            VStack(alignment: .trailing, spacing: 2) {
                Text(NetworkFormat.latency(hop.displayLatency))
                    .font(.subheadline.monospacedDigit())
                    .foregroundStyle(hop.isUnresponsive ? AnyShapeStyle(.tertiary) : AnyShapeStyle(Color.primary))
                if let loss = hop.lossRatio, loss > 0 {
                    Text(NetworkFormat.loss(loss))
                        .font(.caption2)
                        .foregroundStyle(loss >= 0.5 ? Color.serverOffline : Color.warningAmber)
                }
            }
        }
        .padding(.vertical, 2)
        .opacity(hop.isUnresponsive ? 0.6 : 1)
    }
}
