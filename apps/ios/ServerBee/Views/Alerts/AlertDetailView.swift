import SwiftUI

struct AlertDetailView: View {
    let alertKey: String
    @State private var viewModel = AlertDetailViewModel()
    @Environment(\.apiClient) private var apiClient

    var body: some View {
        Group {
            if viewModel.isLoading {
                ProgressView()
            } else if let errorMessage = viewModel.errorMessage {
                ContentUnavailableView(errorMessage, systemImage: "exclamationmark.triangle")
            } else if let detail = viewModel.detail {
                ScrollView {
                    VStack(spacing: 20) {
                        AlertStatusBadge(
                            status: detail.status,
                            font: .headline.bold(),
                            horizontalPadding: 20,
                            verticalPadding: 8
                        )

                        VStack(spacing: 0) {
                            InfoRow(label: String(localized: "Rule Name"), value: detail.ruleName)
                            Divider()
                            InfoRow(label: String(localized: "Server"), value: detail.serverName)
                            Divider()
                            InfoRow(label: String(localized: "Trigger Count"), value: "\(detail.triggerCount)")
                            Divider()
                            InfoRow(
                                label: String(localized: "First Triggered"),
                                value: Formatters.formatRelativeTime(detail.firstTriggeredAt)
                            )
                            if let resolvedAt = detail.resolvedAt {
                                Divider()
                                InfoRow(
                                    label: String(localized: "Resolved At"),
                                    value: Formatters.formatRelativeTime(resolvedAt)
                                )
                            }
                            if !detail.message.isEmpty {
                                Divider()
                                InfoRow(label: String(localized: "Message"), value: detail.message)
                            }
                            Divider()
                            InfoRow(label: String(localized: "Trigger Mode"), value: detail.ruleTriggerMode)
                            Divider()
                            InfoRow(
                                label: String(localized: "Rule Enabled"),
                                value: detail.ruleEnabled ? String(localized: "Yes") : String(localized: "No")
                            )
                        }
                        .background(Color(.systemBackground))
                        .clipShape(RoundedRectangle(cornerRadius: 12))
                        .shadow(color: .black.opacity(0.05), radius: 2, y: 1)

                        NavigationLink(value: detail.serverId) {
                            HStack {
                                Image(systemName: "server.rack")
                                Text(String(localized: "View Server"))
                            }
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 14)
                            .background(Color.accentColor)
                            .foregroundStyle(.white)
                            .clipShape(RoundedRectangle(cornerRadius: 12))
                        }
                    }
                    .padding()
                }
            }
        }
        .navigationTitle(String(localized: "Alert Detail"))
        .navigationBarTitleDisplayMode(.inline)
        .task {
            if let apiClient {
                await viewModel.fetchDetail(alertKey: alertKey, apiClient: apiClient)
            }
        }
    }
}

private struct InfoRow: View {
    let label: String
    let value: String

    var body: some View {
        HStack {
            Text(label)
                .foregroundStyle(.secondary)
            Spacer()
            Text(value)
                .multilineTextAlignment(.trailing)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
    }
}
