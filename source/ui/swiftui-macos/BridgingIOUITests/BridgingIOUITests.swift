import XCTest

final class BridgingIOUITests: XCTestCase {

    override func setUpWithError() throws {
        // Put setup code here. This method is called before the invocation of each test method in the class.

        // In UI tests it is usually best to stop immediately when a failure occurs.
        continueAfterFailure = false

        // In UI tests it’s important to set the initial state - such as interface orientation - required for your tests before they run. The setUp method is a good place to do this.
    }

    @MainActor
    func testTargetSelectionAndSessionSummary() throws {
        let app = XCUIApplication()
        app.launch()

        let targetRow = app.buttons["target-row-ops-prod"]
        XCTAssertTrue(targetRow.waitForExistence(timeout: 2))
        targetRow.click()

        XCTAssertTrue(app.staticTexts["center-session-summary"].waitForExistence(timeout: 2))
        XCTAssertTrue(app.scrollViews["timeline-scroll"].exists)
    }

    @MainActor
    func testCreateTargetProfileFlow() throws {
        let app = XCUIApplication()
        app.launch()

        app.buttons["toolbar-new-target"].click()
        XCTAssertTrue(app.textFields["field-target-name"].waitForExistence(timeout: 2))

        let nameField = app.textFields["field-target-name"]
        nameField.click()
        nameField.typeText("ci-host")

        let aliasField = app.textFields["field-target-alias"]
        aliasField.click()
        aliasField.typeText("ci")

        let hostField = app.textFields["field-ssh-host"]
        hostField.click()
        hostField.typeText("10.8.0.22")

        let userField = app.textFields["field-ssh-username"]
        userField.click()
        userField.typeText("runner")

        app.buttons["target-sheet-save"].click()

        XCTAssertTrue(app.buttons["target-row-ci-host"].waitForExistence(timeout: 2))
    }

    @MainActor
    func testApprovalAndArtifactHashLookup() throws {
        let app = XCUIApplication()
        app.launch()

        let approveButton = app.buttons["approval-approve"]
        if approveButton.waitForExistence(timeout: 2) {
            approveButton.click()
        }

        let hashInput = app.textFields["artifact-hash-input"]
        XCTAssertTrue(hashInput.waitForExistence(timeout: 2))
        app.buttons["artifact-hash-lookup"].click()

        XCTAssertTrue(app.staticTexts["artifact-hash-result"].waitForExistence(timeout: 2))
    }

    @MainActor
    func testLaunchPerformance() throws {
        // This measures how long it takes to launch your application.
        measure(metrics: [XCTApplicationLaunchMetric()]) {
            XCUIApplication().launch()
        }
    }

    @MainActor
    func testChineseLocaleLocalizationAndProductNameConsistency() throws {
        let app = XCUIApplication()
        app.launchArguments += ["-AppleLanguages", "(zh-Hans)", "-AppleLocale", "zh-Hans"]
        app.launch()

        XCTAssertTrue(app.staticTexts["Bridging IO"].waitForExistence(timeout: 2))
        XCTAssertTrue(app.buttons["设置"].waitForExistence(timeout: 2))
    }
}
