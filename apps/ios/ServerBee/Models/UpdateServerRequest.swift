import Foundation

/// A tri-state for a nullable server field (`Option<Option<T>>` on the server):
/// `.unchanged` omits the key, `.clear` sends JSON null, `.set` sends the value.
/// Needed because Swift's synthesized encoder omits nil Optionals (so it can't
/// distinguish "leave alone" from "clear").
enum Tri<Value: Encodable & Sendable>: Sendable {
    case unchanged
    case clear
    case set(Value)

    func encode(
        into container: inout KeyedEncodingContainer<UpdateServerRequest.CodingKeys>,
        forKey key: UpdateServerRequest.CodingKeys
    ) throws {
        switch self {
        case .unchanged: break
        case .clear: try container.encodeNil(forKey: key)
        case let .set(value): try container.encode(value, forKey: key)
        }
    }
}

/// Body for `PUT /api/servers/{id}` (UpdateServerInput). Every field is optional:
/// omit to leave unchanged. Plain fields use `encodeIfPresent`; nullable fields
/// use `Tri` so they can be cleared (remark/public_remark cannot be NULL-cleared
/// server-side — send "" to blank). `capabilities` is intentionally omitted.
struct UpdateServerRequest: Encodable, Sendable {
    var name: String?
    var weight: Int?
    var hidden: Bool?
    var remark: String?
    var publicRemark: String?
    var groupId: Tri<String> = .unchanged
    var price: Tri<Double> = .unchanged
    var billingCycle: Tri<String> = .unchanged
    var currency: Tri<String> = .unchanged
    var expiredAt: Tri<String> = .unchanged
    var trafficLimit: Tri<Int64> = .unchanged
    var trafficLimitType: Tri<String> = .unchanged
    var billingStartDay: Tri<Int> = .unchanged

    enum CodingKeys: String, CodingKey {
        case name, weight, hidden, remark, currency, price
        case publicRemark = "public_remark"
        case groupId = "group_id"
        case billingCycle = "billing_cycle"
        case expiredAt = "expired_at"
        case trafficLimit = "traffic_limit"
        case trafficLimitType = "traffic_limit_type"
        case billingStartDay = "billing_start_day"
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encodeIfPresent(name, forKey: .name)
        try container.encodeIfPresent(weight, forKey: .weight)
        try container.encodeIfPresent(hidden, forKey: .hidden)
        try container.encodeIfPresent(remark, forKey: .remark)
        try container.encodeIfPresent(publicRemark, forKey: .publicRemark)
        try groupId.encode(into: &container, forKey: .groupId)
        try price.encode(into: &container, forKey: .price)
        try billingCycle.encode(into: &container, forKey: .billingCycle)
        try currency.encode(into: &container, forKey: .currency)
        try expiredAt.encode(into: &container, forKey: .expiredAt)
        try trafficLimit.encode(into: &container, forKey: .trafficLimit)
        try trafficLimitType.encode(into: &container, forKey: .trafficLimitType)
        try billingStartDay.encode(into: &container, forKey: .billingStartDay)
    }
}

/// Body for `PUT /api/servers/{id}/tags` (full-replace; server normalizes).
struct SetTagsRequest: Encodable, Sendable {
    let tags: [String]
}
