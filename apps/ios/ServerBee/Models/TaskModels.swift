import Foundation

/// Whether a command runs once immediately or on a recurring cron schedule.
/// Raw values match the wire exactly (no key strategy on the decoders).
enum TaskKind: String, Codable, Sendable, CaseIterable, Identifiable {
    case oneshot
    case scheduled

    var id: String { rawValue }

    var label: String {
        switch self {
        case .oneshot: String(localized: "One-shot")
        case .scheduled: String(localized: "Scheduled")
        }
    }
}

/// A command task (`GET /api/tasks`). `serverIds` is an ARRAY on the wire here
/// (the server decodes its `server_ids_json` column for the response). Named
/// `CommandTask` to avoid clashing with Swift's `Task`.
struct CommandTask: Decodable, Identifiable, Sendable {
    let id: String
    let command: String
    let serverIds: [String]
    let createdAt: String
    let taskType: TaskKind
    let name: String?
    let cronExpression: String?
    let enabled: Bool
    let timeout: Int?
    let retryCount: Int
    let retryInterval: Int
    let lastRunAt: String?
    let nextRunAt: String?

    enum CodingKeys: String, CodingKey {
        case id, command, name, enabled, timeout
        case serverIds = "server_ids"
        case createdAt = "created_at"
        case taskType = "task_type"
        case cronExpression = "cron_expression"
        case retryCount = "retry_count"
        case retryInterval = "retry_interval"
        case lastRunAt = "last_run_at"
        case nextRunAt = "next_run_at"
    }

    /// A human label for the list (name if set, else the command).
    var displayName: String {
        if let name, !name.isEmpty { return name }
        return command
    }
}

/// One execution result for a task (`GET /api/tasks/{id}/results`). `id` is an
/// Int64 (not a String), and `runId` groups the per-server results of a single
/// scheduled run (nil for one-shot dispatches).
struct TaskResult: Decodable, Identifiable, Sendable {
    let id: Int64
    let taskId: String
    let serverId: String
    let output: String
    let exitCode: Int
    let runId: String?
    let attempt: Int
    let startedAt: String?
    let finishedAt: String

    enum CodingKeys: String, CodingKey {
        case id, output, attempt
        case taskId = "task_id"
        case serverId = "server_id"
        case exitCode = "exit_code"
        case runId = "run_id"
        case startedAt = "started_at"
        case finishedAt = "finished_at"
    }

    /// Human label for the exit code, decoding the dispatch sentinels the
    /// scheduler uses (-2 capability denied, -3 offline, -4 timeout).
    var statusLabel: String {
        switch exitCode {
        case 0: String(localized: "Success")
        case -2: String(localized: "Skipped — exec not enabled")
        case -3: String(localized: "Offline / not dispatched")
        case -4: String(localized: "Timed out")
        default: String(format: String(localized: "Exit %d"), exitCode)
        }
    }

    var isSuccess: Bool { exitCode == 0 }
}

/// Create body for `POST /api/tasks`. `server_ids` is an array; `task_type`
/// defaults to "oneshot" server-side. `cron_expression` is required when
/// `task_type == "scheduled"` (validated server-side).
struct CreateTaskRequest: Encodable, Sendable {
    let command: String
    let serverIds: [String]
    var timeout: Int?
    let taskType: TaskKind
    var name: String?
    var cronExpression: String?
    var retryCount: Int?
    var retryInterval: Int?

    enum CodingKeys: String, CodingKey {
        case command, name, timeout
        case serverIds = "server_ids"
        case taskType = "task_type"
        case cronExpression = "cron_expression"
        case retryCount = "retry_count"
        case retryInterval = "retry_interval"
    }
}

/// Partial update body for `PUT /api/tasks/{id}`. Omitted (nil) fields are left
/// unchanged; the enable toggle sends only `enabled`.
struct UpdateTaskRequest: Encodable, Sendable {
    var name: String?
    var command: String?
    var serverIds: [String]?
    var cronExpression: String?
    var enabled: Bool?
    var timeout: Int?
    var retryCount: Int?
    var retryInterval: Int?

    enum CodingKeys: String, CodingKey {
        case name, command, enabled, timeout
        case serverIds = "server_ids"
        case cronExpression = "cron_expression"
        case retryCount = "retry_count"
        case retryInterval = "retry_interval"
    }
}
