import XCTest
@testable import ServerBee

final class AppLanguageStoreTests: XCTestCase {
    private let suite = "AppLanguageStoreTests"
    private var defaults: UserDefaults!

    override func setUp() {
        super.setUp()
        UserDefaults().removePersistentDomain(forName: suite)
        defaults = UserDefaults(suiteName: suite)
    }

    override func tearDown() {
        UserDefaults().removePersistentDomain(forName: suite)
        super.tearDown()
    }

    private var store: AppLanguageStore { AppLanguageStore(defaults: defaults, domain: suite) }

    func test_noOverride_followsTheSystem() {
        XCTAssertEqual(store.selected, .system)
    }

    func test_selectingALanguage_storesItAsTheOnlyPreference() {
        store.select(.simplifiedChinese)
        XCTAssertEqual(store.selected, .simplifiedChinese)
        XCTAssertEqual(defaults.persistentDomain(forName: suite)?[AppLanguageStore.key] as? [String], ["zh-Hans"])
    }

    func test_selectingSystem_removesTheOverride() {
        store.select(.english)
        store.select(.system)
        XCTAssertEqual(store.selected, .system)
        XCTAssertNil(defaults.persistentDomain(forName: suite)?[AppLanguageStore.key])
    }

    /// iOS Settings writes region-qualified codes such as "zh-Hans-CN".
    func test_regionQualifiedOverride_matchesItsLanguage() {
        defaults.set(["zh-Hans-CN"], forKey: AppLanguageStore.key)
        XCTAssertEqual(store.selected, .simplifiedChinese)
        defaults.set(["en-GB"], forKey: AppLanguageStore.key)
        XCTAssertEqual(store.selected, .english)
    }

    func test_unsupportedOverride_readsAsSystem() {
        defaults.set(["fr"], forKey: AppLanguageStore.key)
        XCTAssertEqual(store.selected, .system)
    }
}
