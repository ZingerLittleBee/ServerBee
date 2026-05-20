import UIKit
import XCTest
@testable import ServerBee

@MainActor
final class DeviceNameProviderTests: XCTestCase {
    private let suiteName = "DeviceNameProviderTests"
    private var defaults: UserDefaults {
        UserDefaults(suiteName: suiteName) ?? .standard
    }

    override func setUp() {
        super.setUp()
        UserDefaults().removePersistentDomain(forName: suiteName)
    }

    override func tearDown() {
        UserDefaults().removePersistentDomain(forName: suiteName)
        super.tearDown()
    }

    func test_defaultName_isNonEmpty_andContainsFourCharSuffix() {
        let name = DeviceNameProvider.defaultName(defaults: defaults)
        XCTAssertFalse(name.isEmpty)
        // Suffix is wrapped in parentheses at the end: "Model 17.0 (AB12)".
        guard let open = name.lastIndex(of: "("),
              let close = name.lastIndex(of: ")"),
              open < close
        else {
            XCTFail("Default name missing (suffix): \(name)")
            return
        }
        let suffix = name[name.index(after: open) ..< close]
        XCTAssertEqual(suffix.count, 4)
    }

    func test_suffix_isStableAcrossCalls() {
        let first = DeviceNameProvider.defaultName(defaults: defaults)
        let second = DeviceNameProvider.defaultName(defaults: defaults)
        XCTAssertEqual(first, second)
    }

    func test_set_persistsCustomName() {
        DeviceNameProvider.set("My iPhone", defaults: defaults)
        XCTAssertEqual(DeviceNameProvider.current(defaults: defaults), "My iPhone")
    }

    func test_set_emptyString_fallsBackToDefault() {
        DeviceNameProvider.set("My iPhone", defaults: defaults)
        DeviceNameProvider.set("   ", defaults: defaults)
        let value = DeviceNameProvider.current(defaults: defaults)
        XCTAssertTrue(value.contains(UIDevice.current.model))
    }

    func test_current_returnsDefaultWhenUnset() {
        let value = DeviceNameProvider.current(defaults: defaults)
        XCTAssertEqual(value, DeviceNameProvider.defaultName(defaults: defaults))
    }
}
