import XCTest
@testable import ServerBee

final class ServerStatusMergeTests: XCTestCase {
    func test_merge_preservesOnline_whenIncomingOnlineIsNil() {
        var local = ServerStatus(
            id: "s1", name: "Local", online: true,
            cpuUsage: 10, memoryTotal: nil, memoryUsed: nil,
            diskTotal: nil, diskUsed: nil, networkIn: nil, networkOut: nil,
            load1: nil, load5: nil, load15: nil,
            processCount: nil, tcpCount: nil, udpCount: nil,
            uptime: nil, os: nil, cpuName: nil, ipv4: nil, ipv6: nil,
            region: nil, country: nil, groupName: nil, lastActiveAt: nil
        )
        let partial = ServerStatus(
            id: "s1", name: "Local", online: nil,
            cpuUsage: 42, memoryTotal: nil, memoryUsed: nil,
            diskTotal: nil, diskUsed: nil, networkIn: nil, networkOut: nil,
            load1: nil, load5: nil, load15: nil,
            processCount: nil, tcpCount: nil, udpCount: nil,
            uptime: nil, os: nil, cpuName: nil, ipv4: nil, ipv6: nil,
            region: nil, country: nil, groupName: nil, lastActiveAt: nil
        )
        local.merge(from: partial)
        XCTAssertEqual(local.online, true, "merge with nil online must preserve local")
        XCTAssertEqual(local.cpuUsage, 42)
    }

    func test_merge_appliesOnline_whenIncomingProvidesIt() {
        var local = ServerStatus(
            id: "s1", name: "Local", online: true,
            cpuUsage: nil, memoryTotal: nil, memoryUsed: nil,
            diskTotal: nil, diskUsed: nil, networkIn: nil, networkOut: nil,
            load1: nil, load5: nil, load15: nil,
            processCount: nil, tcpCount: nil, udpCount: nil,
            uptime: nil, os: nil, cpuName: nil, ipv4: nil, ipv6: nil,
            region: nil, country: nil, groupName: nil, lastActiveAt: nil
        )
        let incoming = ServerStatus(
            id: "s1", name: "Local", online: false,
            cpuUsage: nil, memoryTotal: nil, memoryUsed: nil,
            diskTotal: nil, diskUsed: nil, networkIn: nil, networkOut: nil,
            load1: nil, load5: nil, load15: nil,
            processCount: nil, tcpCount: nil, udpCount: nil,
            uptime: nil, os: nil, cpuName: nil, ipv4: nil, ipv6: nil,
            region: nil, country: nil, groupName: nil, lastActiveAt: nil
        )
        local.merge(from: incoming)
        XCTAssertEqual(local.online, false)
    }

    func test_merge_ignoresZeroCapacityFromRuntimeUpdate() {
        var local = ServerStatus(
            id: "s1", name: "Local", online: true,
            cpuUsage: nil, memoryTotal: 8_589_934_592, memoryUsed: 4_294_967_296,
            diskTotal: 21_474_836_480, diskUsed: 10_737_418_240, networkIn: nil, networkOut: nil,
            load1: nil, load5: nil, load15: nil,
            processCount: nil, tcpCount: nil, udpCount: nil,
            uptime: nil, os: nil, cpuName: nil, ipv4: nil, ipv6: nil,
            region: nil, country: nil, groupName: nil, lastActiveAt: nil
        )
        let incoming = ServerStatus(
            id: "s1", name: "Local", online: true,
            cpuUsage: nil, memoryTotal: 0, memoryUsed: 5_368_709_120,
            diskTotal: 0, diskUsed: 11_811_160_064, networkIn: nil, networkOut: nil,
            load1: nil, load5: nil, load15: nil,
            processCount: nil, tcpCount: nil, udpCount: nil,
            uptime: nil, os: nil, cpuName: nil, ipv4: nil, ipv6: nil,
            region: nil, country: nil, groupName: nil, lastActiveAt: nil
        )

        local.merge(from: incoming)

        XCTAssertEqual(local.memoryTotal, 8_589_934_592)
        XCTAssertEqual(local.memoryUsed, 5_368_709_120)
        XCTAssertEqual(local.diskTotal, 21_474_836_480)
        XCTAssertEqual(local.diskUsed, 11_811_160_064)
    }
}
