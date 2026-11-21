import XCTest
@testable import ServerBee

final class CapabilityTests: XCTestCase {
    func test_resolved_prefersEffectiveMask() {
        let set = CapabilitySet(configured: 2047, agentLocal: 2047, effective: 1084)
        XCTAssertEqual(set.resolved, 1084)
        XCTAssertTrue(set.isEnabled(.ipQuality))
        XCTAssertFalse(set.isEnabled(.terminal))
    }

    func test_resolved_fallsBackToConfiguredWhenEffectiveNil() {
        // Mirrors getEffectiveCapabilityEnabled web behaviour: when the server
        // hasn't computed an effective mask, gate on the configured bits.
        let set = CapabilitySet(configured: 1084, agentLocal: nil, effective: nil)
        XCTAssertEqual(set.resolved, 1084)
        XCTAssertTrue(set.isEnabled(.upgrade))
        XCTAssertFalse(set.isEnabled(.docker))
    }

    func test_resolved_intersectsWhenOnlyAgentLocalKnown() {
        let set = CapabilitySet(configured: 2047, agentLocal: 1, effective: nil)
        XCTAssertEqual(set.resolved, 1)
        XCTAssertTrue(set.isEnabled(.terminal))
        XCTAssertFalse(set.isEnabled(.file))
    }

    func test_configuredButUnavailable_listsGap() {
        // Terminal + file configured, but agent only serves terminal.
        let set = CapabilitySet(configured: 65, agentLocal: 1, effective: 1)
        XCTAssertEqual(set.configuredButUnavailable, [.file])
    }

    func test_tailscale_bitAndMetadata() {
        XCTAssertEqual(Capability.tailscale.rawValue, 1 << 11)
        XCTAssertEqual(Capability.tailscale.rawValue, 2048)
        XCTAssertEqual(Capability.tailscale.key, "tailscale")
        XCTAssertTrue(Capability.tailscale.defaultEnabled)
        XCTAssertEqual(Capability.tailscale.riskLevel, "low")

        let set = CapabilitySet(configured: 3900, agentLocal: 3900, effective: 3900)
        XCTAssertTrue(set.isEnabled(.tailscale))
        XCTAssertTrue(set.isConfigured(.tailscale))
    }
}
