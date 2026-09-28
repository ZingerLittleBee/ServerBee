import XCTest
@testable import ServerBee

final class IPMaskTests: XCTestCase {
    func test_ipv4_masksTheLastTwoOctets() {
        XCTAssertEqual(IPMask.mask("203.0.113.7"), "203.0.*.*")
    }

    func test_ipv4_keepsPortsPrefixesAndSurroundingText() {
        XCTAssertEqual(IPMask.mask("1.2.3.0/24"), "1.2.*.*/24")
        XCTAssertEqual(IPMask.mask("0.0.0.0:8080→80/tcp"), "0.0.*.*:8080→80/tcp")
        XCTAssertEqual(IPMask.mask("https://10.1.2.3:8443/health"), "https://10.1.*.*:8443/health")
        XCTAssertEqual(IPMask.mask("45.9.8.7 · root"), "45.9.*.* · root")
        XCTAssertEqual(IPMask.mask("ICMP · 1.1.1.1, TCP · 8.8.8.8"), "ICMP · 1.1.*.*, TCP · 8.8.*.*")
    }

    func test_ipv4_leavesNonAddressesAlone() {
        XCTAssertEqual(IPMask.mask("v1.2.3.4"), "v1.2.3.4")
        XCTAssertEqual(IPMask.mask("1.2.3.4.5"), "1.2.3.4.5")
        XCTAssertEqual(IPMask.mask("300.1.2.3"), "300.1.2.3")
        XCTAssertEqual(IPMask.mask("example.com"), "example.com")
        XCTAssertEqual(IPMask.mask("Ubuntu 24.04"), "Ubuntu 24.04")
    }

    func test_ipv6_masksTheLastTwoGroups() {
        XCTAssertEqual(IPMask.mask("2001:db8:1:2:3:4:5:6"), "2001:db8:1:2:3:4:*:*")
        XCTAssertEqual(IPMask.mask("2001:db8::1"), "2001:db8::*:*")
        XCTAssertEqual(IPMask.mask("2001:DB8:0:0:1::"), "2001:db8::1:0:*:*")
        XCTAssertEqual(IPMask.mask("::1"), "::*:*")
        XCTAssertEqual(IPMask.mask("fe80::1%en0"), "fe80::*:*%en0")
    }

    func test_ipv6_keepsBracketsPortsAndPrefixes() {
        XCTAssertEqual(IPMask.mask("[2001:db8::a:b]:443"), "[2001:db8::*:*]:443")
        XCTAssertEqual(IPMask.mask("2001:db8:1:2::/64"), "2001:db8:1:2::*:*/64")
    }

    func test_ipv6_leavesTimesAndMacAddressesAlone() {
        XCTAssertEqual(IPMask.mask("12:30:45"), "12:30:45")
        XCTAssertEqual(IPMask.mask("2026-09-29T01:40:17Z"), "2026-09-29T01:40:17Z")
        XCTAssertEqual(IPMask.mask("aa:bb:cc:dd:ee:ff"), "aa:bb:cc:dd:ee:ff")
    }

    func test_ipv4MappedIPv6_masksTheEmbeddedIPv4() {
        XCTAssertEqual(IPMask.mask("::ffff:192.0.2.1"), "::ffff:192.0.*.*")
    }

    func test_maskingIPs_isANoOpWhenDisabled() {
        XCTAssertEqual("203.0.113.7".maskingIPs(false), "203.0.113.7")
        XCTAssertEqual("203.0.113.7".maskingIPs(true), "203.0.*.*")
    }
}
