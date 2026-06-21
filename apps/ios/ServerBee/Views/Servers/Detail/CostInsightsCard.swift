import SwiftUI

/// Cost / value-for-money breakdown for a server. Renders a compact summary
/// when billing is unconfigured, or a full burn-rate + resource-value + grade
/// breakdown when configured.
struct CostInsightsCard: View {
    let cost: ServerCostInsights
    let config: ServerConfig?

    var body: some View {
        SectionCard(String(localized: "Cost"), systemImage: "dollarsign.circle") {
            if cost.configured {
                configuredBody
            } else {
                unconfiguredBody
            }
        }
    }

    // MARK: Configured

    private var configuredBody: some View {
        VStack(alignment: .leading, spacing: 14) {
            header
            Divider()
            burnRows
            if let resource = cost.resourceValue {
                Divider()
                resourceRows(resource)
            }
            let advisories = (cost.advisories ?? []).filter { $0 != .unknown }
            if !advisories.isEmpty {
                Divider()
                advisoriesView(advisories)
            }
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(Formatters.formatCurrency(cost.price, code: cost.currencyCode))
                .font(.title3.bold())
            if let cycle = cost.billingCycle {
                Text(localizedCycle(cycle))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var burnRows: some View {
        VStack(spacing: 8) {
            DetailRow(
                label: String(localized: "Per day"),
                value: Formatters.formatCurrency(cost.costPerDay, code: cost.currencyCode)
            )
            DetailRow(
                label: String(localized: "Per hour"),
                value: Formatters.formatCurrencyRate(cost.costPerHour, code: cost.currencyCode)
            )
            if let elapsed = cost.cycleCostElapsed {
                DetailRow(
                    label: String(localized: "Burned this cycle"),
                    value: burnedValue(elapsed)
                )
            }
            DetailRow(
                label: String(localized: "Remaining budget"),
                value: Formatters.formatCurrency(cost.cycleCostRemaining, code: cost.currencyCode)
            )
            if let days = cost.daysRemaining {
                DetailRow(
                    label: String(localized: "Days remaining"),
                    value: String(localized: "\(days) days")
                )
            }
        }
    }

    private func burnedValue(_ elapsed: Double) -> String {
        let amount = Formatters.formatCurrency(elapsed, code: cost.currencyCode)
        if let percent = cost.cycleBurnPercent {
            return "\(amount) (\(String(format: "%.0f%%", percent)))"
        }
        return amount
    }

    private func resourceRows(_ resource: ResourceValue) -> some View {
        VStack(spacing: 8) {
            Text(String(localized: "Value per resource (monthly)"))
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            if let cpu = resource.costPerCpuCore {
                DetailRow(
                    label: String(localized: "Per CPU core"),
                    value: Formatters.formatCurrency(cpu, code: cost.currencyCode)
                )
            }
            if let mem = resource.costPerGbMemory {
                DetailRow(
                    label: String(localized: "Per GB memory"),
                    value: Formatters.formatCurrency(mem, code: cost.currencyCode)
                )
            }
            if let disk = resource.costPerGbDisk {
                DetailRow(
                    label: String(localized: "Per GB disk"),
                    value: Formatters.formatCurrency(disk, code: cost.currencyCode)
                )
            }
            if let traffic = resource.costPerTbTrafficLimit {
                DetailRow(
                    label: String(localized: "Per TB traffic"),
                    value: Formatters.formatCurrency(traffic, code: cost.currencyCode)
                )
            }
        }
    }

    private func advisoriesView(_ advisories: [CostAdvisory]) -> some View {
        FlowChips(items: advisories) { advisory in
            Chip(text: advisory.label, color: .warningAmber)
        }
    }

    // MARK: Unconfigured

    private var unconfiguredBody: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let price = config?.price ?? cost.price {
                DetailRow(
                    label: String(localized: "Price"),
                    value: Formatters.formatCurrency(price, code: cost.currencyCode)
                )
            }
            if let cycle = config?.billingCycle ?? cost.billingCycle {
                DetailRow(label: String(localized: "Billing cycle"), value: localizedCycle(cycle))
            }
            if let reason = cost.invalidReason {
                HStack(spacing: 8) {
                    Image(systemName: "info.circle")
                        .foregroundStyle(.secondary)
                    Text(reason.label)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
            } else {
                Text(String(localized: "Set a price and billing cycle to see cost insights."))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
        }
    }

    // MARK: Helpers

    private func localizedCycle(_ cycle: String) -> String {
        switch cycle {
        case "monthly": String(localized: "Monthly")
        case "quarterly": String(localized: "Quarterly")
        case "yearly": String(localized: "Yearly")
        default: cycle.capitalized
        }
    }
}
