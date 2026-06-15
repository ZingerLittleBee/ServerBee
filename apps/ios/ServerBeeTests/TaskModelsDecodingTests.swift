import XCTest
@testable import ServerBee

final class TaskModelsDecodingTests: XCTestCase {

    func test_decode_commandTask_scheduled() throws {
        let json = """
        {
          "id": "t1",
          "command": "uptime",
          "server_ids": ["s1", "s2"],
          "created_at": "2026-06-15T10:00:00Z",
          "task_type": "scheduled",
          "name": "Daily uptime",
          "cron_expression": "0 0 9 * * *",
          "enabled": true,
          "timeout": 30,
          "retry_count": 2,
          "retry_interval": 60,
          "last_run_at": "2026-06-15T09:00:00Z",
          "next_run_at": "2026-06-16T09:00:00Z"
        }
        """
        let task = try JSONDecoder.snakeCase.decode(CommandTask.self, from: Data(json.utf8))
        XCTAssertEqual(task.id, "t1")
        XCTAssertEqual(task.command, "uptime")
        // server_ids is an ARRAY in the response (unlike maintenance/ping json-string).
        XCTAssertEqual(task.serverIds, ["s1", "s2"])
        XCTAssertEqual(task.taskType, .scheduled)
        XCTAssertEqual(task.cronExpression, "0 0 9 * * *")
        XCTAssertEqual(task.retryCount, 2)
        XCTAssertEqual(task.displayName, "Daily uptime")
    }

    func test_decode_commandTask_oneshot_minimalOptionals() throws {
        let json = """
        {
          "id": "t2",
          "command": "df -h",
          "server_ids": [],
          "created_at": "2026-06-15T10:00:00Z",
          "task_type": "oneshot",
          "name": null,
          "cron_expression": null,
          "enabled": true,
          "timeout": null,
          "retry_count": 0,
          "retry_interval": 60,
          "last_run_at": null,
          "next_run_at": null
        }
        """
        let task = try JSONDecoder.snakeCase.decode(CommandTask.self, from: Data(json.utf8))
        XCTAssertEqual(task.taskType, .oneshot)
        XCTAssertNil(task.name)
        XCTAssertNil(task.timeout)
        XCTAssertNil(task.nextRunAt)
        // displayName falls back to the command when name is nil.
        XCTAssertEqual(task.displayName, "df -h")
    }

    func test_decode_taskResult_int64IdAndExitSentinels() throws {
        let json = """
        [
          {
            "id": 1001,
            "task_id": "t1",
            "server_id": "s1",
            "output": "ok",
            "exit_code": 0,
            "run_id": "run-1",
            "attempt": 1,
            "started_at": "2026-06-15T09:00:00Z",
            "finished_at": "2026-06-15T09:00:01Z"
          },
          {
            "id": 1002,
            "task_id": "t1",
            "server_id": "s2",
            "output": "",
            "exit_code": -2,
            "run_id": null,
            "attempt": 1,
            "started_at": null,
            "finished_at": "2026-06-15T09:00:01Z"
          }
        ]
        """
        let results = try JSONDecoder.snakeCase.decode([TaskResult].self, from: Data(json.utf8))
        XCTAssertEqual(results[0].id, 1001)
        XCTAssertTrue(results[0].isSuccess)
        XCTAssertEqual(results[0].runId, "run-1")
        // -2 sentinel = capability denied / skipped; started_at nullable.
        XCTAssertFalse(results[1].isSuccess)
        XCTAssertNil(results[1].startedAt)
        XCTAssertEqual(results[1].statusLabel, String(localized: "Skipped — exec not enabled"))
    }

    func test_encode_createRequest_snakeCaseAndOmittedOptionals() throws {
        let request = CreateTaskRequest(
            command: "uptime", serverIds: ["s1"], timeout: nil,
            taskType: .oneshot, name: nil, cronExpression: nil,
            retryCount: nil, retryInterval: nil
        )
        let data = try JSONEncoder.snakeCase.encode(request)
        let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        XCTAssertEqual(obj?["command"] as? String, "uptime")
        XCTAssertEqual(obj?["server_ids"] as? [String], ["s1"])
        XCTAssertEqual(obj?["task_type"] as? String, "oneshot")
        // nil optionals are omitted (key absent) → server defaults apply.
        XCTAssertNil(obj?["timeout"])
        XCTAssertNil(obj?["name"])
        XCTAssertNil(obj?["cron_expression"])
    }

    func test_encode_updateRequest_enabledOnly() throws {
        let data = try JSONEncoder.snakeCase.encode(UpdateTaskRequest(enabled: false))
        let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        XCTAssertEqual(obj?["enabled"] as? Bool, false)
        XCTAssertNil(obj?["command"])
        XCTAssertNil(obj?["server_ids"])
    }
}
