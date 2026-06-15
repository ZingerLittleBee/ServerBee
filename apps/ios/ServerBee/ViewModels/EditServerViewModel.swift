import SwiftUI

/// Backs the admin "Edit server" form: prefills from `ServerConfig`, loads the
/// group list + current tags, and PUTs `/api/servers/{id}` (+ `/tags`).
@MainActor
@Observable
final class EditServerViewModel {
    // Basic
    var name = ""
    var groupId = ""          // "" = no group
    var tagsText = ""
    var remark = ""
    var publicRemark = ""
    var hidden = false
    var weight = 0
    // Billing
    var priceText = ""
    var currency = ""         // "" = none
    var billingCycle = ""     // "" = none
    var billingStartDayText = ""
    var hasExpiry = false
    var expiryDate = Date()
    var trafficLimitGiBText = ""
    var trafficLimitType = "" // "" = none

    var groups: [ServerGroup] = []
    var isSaving = false
    var errorMessage: String?
    private var originalTags: [String] = []

    static let gibibyte = 1_073_741_824.0

    func prefill(from config: ServerConfig) {
        name = config.name
        groupId = config.groupId ?? ""
        remark = config.remark ?? ""
        publicRemark = config.publicRemark ?? ""
        hidden = config.hidden ?? false
        weight = config.weight ?? 0
        priceText = config.price.map { Self.trim(String(format: "%.2f", $0)) } ?? ""
        currency = config.currency ?? ""
        billingCycle = config.billingCycle ?? ""
        billingStartDayText = config.billingStartDay.map(String.init) ?? ""
        if let expiry = config.expiredDate {
            hasExpiry = true
            expiryDate = expiry
        }
        if let limit = config.trafficLimit {
            trafficLimitGiBText = Self.trim(String(format: "%.2f", Double(limit) / Self.gibibyte))
        }
        trafficLimitType = config.trafficLimitType ?? ""
    }

    func loadGroups(apiClient: APIClient) async {
        groups = (try? await apiClient.get("/api/server-groups")) ?? []
    }

    func loadTags(serverId: String, apiClient: APIClient) async {
        if let tags: [String] = try? await apiClient.get("/api/servers/\(serverId)/tags") {
            originalTags = tags
            tagsText = tags.joined(separator: ", ")
        }
    }

    /// Trims a trailing ".00" / ".0" from a formatted number for nicer prefill.
    private static func trim(_ string: String) -> String {
        var result = string
        if result.contains(".") {
            while result.hasSuffix("0") { result.removeLast() }
            if result.hasSuffix(".") { result.removeLast() }
        }
        return result
    }
}

private extension EditServerViewModel {
    /// Parse the comma/space-separated tag text, applying the server's rules so
    /// the user gets immediate feedback. Returns nil + sets errorMessage on a
    /// validation failure.
    func parsedTags() -> [String]? {
        let raw = tagsText
            .split(whereSeparator: { $0 == "," || $0.isWhitespace })
            .map { String($0) }
        var seen: Set<String> = []
        var result: [String] = []
        for tag in raw where !seen.contains(tag) {
            guard tag.count <= 16,
                  tag.allSatisfy({ $0.isLetter || $0.isNumber || "_-.".contains($0) }) else {
                errorMessage = String(format: String(localized: "Invalid tag: %@"), tag)
                return nil
            }
            seen.insert(tag)
            result.append(tag)
        }
        guard result.count <= 8 else {
            errorMessage = String(localized: "At most 8 tags are allowed.")
            return nil
        }
        return result.sorted()
    }

    func buildRequest() -> UpdateServerRequest {
        var request = UpdateServerRequest()
        request.name = name.trimmingCharacters(in: .whitespaces)
        request.weight = weight
        request.hidden = hidden
        request.remark = remark
        request.publicRemark = publicRemark
        request.groupId = groupId.isEmpty ? .clear : .set(groupId)
        request.currency = currency.isEmpty ? .clear : .set(currency)
        request.billingCycle = billingCycle.isEmpty ? .clear : .set(billingCycle)
        request.trafficLimitType = trafficLimitType.isEmpty ? .clear : .set(trafficLimitType)
        request.price = tri(from: priceText, Double.init)
        request.billingStartDay = tri(from: billingStartDayText, Int.init)
        request.expiredAt = hasExpiry ? .set(WireDate.string(from: expiryDate)) : .clear
        if trafficLimitGiBText.trimmingCharacters(in: .whitespaces).isEmpty {
            request.trafficLimit = .clear
        } else if let gib = Double(trafficLimitGiBText) {
            request.trafficLimit = .set(Int64((gib * Self.gibibyte).rounded()))
        }
        return request
    }

    /// Empty text => clear; parseable => set; unparseable => leave unchanged.
    func tri<T>(from text: String, _ parse: (String) -> T?) -> Tri<T> {
        let trimmed = text.trimmingCharacters(in: .whitespaces)
        if trimmed.isEmpty { return .clear }
        if let value = parse(trimmed) { return .set(value) }
        return .unchanged
    }
}

extension EditServerViewModel {
    /// PUT the server config and (if changed) the tag set. Returns true on success.
    func save(serverId: String, apiClient: APIClient) async -> Bool {
        let trimmedName = name.trimmingCharacters(in: .whitespaces)
        guard !trimmedName.isEmpty else {
            errorMessage = String(localized: "Name is required.")
            return false
        }
        errorMessage = nil
        guard let tags = parsedTags() else { return false }

        isSaving = true
        defer { isSaving = false }
        do {
            let _: ServerConfig = try await apiClient.put("/api/servers/\(serverId)", body: buildRequest())
            if tags != originalTags {
                let _: [String] = try await apiClient.put("/api/servers/\(serverId)/tags", body: SetTagsRequest(tags: tags))
            }
            return true
        } catch {
            errorMessage = AccountSecurityViewModel.message(
                for: error, fallback: String(localized: "Couldn't save changes.")
            )
            return false
        }
    }
}
