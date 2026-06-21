import Foundation

// MARK: - Fleet summary (client-side aggregation)

/// Cross-server roll-up computed purely from the live `ServerStatus` list the
/// app already holds — no extra endpoint required.
struct FleetSummary: Equatable, Sendable {
    let total: Int
    let online: Int
    let avgCpu: Double?
    let avgMemory: Double?
    let totalNetworkIn: Int64
    let totalNetworkOut: Int64
    let totalInTransfer: Int64
    let totalOutTransfer: Int64

    var offline: Int { total - online }

    static func from(_ servers: [ServerStatus]) -> FleetSummary {
        let onlineServers = servers.filter(\.isOnline)
        let cpuValues = onlineServers.compactMap(\.cpuUsage)
        let memValues = onlineServers.compactMap(\.memoryPercent)
        return FleetSummary(
            total: servers.count,
            online: onlineServers.count,
            avgCpu: cpuValues.isEmpty ? nil : cpuValues.reduce(0, +) / Double(cpuValues.count),
            avgMemory: memValues.isEmpty ? nil : memValues.reduce(0, +) / Double(memValues.count),
            totalNetworkIn: onlineServers.compactMap(\.networkIn).reduce(0, +),
            totalNetworkOut: onlineServers.compactMap(\.networkOut).reduce(0, +),
            totalInTransfer: onlineServers.compactMap(\.netInTransfer).reduce(0, +),
            totalOutTransfer: onlineServers.compactMap(\.netOutTransfer).reduce(0, +)
        )
    }
}

// MARK: - Cost overview (`GET /api/cost/overview`)

/// Fleet-wide cost aggregation, grouped by currency (costs of different
/// currencies are never summed together).
struct CostOverviewResponse: Decodable, Sendable {
    let currencies: [CurrencyCostSummary]
    let servers: [ServerCostOverview]
}

struct CurrencyCostSummary: Decodable, Identifiable, Sendable {
    let currency: String
    let configuredServerCount: Int
    let monthlyEquivalentTotal: Double
    let dailyTotal: Double
    let cycleElapsedTotal: Double

    var id: String { currency }

    enum CodingKeys: String, CodingKey {
        case currency
        case configuredServerCount = "configured_server_count"
        case monthlyEquivalentTotal = "monthly_equivalent_total"
        case dailyTotal = "daily_total"
        case cycleElapsedTotal = "cycle_elapsed_total"
    }
}

struct ServerCostOverview: Decodable, Identifiable, Sendable {
    let serverId: String
    let name: String
    let configured: Bool
    let invalidReason: CostInvalidReason?
    let currency: String?
    let billingCycle: String?
    let costPerDay: Double?
    let costPerMonthEquivalent: Double?
    let cycleBurnPercent: Double?
    let daysRemaining: Int?
    let advisories: [CostAdvisory]?

    var id: String { serverId }

    enum CodingKeys: String, CodingKey {
        case name, configured, currency, advisories
        case serverId = "server_id"
        case invalidReason = "invalid_reason"
        case billingCycle = "billing_cycle"
        case costPerDay = "cost_per_day"
        case costPerMonthEquivalent = "cost_per_month_equivalent"
        case cycleBurnPercent = "cycle_burn_percent"
        case daysRemaining = "days_remaining"
    }
}
