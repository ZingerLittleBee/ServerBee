import XCTest
@testable import ServerBee

/// Regression coverage for the REST-config ⊕ WS-live merge. The browser WS
/// frame omits `ipv4`/`ipv6`/`capabilities`/billing; the REST list omits live
/// metrics and `online`. Replacing wholesale (the old behaviour) made the IP
/// and capabilities vanish the instant the first WS frame arrived.
@MainActor
final class ServersViewModelMergeTests: XCTestCase {
    private func config(_ id: String) -> ServerStatus {
        var s = ServerStatus(
            id: id, name: "srv-\(id)", online: nil, cpuUsage: nil,
            memoryTotal: 2000, memoryUsed: nil, diskTotal: 10000, diskUsed: nil,
            networkIn: nil, networkOut: nil, load1: nil, load5: nil, load15: nil,
            processCount: nil, tcpCount: nil, udpCount: nil, uptime: nil,
            os: "Linux", cpuName: "CPU", ipv4: "1.2.3.4", ipv6: nil,
            region: nil, country: "JP", groupName: nil, lastActiveAt: nil
        )
        s.capabilities = 1084
        return s
    }

    private func liveFrame(_ id: String, online: Bool, cpu: Double) -> ServerStatus {
        // Simulates a WS frame: live metrics present, config fields absent.
        ServerStatus(id: id, name: "srv-\(id)", online: online, cpuUsage: cpu)
    }

    func test_fullSync_preservesRestConfigFields() {
        let vm = ServersViewModel()
        vm.applyConfig([config("1")])
        XCTAssertEqual(vm.servers.first?.ipv4, "1.2.3.4")

        vm.handleWSMessage(.fullSync(servers: [liveFrame("1", online: true, cpu: 42)], upgrades: []))

        let s = vm.servers.first
        XCTAssertEqual(s?.online, true, "live online applied")
        XCTAssertEqual(s?.cpuUsage, 42, "live cpu applied")
        XCTAssertEqual(s?.ipv4, "1.2.3.4", "REST ipv4 must survive a metrics-only frame")
        XCTAssertEqual(s?.capabilities, 1084, "REST capabilities must survive")
        XCTAssertEqual(s?.os, "Linux")
    }

    func test_configOverlay_doesNotEraseLiveMetrics_whenWSArrivesFirst() {
        let vm = ServersViewModel()
        // WS full_sync first (no config fields yet).
        vm.handleWSMessage(.fullSync(servers: [liveFrame("1", online: true, cpu: 42)], upgrades: []))
        // Then the REST fetch overlays config.
        vm.applyConfig([config("1")])

        let s = vm.servers.first
        XCTAssertEqual(s?.cpuUsage, 42, "live cpu must survive a later config overlay")
        XCTAssertEqual(s?.online, true)
        XCTAssertEqual(s?.ipv4, "1.2.3.4")
        XCTAssertEqual(s?.capabilities, 1084)
    }

    func test_fullSync_dropsServersNoLongerPresent() {
        let vm = ServersViewModel()
        vm.applyConfig([config("1"), config("2")])
        XCTAssertEqual(vm.servers.count, 2)

        vm.handleWSMessage(.fullSync(servers: [liveFrame("1", online: true, cpu: 1)], upgrades: []))
        XCTAssertEqual(vm.servers.map(\.id), ["1"], "server 2 removed by authoritative full_sync")
    }

    func test_capabilitiesChanged_updatesResolvedSet() {
        let vm = ServersViewModel()
        vm.applyConfig([config("1")])
        vm.handleWSMessage(.capabilitiesChanged(serverId: "1", capabilities: 2047, agentLocal: 2047, effective: 2047))

        let s = vm.servers.first
        XCTAssertEqual(s?.capabilities, 2047)
        XCTAssertEqual(s?.effectiveCapabilities, 2047)
        XCTAssertTrue(s?.capabilitySet.isEnabled(.terminal) ?? false)
    }

    func test_resolvedGroupName_mapsIdToName() {
        let vm = ServersViewModel()
        vm.groupsByID = ["g1": "Production"]
        var s = config("1")
        s.groupId = "g1"
        vm.applyConfig([s])
        XCTAssertEqual(vm.resolvedGroupName(for: vm.servers[0]), "Production")
    }
}
