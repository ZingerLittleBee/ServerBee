import SwiftUI

/// Holds live agent upgrade jobs pushed over the WebSocket, keyed by server id.
/// Mirrors the web zustand `upgrade-jobs-store`: a full sync replaces the whole
/// map, progress frames merge onto an existing job (never create one), and a
/// result frame upserts the terminal state. Finished jobs (succeeded/failed,
/// but NOT timeout) auto-clear after a short delay, guarded by job id so a newer
/// upgrade isn't wiped by an older job's timer.
@MainActor
@Observable
final class UpgradeJobsStore {
    private(set) var jobs: [String: UpgradeJob] = [:]

    private static let autoClearDelay: Duration = .seconds(5)

    /// The live job for a server, if any.
    func job(forServer serverId: String) -> UpgradeJob? { jobs[serverId] }

    /// Replace the whole map from a full sync, then schedule auto-clear for any
    /// already-finished entries.
    func setJobs(_ list: [UpgradeJob]) {
        jobs = Dictionary(uniqueKeysWithValues: list.map { ($0.serverId, $0) })
        for job in list where job.status.isFinished {
            scheduleAutoClear(serverId: job.serverId, jobId: job.jobId)
        }
    }

    /// Merge a progress frame. Progress alone NEVER creates a job — a client that
    /// missed the full sync would otherwise render a partial job — so this no-ops
    /// when there is no existing entry for the server.
    func applyProgress(serverId: String, jobId: String, targetVersion: String, stage: UpgradeStage) {
        guard let existing = jobs[serverId] else { return }
        jobs[serverId] = UpgradeJob(
            serverId: serverId,
            jobId: jobId,
            targetVersion: targetVersion,
            stage: stage,
            status: .running,
            error: existing.error,
            backupPath: existing.backupPath,
            startedAt: existing.startedAt,
            finishedAt: nil
        )
    }

    /// Upsert a result frame. `stage` falls back to the existing stage then
    /// `.downloading` (parity with the web), `startedAt` is preserved, and a
    /// finished status schedules auto-clear.
    func applyResult(
        serverId: String,
        jobId: String,
        targetVersion: String,
        status: UpgradeStatus,
        stage: UpgradeStage?,
        error: String?,
        backupPath: String?
    ) {
        let existing = jobs[serverId]
        let now = WireDate.string(from: Date())
        jobs[serverId] = UpgradeJob(
            serverId: serverId,
            jobId: jobId,
            targetVersion: targetVersion,
            stage: stage ?? existing?.stage ?? .downloading,
            status: status,
            error: error,
            backupPath: backupPath,
            startedAt: existing?.startedAt ?? now,
            finishedAt: now
        )
        if status.isFinished {
            scheduleAutoClear(serverId: serverId, jobId: jobId)
        }
    }

    /// Clear a finished job after the delay, but only if it's still the same job
    /// (a newer upgrade for the same server must survive).
    private func scheduleAutoClear(serverId: String, jobId: String) {
        Task { [weak self] in
            try? await Task.sleep(for: Self.autoClearDelay)
            guard let self, self.jobs[serverId]?.jobId == jobId else { return }
            self.jobs[serverId] = nil
        }
    }
}
