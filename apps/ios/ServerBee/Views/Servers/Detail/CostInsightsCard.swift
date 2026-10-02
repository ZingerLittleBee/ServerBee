import SwiftUI

/// Cost / value-for-money breakdown for a server. Renders a compact summary
/// when billing is unconfigured, or a full burn-rate + resource-value + grade
/// breakdown when configured.
struct CostInsightsCard: View {
    let cost: ServerCostInsights
    let config: ServerConfig?

    @ScaledMetric(relativeTo: .body) private var burnBarHeight: CGFloat = 6

    var body: some View {
        SectionCard(String(localized: "Cost")) {
            if cost.configured {
                configuredBody
            } else {
                unconfiguredBody
            }
        } accessory: {
            if cost.configured, let cycle = cost.billingCycle {
                Text(localizedCycle(cycle))
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

/// One label/value pair in the configured cost grid.
private struct CostStat: Hashable {
    let label: String
    let value: String
}

// MARK: Configured

private extension CostInsightsCard {
    var configuredBody: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(Formatters.formatCurrency(cost.price, code: cost.currencyCode))
                .font(.title.bold())
                .monospacedDigit()
                .lineLimit(1)
                .minimumScaleFactor(0.6)
            if let elapsed = cost.cycleCostElapsed {
                burnSection(elapsed)
            }
            statsGrid
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

    func burnSection(_ elapsed: Double) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            ViewThatFits(in: .horizontal) {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    burnLabel
                    Spacer(minLength: 8)
                    burnAmount(elapsed)
                }
                VStack(alignment: .leading, spacing: 2) {
                    burnLabel
                    burnAmount(elapsed)
                }
            }
            .accessibilityElement(children: .combine)
            if let percent = cost.cycleBurnPercent {
                UsageBar(value: percent / 100, height: burnBarHeight, tint: .accentColor)
                    .accessibilityLabel(Text(String(localized: "Burned this cycle")))
            }
        }
    }

    var burnLabel: some View {
        Text(String(localized: "Burned this cycle"))
            .font(.subheadline)
    }

    func burnAmount(_ elapsed: Double) -> some View {
        Text(burnedValue(elapsed))
            .font(.subheadline.weight(.semibold))
            .monospacedDigit()
            .lineLimit(1)
    }

    var stats: [CostStat] {
        var stats = [
            CostStat(
                label: String(localized: "Per day"),
                value: Formatters.formatCurrency(cost.costPerDay, code: cost.currencyCode)
            ),
            CostStat(
                label: String(localized: "Per hour"),
                value: Formatters.formatCurrencyRate(cost.costPerHour, code: cost.currencyCode)
            ),
            CostStat(
                label: String(localized: "Remaining budget"),
                value: Formatters.formatCurrency(cost.cycleCostRemaining, code: cost.currencyCode)
            )
        ]
        if let days = cost.daysRemaining {
            stats.append(CostStat(label: String(localized: "Days remaining"), value: days == 1 ? String(localized: "1 day") : String(localized: "\(days) days")))
        }
        return stats
    }

    var statsGrid: some View {
        let columns = [
            GridItem(.flexible(), spacing: 12, alignment: .topLeading),
            GridItem(.flexible(), spacing: 12, alignment: .topLeading)
        ]
        return LazyVGrid(columns: columns, alignment: .leading, spacing: 12) {
            ForEach(stats, id: \.label) { stat in
                TrafficStatCell(label: stat.label, value: stat.value, valueFont: .headline)
            }
        }
    }

    func burnedValue(_ elapsed: Double) -> String {
        let amount = Formatters.formatCurrency(elapsed, code: cost.currencyCode)
        if let percent = cost.cycleBurnPercent {
            return "\(amount) (\(String(format: "%.0f%%", percent)))"
        }
        return amount
    }

    func resourceRows(_ resource: ResourceValue) -> some View {
        VStack(spacing: 8) {
            Text(String(localized: "Value per resource (monthly)"))
                .font(.footnote.weight(.semibold))
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .accessibilityAddTraits(.isHeader)
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

    func advisoriesView(_ advisories: [CostAdvisory]) -> some View {
        FlexibleWrap(items: advisories) { advisory in
            Chip(text: advisory.label, systemImage: "exclamationmark.triangle.fill", color: .warningAmber)
        }
    }
}

// MARK: Unconfigured

private extension CostInsightsCard {
    var unconfiguredBody: some View {
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
            Label {
                Text(cost.invalidReason?.label ?? String(localized: "Set a price and billing cycle to see cost insights."))
            } icon: {
                Image(systemName: "info.circle")
            }
            .font(.subheadline)
            .foregroundStyle(.secondary)
        }
    }
}

// MARK: Helpers

private extension CostInsightsCard {
    func localizedCycle(_ cycle: String) -> String {
        switch cycle {
        case "monthly": String(localized: "Monthly")
        case "quarterly": String(localized: "Quarterly")
        case "yearly": String(localized: "Yearly")
        default: cycle.capitalized
        }
    }
}
