import Foundation

/// Cost analysis for one server (`GET /api/servers/{id}/cost-insights`).
///
/// When `configured` is false, the burn fields are all null and `invalidReason`
/// explains why (missing price, missing/invalid cycle, etc.). `advisories` lists
/// objective, per-server warnings (idle burn, offline while paying, etc.).
struct ServerCostInsights: Decodable, Sendable {
    let serverId: String
    let configured: Bool
    var invalidReason: CostInvalidReason?
    var price: Double?
    var currency: String?
    var billingCycle: String?
    var cycleStart: String?
    var cycleEnd: String?
    var cycleDays: Int?
    var daysElapsed: Int?
    var daysRemaining: Int?
    var costPerSecond: Double?
    var costPerHour: Double?
    var costPerDay: Double?
    var costPerMonthEquivalent: Double?
    var cycleCostElapsed: Double?
    var cycleCostRemaining: Double?
    var cycleBurnPercent: Double?
    var resourceValue: ResourceValue?
    var advisories: [CostAdvisory]?

    enum CodingKeys: String, CodingKey {
        case serverId = "server_id"
        case configured
        case invalidReason = "invalid_reason"
        case price, currency
        case billingCycle = "billing_cycle"
        case cycleStart = "cycle_start"
        case cycleEnd = "cycle_end"
        case cycleDays = "cycle_days"
        case daysElapsed = "days_elapsed"
        case daysRemaining = "days_remaining"
        case costPerSecond = "cost_per_second"
        case costPerHour = "cost_per_hour"
        case costPerDay = "cost_per_day"
        case costPerMonthEquivalent = "cost_per_month_equivalent"
        case cycleCostElapsed = "cycle_cost_elapsed"
        case cycleCostRemaining = "cycle_cost_remaining"
        case cycleBurnPercent = "cycle_burn_percent"
        case resourceValue = "resource_value"
        case advisories
    }

    var currencyCode: String { currency ?? "USD" }
}

/// Why a server's cost could not be analysed. Decoded leniently so an unknown
/// server-side variant degrades to `.unknown` rather than failing the whole
/// cost response.
enum CostInvalidReason: String, Decodable, Sendable {
    case missingPrice = "missing_price"
    case missingBillingCycle = "missing_billing_cycle"
    case invalidBillingCycle = "invalid_billing_cycle"
    case invalidPrice = "invalid_price"
    case unknown

    init(from decoder: Decoder) throws {
        let raw = try decoder.singleValueContainer().decode(String.self)
        self = CostInvalidReason(rawValue: raw) ?? .unknown
    }

    var label: String {
        switch self {
        case .missingPrice: String(localized: "No price set")
        case .missingBillingCycle: String(localized: "No billing cycle set")
        case .invalidBillingCycle: String(localized: "Invalid billing cycle")
        case .invalidPrice: String(localized: "Invalid price")
        case .unknown: String(localized: "Cost configuration issue")
        }
    }
}

/// Per-resource unit cost (monthly-normalised).
struct ResourceValue: Decodable, Sendable {
    var costPerCpuCore: Double?
    var costPerGbMemory: Double?
    var costPerGbDisk: Double?
    var costPerTbTrafficLimit: Double?
    var trafficLimitType: String?

    enum CodingKeys: String, CodingKey {
        case costPerCpuCore = "cost_per_cpu_core"
        case costPerGbMemory = "cost_per_gb_memory"
        case costPerGbDisk = "cost_per_gb_disk"
        case costPerTbTrafficLimit = "cost_per_tb_traffic_limit"
        case trafficLimitType = "traffic_limit_type"
    }
}

/// An objective, per-server cost advisory surfaced alongside the cost
/// breakdown. Decoded leniently so an unknown server-side variant degrades to
/// `.unknown` rather than failing the whole cost response.
enum CostAdvisory: String, Decodable, Sendable {
    case expiredBilling = "expired_billing"
    case sleepingMoney = "sleeping_money"
    case idleBurn = "idle_burn"
    case lowUptime = "low_uptime"
    case unknown

    init(from decoder: Decoder) throws {
        let raw = try decoder.singleValueContainer().decode(String.self)
        self = CostAdvisory(rawValue: raw) ?? .unknown
    }

    var label: String {
        switch self {
        case .expiredBilling: String(localized: "Billing expired")
        case .sleepingMoney: String(localized: "Offline & paying")
        case .idleBurn: String(localized: "Idle & paying")
        case .lowUptime: String(localized: "Low uptime")
        case .unknown: String(localized: "—")
        }
    }
}
