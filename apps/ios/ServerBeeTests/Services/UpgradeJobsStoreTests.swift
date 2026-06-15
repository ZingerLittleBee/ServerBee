import XCTest
@testable import ServerBee

@MainActor
final class UpgradeJobsStoreTests: XCTestCase {

    private func runningJob(server: String = "s1", job: String = "job-1", stage: UpgradeStage = .downloading) -> UpgradeJob {
        UpgradeJob(
            serverId: server,
            jobId: job,
            targetVersion: "1.9.0",
            stage: stage,
            status: .running,
            error: nil,
            backupPath: nil,
            startedAt: "2026-06-15T18:00:00Z",
            finishedAt: nil
        )
    }

    func test_setJobs_replacesMap() {
        let store = UpgradeJobsStore()
        store.setJobs([runningJob(server: "s1"), runningJob(server: "s2")])
        XCTAssertNotNil(store.job(forServer: "s1"))
        XCTAssertNotNil(store.job(forServer: "s2"))

        store.setJobs([runningJob(server: "s3")])
        XCTAssertNil(store.job(forServer: "s1"))
        XCTAssertNotNil(store.job(forServer: "s3"))
    }

    func test_applyProgress_noopsWhenNoExistingJob() {
        let store = UpgradeJobsStore()
        // Progress alone must never create a job (web parity).
        store.applyProgress(serverId: "s1", jobId: "job-1", targetVersion: "1.9.0", stage: .verifying)
        XCTAssertNil(store.job(forServer: "s1"))
    }

    func test_applyProgress_mergesStageOntoExistingJob() {
        let store = UpgradeJobsStore()
        store.setJobs([runningJob(stage: .downloading)])
        store.applyProgress(serverId: "s1", jobId: "job-1", targetVersion: "1.9.0", stage: .installing)

        let job = store.job(forServer: "s1")
        XCTAssertEqual(job?.stage, .installing)
        XCTAssertEqual(job?.status, .running)
        XCTAssertEqual(job?.startedAt, "2026-06-15T18:00:00Z")
    }

    func test_applyResult_fallsBackStageToExisting() {
        let store = UpgradeJobsStore()
        store.setJobs([runningJob(stage: .installing)])
        // Result omits stage → keep the existing stage.
        store.applyResult(
            serverId: "s1", jobId: "job-1", targetVersion: "1.9.0",
            status: .failed, stage: nil, error: "boom", backupPath: nil
        )
        let job = store.job(forServer: "s1")
        XCTAssertEqual(job?.status, .failed)
        XCTAssertEqual(job?.stage, .installing)
        XCTAssertEqual(job?.error, "boom")
        XCTAssertNotNil(job?.finishedAt)
    }

    func test_finishedJob_autoClears() async throws {
        let store = UpgradeJobsStore()
        store.setJobs([runningJob()])
        store.applyResult(
            serverId: "s1", jobId: "job-1", targetVersion: "1.9.0",
            status: .succeeded, stage: .restarting, error: nil, backupPath: nil
        )
        XCTAssertNotNil(store.job(forServer: "s1"))
        // Auto-clear fires after 5s; wait a touch longer.
        try await Task.sleep(for: .seconds(6))
        XCTAssertNil(store.job(forServer: "s1"))
    }

    func test_timeoutJob_doesNotAutoClear() async throws {
        let store = UpgradeJobsStore()
        store.setJobs([runningJob()])
        store.applyResult(
            serverId: "s1", jobId: "job-1", targetVersion: "1.9.0",
            status: .timeout, stage: .installing, error: nil, backupPath: "/tmp/b"
        )
        try await Task.sleep(for: .seconds(6))
        // Timeout is intentionally excluded from auto-clear.
        XCTAssertEqual(store.job(forServer: "s1")?.status, .timeout)
    }

    func test_autoClear_guardedByJobId() async throws {
        let store = UpgradeJobsStore()
        store.setJobs([runningJob(job: "old")])
        // Finish the old job (schedules a 5s clear keyed to "old")…
        store.applyResult(
            serverId: "s1", jobId: "old", targetVersion: "1.9.0",
            status: .succeeded, stage: .restarting, error: nil, backupPath: nil
        )
        // …then a newer running job replaces it before the timer fires.
        store.setJobs([runningJob(job: "new")])
        try await Task.sleep(for: .seconds(6))
        // The old job's timer must NOT wipe the newer job.
        XCTAssertEqual(store.job(forServer: "s1")?.jobId, "new")
    }
}
