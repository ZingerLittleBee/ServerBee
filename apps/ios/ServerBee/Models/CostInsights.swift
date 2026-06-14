import Foundation

/// Cost / value analysis for one server (`GET /api/servers/{id}/cost-insights`).
///
/// When `configured` is false, the burn / value fields are all null and
/// `invalidReason` explains why (missing price, missing/invalid cycle, etc.).
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
    var valueScore: ValueScore?

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
        case valueScore = "value_score"
    }

    var currencyCode: String { currency ?? "USD" }
}

/// Why a server's cost could not be analysed.
enum CostInvalidReason: String, Decodable, Sendable {
    case missingPrice = "missing_price"
    case missingBillingCycle = "missing_billing_cycle"
    case invalidBillingCycle = "invalid_billing_cycle"
    case invalidPrice = "invalid_price"

    var label: String {
        switch self {
        case .missingPrice: String(localized: "No price set")
        case .missingBillingCycle: String(localized: "No billing cycle set")
        case .invalidBillingCycle: String(localized: "Invalid billing cycle")
        case .invalidPrice: String(localized: "Invalid price")
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

/// Composite value-for-money score (0-100) with grade and explanations.
struct ValueScore: Decodable, Sendable {
    let score: Double
    let grade: ValueGrade
    let reasons: [ValueReason]
    let confidence: ValueConfidence

    enum CodingKeys: String, CodingKey {
        case score, grade, reasons, confidence
    }
}

enum ValueGrade: String, Decodable, Sendable {
    case excellent, good, okay, poor, waste

    var label: String {
        switch self {
        case .excellent: String(localized: "Excellent")
        case .good: String(localized: "Good")
        case .okay: String(localized: "Okay")
        case .poor: String(localized: "Poor")
        case .waste: String(localized: "Waste")
        }
    }
}

enum ValueConfidence: String, Decodable, Sendable {
    case high, medium, low

    var label: String {
        switch self {
        case .high: String(localized: "High confidence")
        case .medium: String(localized: "Medium confidence")
        case .low: String(localized: "Low confidence")
        }
    }
}

/// A single prioritised explanation for a value score. Decoded leniently so an
/// unknown server-side variant degrades to `.unknown` rather than failing the
/// whole response.
enum ValueReason: String, Decodable, Sendable {
    case idleBurn = "idle_burn"
    case sleepingMoney = "sleeping_money"
    case goodMemoryValue = "good_memory_value"
    case goodDiskValue = "good_disk_value"
    case expensiveCpu = "expensive_cpu"
    case healthyUptime = "healthy_uptime"
    case lowUptime = "low_uptime"
    case expiredBilling = "expired_billing"
    case noPriceCycle = "no_price_cycle"
    case insufficientData = "insufficient_data"
    case freeOrZeroPrice = "free_or_zero_price"
    case unknown

    init(from decoder: Decoder) throws {
        let raw = try decoder.singleValueContainer().decode(String.self)
        self = ValueReason(rawValue: raw) ?? .unknown
    }

    var label: String {
        switch self {
        case .idleBurn: String(localized: "Paying for idle capacity")
        case .sleepingMoney: String(localized: "Mostly offline — money asleep")
        case .goodMemoryValue: String(localized: "Good memory value")
        case .goodDiskValue: String(localized: "Good disk value")
        case .expensiveCpu: String(localized: "Expensive per CPU core")
        case .healthyUptime: String(localized: "Healthy uptime")
        case .lowUptime: String(localized: "Low uptime")
        case .expiredBilling: String(localized: "Billing expired")
        case .noPriceCycle: String(localized: "No price or cycle")
        case .insufficientData: String(localized: "Not enough data yet")
        case .freeOrZeroPrice: String(localized: "Free / zero price")
        case .unknown: String(localized: "—")
        }
    }
}
