import Foundation
import XCTest

@testable import ServerBee

final class TailscaleTests: XCTestCase {
    // MARK: - Capability & Visibility

    func test_tailscaleCapabilityBitAndVisibilityRules() {
        XCTAssertEqual(Capability.tailscale.rawValue, 2048)
        let enabledCaps = CapabilitySet(configured: 3900, agentLocal: 3900, effective: 3900)
        let disabledCaps = CapabilitySet(configured: 1852, agentLocal: 1852, effective: 1852)

        XCTAssertTrue(
            TailscaleVisibility.shouldShow(
                capabilities: enabledCaps,
                isOnline: true,
                features: ["tailscale"],
                tailscaleIpv4: nil
            )
        )
        XCTAssertTrue(
            TailscaleVisibility.shouldShow(
                capabilities: enabledCaps,
                isOnline: true,
                features: nil,
                tailscaleIpv4: "100.64.0.10"
            )
        )
        XCTAssertFalse(
            TailscaleVisibility.shouldShow(
                capabilities: enabledCaps,
                isOnline: true,
                features: ["docker"],
                tailscaleIpv4: nil
            )
        )
        XCTAssertFalse(
            TailscaleVisibility.shouldShow(
                capabilities: disabledCaps,
                isOnline: true,
                features: ["tailscale"],
                tailscaleIpv4: "100.64.0.10"
            )
        )

        // Offline server with effective=0 still shows cached Tailscale if configured + persisted IP.
        let offlineRuntimeCaps = CapabilitySet(configured: 3900, agentLocal: 0, effective: 0)
        XCTAssertTrue(
            TailscaleVisibility.shouldShow(
                capabilities: offlineRuntimeCaps,
                sectionCapabilities: CapabilitySet(configured: 3900, agentLocal: nil, effective: nil),
                isOnline: false,
                features: nil,
                tailscaleIpv4: "100.64.0.10"
            )
        )
    }

    // MARK: - Legacy & Tailscale Server Model Decoding

    func test_legacyServerModelsDecodeWithoutTailscaleKeys() throws {
        let legacyStatusJSON = """
            {
              "id": "srv-legacy",
              "name": "Legacy Host",
              "online": true,
              "cpu": 12.5,
              "ipv4": "203.0.113.10"
            }
            """
        let status = try JSONDecoder.snakeCase.decode(ServerStatus.self, from: Data(legacyStatusJSON.utf8))
        XCTAssertEqual(status.id, "srv-legacy")
        XCTAssertNil(status.tailscaleIpv4)
        XCTAssertNil(status.tailscaleIpv6)
        XCTAssertNil(status.tailscaleDnsName)
        XCTAssertNil(status.tailscaleTailnet)

        let legacyConfigJSON = """
            {
              "id": "srv-legacy",
              "name": "Legacy Host",
              "ipv4": "203.0.113.10",
              "capabilities": 1852
            }
            """
        let config = try JSONDecoder.snakeCase.decode(ServerConfig.self, from: Data(legacyConfigJSON.utf8))
        XCTAssertEqual(config.id, "srv-legacy")
        XCTAssertNil(config.tailscaleIpv4)
        XCTAssertNil(config.tailscaleIpv6)
        XCTAssertNil(config.tailscaleDnsName)
        XCTAssertNil(config.tailscaleTailnet)
    }

    func test_serverModelsDecodeAndPreserveTailscaleFieldsAcrossPartialMerge() throws {
        let fullJSON = """
            {
              "id": "srv-ts",
              "name": "Mesh Node",
              "online": true,
              "features": ["docker", "tailscale"],
              "tailscale_ipv4": "100.64.1.20",
              "tailscale_ipv6": "fd7a:115c:a1e0::20",
              "tailscale_dns_name": "mesh-node.tailnet.ts.net.",
              "tailscale_tailnet": "tailnet.ts.net"
            }
            """
        var status = try JSONDecoder.snakeCase.decode(ServerStatus.self, from: Data(fullJSON.utf8))
        XCTAssertEqual(status.tailscaleIpv4, "100.64.1.20")
        XCTAssertEqual(status.tailscaleIpv6, "fd7a:115c:a1e0::20")
        XCTAssertEqual(status.tailscaleDnsName, "mesh-node.tailnet.ts.net.")
        XCTAssertEqual(status.tailscaleTailnet, "tailnet.ts.net")
        XCTAssertEqual(status.features, ["docker", "tailscale"])

        let partialUpdateJSON = #"{"id":"srv-ts","name":"Mesh Node","cpu":55.0}"#
        let partial = try JSONDecoder.snakeCase.decode(ServerStatus.self, from: Data(partialUpdateJSON.utf8))
        status.merge(from: partial)

        XCTAssertEqual(status.cpuUsage, 55.0)
        XCTAssertEqual(status.tailscaleIpv4, "100.64.1.20")
        XCTAssertEqual(status.tailscaleIpv6, "fd7a:115c:a1e0::20")
        XCTAssertEqual(status.tailscaleDnsName, "mesh-node.tailnet.ts.net.")
        XCTAssertEqual(status.tailscaleTailnet, "tailnet.ts.net")

        let config = try JSONDecoder.snakeCase.decode(ServerConfig.self, from: Data(fullJSON.utf8))
        XCTAssertEqual(config.tailscaleIpv4, "100.64.1.20")
        XCTAssertEqual(config.tailscaleIpv6, "fd7a:115c:a1e0::20")
        XCTAssertEqual(config.tailscaleDnsName, "mesh-node.tailnet.ts.net.")
        XCTAssertEqual(config.tailscaleTailnet, "tailnet.ts.net")
    }

    // MARK: - TailscaleModels CodingKeys Round-Trips

    func test_serverTailscaleStatusResponseRoundTrip() throws {
        let payload = ServerTailscaleStatusResponse(
            available: true,
            online: true,
            cached: false,
            tailscaleIpv4: "100.64.0.10",
            tailscaleIpv6: "fd7a:115c:a1e0::10",
            tailscaleDnsName: "node-a.tailnet.ts.net.",
            tailscaleTailnet: "tailnet.ts.net",
            status: TailscaleStatusReport(
                summary: TailscaleNodeSummary(
                    backendState: "Running",
                    tailscaleIps: ["100.64.0.10", "fd7a:115c:a1e0::10"],
                    dnsName: "node-a.tailnet.ts.net.",
                    tailnetName: "tailnet.ts.net",
                    version: "1.76.1",
                    hostname: "node-a",
                    online: true,
                    exitNode: false,
                    exitNodeOption: true,
                    keyExpiry: "2027-01-01T00:00:00Z"
                ),
                advertisedRoutes: ["10.0.0.0/24"],
                tags: ["tag:prod"],
                derpRegion: "tok",
                peers: [
                    TailscalePeerInfo(
                        id: "peer-1",
                        hostName: "node-b",
                        dnsName: "node-b.tailnet.ts.net.",
                        os: "linux",
                        tailscaleIps: ["100.64.0.11"],
                        online: true,
                        active: true,
                        connectionMode: "direct",
                        curAddr: "203.0.113.5:41641",
                        relay: "tok",
                        rxBytes: 1024,
                        txBytes: 2048,
                        lastHandshake: "2026-11-21T03:00:00Z",
                        lastSeen: nil,
                        exitNode: false,
                        exitNodeOption: false,
                        tags: ["tag:peer"]
                    )
                ],
                healthWarnings: [],
                collectedAt: "2026-11-21T03:35:00Z"
            )
        )

        let data = try JSONEncoder.snakeCase.encode(payload)
        let jsonString = try XCTUnwrap(String(data: data, encoding: .utf8))
        XCTAssertTrue(jsonString.contains("\"tailscale_ipv4\":\"100.64.0.10\""))
        XCTAssertTrue(jsonString.contains("\"backend_state\":\"Running\""))
        XCTAssertTrue(jsonString.contains("\"advertised_routes\":[\"10.0.0.0\\/24\"]") || jsonString.contains("\"advertised_routes\":[\"10.0.0.0/24\"]"))
        XCTAssertTrue(jsonString.contains("\"connection_mode\":\"direct\""))
        XCTAssertTrue(jsonString.contains("\"cur_addr\":\"203.0.113.5:41641\""))
        XCTAssertTrue(jsonString.contains("\"rx_bytes\":1024"))
        XCTAssertFalse(jsonString.contains("tailscaleIpv4"))
        XCTAssertFalse(jsonString.contains("backendState"))

        let decoded = try JSONDecoder.snakeCase.decode(ServerTailscaleStatusResponse.self, from: data)
        XCTAssertEqual(decoded, payload)
        XCTAssertEqual(decoded.status?.summary.cleanDnsName, "node-a.tailnet.ts.net")
    }

    func test_tailnetOverviewAndAdminConfigRoundTrips() throws {
        let overview = TailnetOverviewResponse(
            configured: true,
            provider: "headscale",
            tailnet: "corp.internal",
            serverTailnetUrl: "https://panel.corp.internal",
            totalDevices: 2,
            onlineDevices: 1,
            linkedServers: 1,
            expiringKeysCount: 0,
            fetchedAt: "2026-11-21T03:40:00Z",
            error: nil,
            devices: [
                TailnetDeviceItem(
                    id: "dev-1",
                    name: "tokyo-edge.corp.internal.",
                    hostname: "tokyo-edge",
                    os: "linux",
                    addresses: ["100.64.0.1", "fd7a:115c:a1e0::1"],
                    authorized: true,
                    online: true,
                    lastSeen: "2026-11-21T03:39:00Z",
                    keyExpiryDisabled: true,
                    expires: nil,
                    clientVersion: "1.76.1",
                    tags: ["tag:edge"],
                    matchedServerId: "srv-1",
                    matchedServerName: "Tokyo Edge"
                )
            ]
        )

        let overviewData = try JSONEncoder.snakeCase.encode(overview)
        let overviewObj = try XCTUnwrap(JSONSerialization.jsonObject(with: overviewData) as? [String: Any])
        XCTAssertEqual(overviewObj["server_tailnet_url"] as? String, "https://panel.corp.internal")
        XCTAssertEqual(overviewObj["total_devices"] as? Int, 2)
        XCTAssertEqual(overviewObj["linked_servers"] as? Int, 1)
        let decodedOverview = try JSONDecoder.snakeCase.decode(TailnetOverviewResponse.self, from: overviewData)
        XCTAssertEqual(decodedOverview, overview)
        XCTAssertEqual(decodedOverview.devices.first?.cleanName, "tokyo-edge.corp.internal")

        let adminConfig = TailscaleAdminConfigResponse(
            enabled: true,
            provider: "tailscale",
            apiBaseUrl: "https://api.tailscale.com",
            tailnet: "-",
            hasApiKey: true,
            preferredServerUrl: "https://sb.tailnet.ts.net",
            authEnabled: true,
            autoProvisionRole: "member"
        )
        let configData = try JSONEncoder.snakeCase.encode(adminConfig)
        let decodedConfig = try JSONDecoder.snakeCase.decode(TailscaleAdminConfigResponse.self, from: configData)
        XCTAssertEqual(decodedConfig, adminConfig)

        let updateReq = UpdateTailscaleConfigRequest(
            enabled: true,
            provider: "tailscale",
            apiBaseUrl: "https://api.tailscale.com",
            tailnet: "-",
            apiKey: nil,
            preferredServerUrl: "https://sb.tailnet.ts.net",
            authEnabled: false,
            autoProvisionRole: ""
        )
        let updateObj = try XCTUnwrap(
            JSONSerialization.jsonObject(with: JSONEncoder.snakeCase.encode(updateReq)) as? [String: Any]
        )
        XCTAssertNil(updateObj["api_key"], "Nil apiKey must be omitted so stored key is preserved")
        XCTAssertEqual(updateObj["api_base_url"] as? String, "https://api.tailscale.com")
        XCTAssertEqual(updateObj["auth_enabled"] as? Bool, false)

        let testReq = TestTailscaleConfigRequest(
            provider: "headscale",
            apiBaseUrl: "https://hs.example.com",
            tailnet: "default",
            apiKey: "secret-key"
        )
        let testReqObj = try XCTUnwrap(
            JSONSerialization.jsonObject(with: JSONEncoder.snakeCase.encode(testReq)) as? [String: Any]
        )
        XCTAssertEqual(testReqObj["api_base_url"] as? String, "https://hs.example.com")
        XCTAssertEqual(testReqObj["api_key"] as? String, "secret-key")

        let testRespJSON = #"{"success":true,"device_count":5,"message":"Connected"}"#
        let testResp = try JSONDecoder.snakeCase.decode(
            TailscaleConnectionTestResponse.self,
            from: Data(testRespJSON.utf8)
        )
        XCTAssertTrue(testResp.success)
        XCTAssertEqual(testResp.deviceCount, 5)
        XCTAssertEqual(testResp.message, "Connected")
    }

    // MARK: - Localized Labels & Timestamp Filtering

    func test_localizedConnectionModeAndBackendStateLabels() {
        XCTAssertEqual(
            TailscalePeerInfo.connectionModeLabel(mode: "direct", relay: "tok"),
            String(localized: "Direct")
        )
        XCTAssertEqual(
            TailscalePeerInfo.connectionModeLabel(mode: "derp", relay: "tok"),
            String(localized: "DERP · \("tok")")
        )
        XCTAssertEqual(
            TailscalePeerInfo.connectionModeLabel(mode: "derp", relay: nil),
            String(localized: "DERP")
        )
        XCTAssertEqual(
            TailscalePeerInfo.connectionModeLabel(mode: "peer_relay", relay: nil),
            String(localized: "Peer Relay")
        )
        XCTAssertEqual(
            TailscalePeerInfo.connectionModeLabel(mode: "idle", relay: nil),
            String(localized: "Idle")
        )
        XCTAssertEqual(
            TailscaleNodeSummary.backendStateLabel(for: "Running"),
            String(localized: "Running")
        )
        XCTAssertEqual(
            TailscaleNodeSummary.backendStateLabel(for: "NeedsLogin"),
            String(localized: "Needs login")
        )
        XCTAssertNil(TailscaleFormatting.validTimestamp("0001-01-01T00:00:00Z"))
        XCTAssertEqual(
            TailscaleFormatting.validTimestamp("2026-11-21T03:00:00Z"),
            "2026-11-21T03:00:00Z"
        )
    }
}

@MainActor
final class TailscaleViewModelTests: XCTestCase {
    override func setUp() async throws {
        URLProtocol.registerClass(AuthenticationURLProtocol.self)
    }

    override func tearDown() async throws {
        AuthenticationURLProtocol.handler = nil
        AuthenticationURLProtocol.cancelPending()
        URLProtocol.unregisterClass(AuthenticationURLProtocol.self)
        AuthManager().clearAuth()
        try? KeychainService.deleteThrowing(for: PrivateSessionRevocationStorage.key)
    }

    private func makeClient(role: String = "admin") -> APIClient {
        let auth = AuthManager()
        auth.setServerUrl("https://metrics.test")
        auth.handleLoginResponse(
            MobileTokenResponse(
                accessToken: "fixture-access",
                accessExpiresInSecs: 900,
                refreshToken: "fixture-refresh",
                refreshExpiresInSecs: 3_600,
                tokenType: "Bearer",
                user: MobileUser(id: "u1", username: "operator", role: role),
                revocationToken: "fixture-proof",
                mobileSessionId: "11111111-1111-4111-8111-111111111111"
            ))
        return APIClient(authManager: auth)
    }

    func test_serverTailscaleViewModelLoadsAndMapsErrors() async {
        let vm = ServerTailscaleViewModel()
        let api = makeClient()

        AuthenticationURLProtocol.handler = { req in
            let body = """
                {"data":{
                  "available":true,
                  "online":true,
                  "cached":false,
                  "tailscale_ipv4":"100.64.0.9",
                  "status":{
                    "summary":{
                      "backend_state":"Running",
                      "tailscale_ips":["100.64.0.9"],
                      "dns_name":"edge.tailnet.ts.net.",
                      "online":true,
                      "exit_node":false,
                      "exit_node_option":false
                    },
                    "advertised_routes":[],
                    "tags":[],
                    "peers":[
                      {"id":"2","host_name":"offline-peer","tailscale_ips":["100.64.0.3"],"online":false,"active":false,"connection_mode":"idle","rx_bytes":0,"tx_bytes":0,"exit_node":false,"exit_node_option":false,"tags":[]},
                      {"id":"1","host_name":"active-peer","tailscale_ips":["100.64.0.2"],"online":true,"active":true,"connection_mode":"direct","rx_bytes":100,"tx_bytes":200,"exit_node":false,"exit_node_option":false,"tags":[]}
                    ],
                    "health_warnings":[],
                    "collected_at":"2026-11-21T03:00:00Z"
                  }
                }}
                """
            req.respond(200, data: Data(body.utf8))
        }

        await vm.load(serverId: "srv-1", apiClient: api)
        XCTAssertTrue(vm.hasLoaded)
        XCTAssertNil(vm.errorMessage)
        XCTAssertEqual(vm.primaryIpv4, "100.64.0.9")
        XCTAssertEqual(vm.dnsName, "edge.tailnet.ts.net")
        XCTAssertEqual(vm.sortedPeers.map(\.hostName), ["active-peer", "offline-peer"])

        // 403 error mapping
        AuthenticationURLProtocol.handler = { req in
            req.respond(403, data: Data(#"{"error":{"message":"Capability tailscale disabled"}}"#.utf8))
        }
        await vm.load(serverId: "srv-1", apiClient: api)
        XCTAssertEqual(vm.errorMessage, "Capability tailscale disabled")
    }

    func test_tailscaleAdminViewModelLoadSaveAndTestConnection() async {
        let vm = TailscaleAdminViewModel()
        let api = makeClient(role: "admin")

        AuthenticationURLProtocol.handler = { req in
            let path = req.request.url?.path ?? ""
            let method = req.request.httpMethod ?? "GET"
            if path == "/api/tailscale/config" && method == "GET" {
                let json = """
                    {"data":{"enabled":true,"provider":"tailscale","api_base_url":"https://api.tailscale.com",\
                    "tailnet":"-","has_api_key":true,"preferred_server_url":"","auth_enabled":false,\
                    "auto_provision_role":""}}
                    """
                req.respond(200, data: Data(json.utf8))
            } else if path == "/api/tailscale/config" && method == "PUT" {
                let json = """
                    {"data":{"enabled":true,"provider":"headscale","api_base_url":"https://hs.test",\
                    "tailnet":"corp","has_api_key":true,"preferred_server_url":"https://sb.corp",\
                    "auth_enabled":true,"auto_provision_role":"member"}}
                    """
                req.respond(200, data: Data(json.utf8))
            } else if path == "/api/tailscale/overview" {
                let json = """
                    {"data":{"configured":true,"provider":"tailscale","tailnet":"corp","total_devices":1,\
                    "online_devices":1,"linked_servers":1,"expiring_keys_count":0,"devices":[\
                    {"id":"d1","name":"node1","hostname":"node1","addresses":["100.64.0.1"],\
                    "authorized":true,"online":true,"key_expiry_disabled":true,"tags":[],\
                    "matched_server_id":"s1","matched_server_name":"Server 1"}]}}
                    """
                req.respond(200, data: Data(json.utf8))
            } else if path == "/api/tailscale/test" {
                req.respond(200, data: Data(#"{"data":{"success":true,"device_count":3,"message":"3 devices found"}}"#.utf8))
            } else {
                req.respond(404, data: Data(#"{"error":{"message":"Not found"}}"#.utf8))
            }
        }

        await vm.load(isAdmin: true, apiClient: api)
        XCTAssertNil(vm.loadError)
        XCTAssertEqual(vm.config?.hasApiKey, true)
        XCTAssertEqual(vm.overview?.linkedServers, 1)

        let testResp = await vm.testConnection(
            TestTailscaleConfigRequest(provider: "tailscale", apiBaseUrl: "https://api.tailscale.com", tailnet: "-", apiKey: nil),
            apiClient: api
        )
        XCTAssertEqual(testResp?.success, true)
        XCTAssertEqual(testResp?.deviceCount, 3)

        let saved = await vm.save(
            UpdateTailscaleConfigRequest(
                enabled: true,
                provider: "headscale",
                apiBaseUrl: "https://hs.test",
                tailnet: "corp",
                apiKey: nil,
                preferredServerUrl: "https://sb.corp",
                authEnabled: true,
                autoProvisionRole: "member"
            ),
            apiClient: api
        )
        XCTAssertTrue(saved)
        XCTAssertEqual(vm.config?.provider, "headscale")
        XCTAssertEqual(vm.config?.authEnabled, true)
    }
}
