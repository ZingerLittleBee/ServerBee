import XCTest
@testable import ServerBee

/// Decoding / encoding coverage for M10 Docker models. Shapes mirror
/// `crates/common/src/docker_types.rs` and the docker REST/WS payloads.
final class DockerModelsDecodingTests: XCTestCase {
    private func decode<T: Decodable>(_ type: T.Type, _ json: String) throws -> T {
        try JSONDecoder.snakeCase.decode(T.self, from: Data(json.utf8))
    }

    private func encode<T: Encodable>(_ value: T) throws -> String {
        String(data: try JSONEncoder.snakeCase.encode(value), encoding: .utf8)!
    }

    // MARK: - Container / ports

    func test_containersResponse_decodes() throws {
        let json = """
        { "containers": [
            { "id": "abc123", "name": "/web", "image": "nginx:alpine",
              "state": "running", "status": "Up 3 hours", "created": 1710000000,
              "ports": [{ "private_port": 80, "public_port": 8080, "port_type": "tcp", "ip": "0.0.0.0" }],
              "labels": { "com.docker.compose.project": "sb" } }
          ] }
        """
        let resp = try decode(DockerContainersResponse.self, json)
        let c = resp.containers[0]
        XCTAssertEqual(c.displayName, "web")
        XCTAssertTrue(c.isRunning)
        XCTAssertEqual(c.ports[0].privatePort, 80)
        XCTAssertEqual(c.ports[0].publicPort, 8080)
        XCTAssertEqual(c.ports[0].display, "0.0.0.0:8080→80/tcp")
        XCTAssertEqual(c.labels["com.docker.compose.project"], "sb")
    }

    func test_port_displayWithoutPublic() throws {
        let json = #"{ "private_port": 5432, "public_port": null, "port_type": "tcp", "ip": null }"#
        let port = try decode(DockerPort.self, json)
        XCTAssertNil(port.publicPort)
        XCTAssertEqual(port.display, "5432/tcp")
    }

    // MARK: - Stats / info

    func test_statsResponse_decodes() throws {
        let json = """
        { "stats": [
            { "id": "abc123", "name": "web", "cpu_percent": 2.5, "memory_usage": 52428800,
              "memory_limit": 536870912, "memory_percent": 9.8, "network_rx": 1048576,
              "network_tx": 524288, "block_read": 0, "block_write": 10240 }
          ] }
        """
        let resp = try decode(DockerStatsResponse.self, json)
        XCTAssertEqual(resp.stats[0].cpuPercent, 2.5, accuracy: 0.001)
        XCTAssertEqual(resp.stats[0].memoryUsage, 52_428_800)
        XCTAssertEqual(resp.stats[0].networkRx, 1_048_576)
    }

    func test_infoResponse_decodes() throws {
        let json = """
        { "info": { "docker_version": "27.1.1", "api_version": "1.46", "os": "linux",
          "arch": "x86_64", "containers_running": 3, "containers_paused": 0,
          "containers_stopped": 1, "images": 12, "memory_total": 8589934592 } }
        """
        let resp = try decode(DockerInfoResponse.self, json)
        XCTAssertEqual(resp.info.dockerVersion, "27.1.1")
        XCTAssertEqual(resp.info.containersRunning, 3)
        XCTAssertEqual(resp.info.memoryTotal, 8_589_934_592)
    }

    // MARK: - Events / networks / volumes

    func test_eventsResponse_decodes() throws {
        let json = """
        { "events": [
            { "timestamp": 1748010000, "event_type": "container", "action": "start",
              "actor_id": "abc123", "actor_name": "web", "attributes": {} }
          ] }
        """
        let resp = try decode(DockerEventsResponse.self, json)
        XCTAssertEqual(resp.events[0].eventType, "container")
        XCTAssertEqual(resp.events[0].action, "start")
        XCTAssertEqual(resp.events[0].actorName, "web")
    }

    func test_networksResponse_decodes() throws {
        let json = """
        { "networks": [{ "id": "n1", "name": "bridge", "driver": "bridge",
          "scope": "local", "containers": { "a": "web" } }] }
        """
        let resp = try decode(DockerNetworksResponse.self, json)
        XCTAssertEqual(resp.networks[0].name, "bridge")
        XCTAssertEqual(resp.networks[0].containers.count, 1)
    }

    func test_volumesResponse_decodes() throws {
        let json = """
        { "volumes": [{ "name": "pgdata", "driver": "local",
          "mountpoint": "/var/lib/docker/volumes/pgdata/_data",
          "created_at": "2026-05-20T10:00:00Z", "labels": {} }] }
        """
        let resp = try decode(DockerVolumesResponse.self, json)
        XCTAssertEqual(resp.volumes[0].id, "pgdata")
        XCTAssertEqual(resp.volumes[0].createdAt, "2026-05-20T10:00:00Z")
    }

    func test_actionResult_decodes() throws {
        let ok = try decode(DockerActionResult.self, #"{ "success": true, "error": null }"#)
        XCTAssertTrue(ok.success)
        XCTAssertNil(ok.error)
        let fail = try decode(DockerActionResult.self, #"{ "success": false, "error": "No such container" }"#)
        XCTAssertFalse(fail.success)
        XCTAssertEqual(fail.error, "No such container")
    }

    // MARK: - DockerAction encoding (externally-tagged enum)

    func test_action_start_encodesAsBareString() throws {
        let json = try encode(ContainerActionRequest(action: .start))
        XCTAssertEqual(json, #"{"action":"Start"}"#)
    }

    func test_action_stop_encodesNestedTimeout() throws {
        let json = try encode(ContainerActionRequest(action: .stop(timeout: 10)))
        XCTAssertEqual(json, #"{"action":{"Stop":{"timeout":10}}}"#)
    }

    func test_action_stop_nilTimeoutOmitsKey() throws {
        let json = try encode(ContainerActionRequest(action: .stop(timeout: nil)))
        XCTAssertEqual(json, #"{"action":{"Stop":{}}}"#)
    }

    func test_action_restart_encodes() throws {
        let json = try encode(ContainerActionRequest(action: .restart(timeout: 5)))
        XCTAssertEqual(json, #"{"action":{"Restart":{"timeout":5}}}"#)
    }

    func test_action_remove_encodesForce() throws {
        XCTAssertEqual(try encode(ContainerActionRequest(action: .remove(force: true))),
                       #"{"action":{"Remove":{"force":true}}}"#)
        XCTAssertEqual(try encode(ContainerActionRequest(action: .remove(force: false))),
                       #"{"action":{"Remove":{"force":false}}}"#)
    }

    // MARK: - Log stream parsing

    @MainActor
    func test_logParse_session() {
        XCTAssertEqual(DockerLogsViewModel.parse(#"{"type":"session","session_id":"s1"}"#), .session)
    }

    @MainActor
    func test_logParse_logs() {
        let msg = DockerLogsViewModel.parse(#"{"type":"logs","entries":[{"timestamp":"2026-06-15T00:00:00Z","stream":"stdout","message":"hello"},{"timestamp":null,"stream":"stderr","message":"oops"}]}"#)
        guard case .logs(let entries) = msg else { return XCTFail("expected logs") }
        XCTAssertEqual(entries.count, 2)
        XCTAssertEqual(entries[0].message, "hello")
        XCTAssertFalse(entries[0].isError)
        XCTAssertTrue(entries[1].isError)
    }

    @MainActor
    func test_logParse_unknown() {
        XCTAssertEqual(DockerLogsViewModel.parse(#"{"type":"pong"}"#), .unknown)
        XCTAssertEqual(DockerLogsViewModel.parse("not json"), .unknown)
    }

    @MainActor
    func test_logURL_buildsWss() {
        let url = DockerLogsViewModel.makeURL(serverUrl: "https://demo.serverbee.app", serverId: "srv1")
        XCTAssertEqual(url?.absoluteString, "wss://demo.serverbee.app/api/ws/docker/logs/srv1")
    }
}
