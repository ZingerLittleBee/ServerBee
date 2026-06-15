import SwiftUI

/// A single stage of an agent self-upgrade, mirroring the web `UpgradeStage`.
///
/// Raw values must match the wire snake_case exactly — the project decoders use
/// NO key strategy, so `preFlight` MUST spell `pre_flight` or decoding throws.
/// All five stages render in the stepper for visual parity with the web, even
/// though the current agent never emits a `pre_flight` progress frame.
enum UpgradeStage: String, Decodable, Sendable, CaseIterable {
    case downloading
    case verifying
    case preFlight = "pre_flight"
    case installing
    case restarting

    /// Position in the canonical stage order (drives the stepper progress fill).
    var order: Int { Self.allCases.firstIndex(of: self) ?? 0 }

    var label: String {
        switch self {
        case .downloading: String(localized: "Downloading")
        case .verifying: String(localized: "Verifying")
        case .preFlight: String(localized: "Pre-flight")
        case .installing: String(localized: "Installing")
        case .restarting: String(localized: "Restarting")
        }
    }

    var systemImage: String {
        switch self {
        case .downloading: "arrow.down.circle"
        case .verifying: "checkmark.shield"
        case .preFlight: "wrench.and.screwdriver"
        case .installing: "arrow.triangle.2.circlepath"
        case .restarting: "arrow.clockwise"
        }
    }
}

/// Terminal (or running) status of an upgrade job, mirroring the web
/// `UpgradeStatus`. Raw values already match the lowercase wire values.
enum UpgradeStatus: String, Decodable, Sendable {
    case running
    case succeeded
    case failed
    case timeout

    /// Whether the job should auto-clear after a short delay. Matches the web
    /// AUTO_CLEAR predicate — `timeout` is intentionally excluded so a timed-out
    /// upgrade stays visible until the next full sync.
    var isFinished: Bool { self == .succeeded || self == .failed }
}

/// A live agent upgrade job. Progress arrives only over the WebSocket; there is
/// no REST polling endpoint. `startedAt`/`finishedAt` stay as RFC3339 strings
/// (the plain coders apply no date strategy).
struct UpgradeJob: Decodable, Sendable, Identifiable {
    let serverId: String
    let jobId: String
    let targetVersion: String
    let stage: UpgradeStage
    let status: UpgradeStatus
    let error: String?
    let backupPath: String?
    let startedAt: String
    let finishedAt: String?

    var id: String { jobId }

    private enum CodingKeys: String, CodingKey {
        case serverId = "server_id"
        case jobId = "job_id"
        case targetVersion = "target_version"
        case stage
        case status
        case error
        case backupPath = "backup_path"
        case startedAt = "started_at"
        case finishedAt = "finished_at"
    }

    init(
        serverId: String,
        jobId: String,
        targetVersion: String,
        stage: UpgradeStage,
        status: UpgradeStatus,
        error: String?,
        backupPath: String?,
        startedAt: String,
        finishedAt: String?
    ) {
        self.serverId = serverId
        self.jobId = jobId
        self.targetVersion = targetVersion
        self.stage = stage
        self.status = status
        self.error = error
        self.backupPath = backupPath
        self.startedAt = startedAt
        self.finishedAt = finishedAt
    }
}
