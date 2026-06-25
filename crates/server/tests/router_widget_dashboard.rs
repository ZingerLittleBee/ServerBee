//! Router-level integration tests for the dashboard and widget-module APIs.
//!
//! These exercise the real HTTP surface (`/api/dashboards*` and
//! `/api/widget-modules*`) through the shared test harness: a freshly migrated
//! SQLite DB, a seeded admin, and session-cookie auth. Each `#[tokio::test]`
//! gets its own server bound to a random port, so state never leaks between
//! tests.
mod common;

use common::{create_server, http_client, login_admin, login_as_new_user, start_test_server};
use serde_json::{Value, json};

/// A minimal valid single-file widget JS source with a JSDoc manifest the
/// extractor accepts. The `id` is baked into the manifest.
fn widget_js(id: &str) -> String {
    format!(
        r#"/**
 * @serverbee-widget {{
 *   "id": "{id}",
 *   "version": "1.0.0",
 *   "name": "Test {id}",
 *   "category": "Real-time",
 *   "sizing": {{ "defaultW": 2, "defaultH": 2, "minW": 1, "minH": 1, "strategy": "free" }},
 *   "sdkVersion": "^0.1.0"
 * }}
 */
export default {{}};"#
    )
}

// ===================== dashboards =====================

/// GET /api/dashboards/default auto-creates and returns the default dashboard
/// with its preset widgets for an authenticated admin.
#[tokio::test]
async fn get_default_dashboard_auto_creates_for_admin() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .get(format!("{base_url}/api/dashboards/default"))
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.expect("parse body");
    assert_eq!(body["data"]["is_default"], true);
    // The auto-created default ships with 6 preset widgets.
    assert_eq!(body["data"]["widgets"].as_array().unwrap().len(), 6);
}

/// GET /api/dashboards/default is a read route, so a member can read it too.
#[tokio::test]
async fn get_default_dashboard_allowed_for_member() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "reader", "member").await;

    let resp = member
        .get(format!("{base_url}/api/dashboards/default"))
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.expect("parse body");
    assert_eq!(body["data"]["is_default"], true);
}

/// GET /api/dashboards without a session cookie is rejected by auth middleware.
#[tokio::test]
async fn list_dashboards_unauthenticated_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .get(format!("{base_url}/api/dashboards"))
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 401);
}

/// POST /api/dashboards creates a dashboard for an admin; the first one becomes
/// the default. GET /api/dashboards then lists it.
#[tokio::test]
async fn create_and_list_dashboard_happy_path() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{base_url}/api/dashboards"))
        .json(&json!({ "name": "Ops" }))
        .send()
        .await
        .expect("create failed");
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.expect("parse body");
    assert_eq!(body["data"]["name"], "Ops");
    // First dashboard created is promoted to default.
    assert_eq!(body["data"]["is_default"], true);
    let id = body["data"]["id"].as_str().unwrap().to_string();

    let list = client
        .get(format!("{base_url}/api/dashboards"))
        .send()
        .await
        .expect("list failed");
    assert_eq!(list.status(), 200);
    let list_body: Value = list.json().await.expect("parse list");
    assert!(
        list_body["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["id"] == id.as_str())
    );
}

/// POST /api/dashboards with a malformed JSON body is rejected before reaching
/// the handler (axum Json extractor -> 400/422).
#[tokio::test]
async fn create_dashboard_malformed_body_is_rejected() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{base_url}/api/dashboards"))
        .header("content-type", "application/json")
        // Missing the required `name` field.
        .body("{}")
        .send()
        .await
        .expect("request failed");
    assert!(
        resp.status() == 400 || resp.status() == 422,
        "expected 400/422 for invalid body, got {}",
        resp.status()
    );
}

/// POST /api/dashboards from a member is blocked by require_admin (403).
#[tokio::test]
async fn create_dashboard_member_is_403() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "writer", "member").await;

    let resp = member
        .post(format!("{base_url}/api/dashboards"))
        .json(&json!({ "name": "Nope" }))
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 403);
}

/// GET /api/dashboards/{id} for an unknown id returns 404.
#[tokio::test]
async fn get_dashboard_unknown_id_is_404() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .get(format!("{base_url}/api/dashboards/does-not-exist"))
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 404);
}

/// PUT /api/dashboards/{id} updates widgets with a valid built-in widget type
/// and returns the dashboard with its persisted widgets.
#[tokio::test]
async fn update_dashboard_valid_widget_type_succeeds() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Create a dashboard to update.
    let created: Value = client
        .post(format!("{base_url}/api/dashboards"))
        .json(&json!({ "name": "Edit me" }))
        .send()
        .await
        .expect("create failed")
        .json()
        .await
        .expect("parse create");
    let id = created["data"]["id"].as_str().unwrap().to_string();

    let resp = client
        .put(format!("{base_url}/api/dashboards/{id}"))
        .json(&json!({
            "widgets": [{
                "widget_type": "gauge",
                "title": "CPU",
                "config_json": { "metric": "cpu" },
                "grid_x": 0, "grid_y": 0, "grid_w": 4, "grid_h": 3,
                "sort_order": 0
            }]
        }))
        .send()
        .await
        .expect("update failed");
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.expect("parse body");
    let widgets = body["data"]["widgets"].as_array().unwrap();
    assert_eq!(widgets.len(), 1);
    assert_eq!(widgets[0]["widget_type"], "gauge");
}

/// PUT /api/dashboards/{id} with an unknown widget_type is rejected by the
/// whitelist validation with 400.
#[tokio::test]
async fn update_dashboard_unknown_widget_type_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let created: Value = client
        .post(format!("{base_url}/api/dashboards"))
        .json(&json!({ "name": "Bad widget" }))
        .send()
        .await
        .expect("create failed")
        .json()
        .await
        .expect("parse create");
    let id = created["data"]["id"].as_str().unwrap().to_string();

    let resp = client
        .put(format!("{base_url}/api/dashboards/{id}"))
        .json(&json!({
            "widgets": [{
                "widget_type": "totally-not-real",
                "config_json": {},
                "grid_x": 0, "grid_y": 0, "grid_w": 4, "grid_h": 3,
                "sort_order": 0
            }]
        }))
        .send()
        .await
        .expect("update failed");
    assert_eq!(resp.status(), 400);
}

/// PUT /api/dashboards/{id} with widget_type "module" referencing an unknown
/// module_id is rejected with 400.
#[tokio::test]
async fn update_dashboard_unknown_module_id_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let created: Value = client
        .post(format!("{base_url}/api/dashboards"))
        .json(&json!({ "name": "Module ref" }))
        .send()
        .await
        .expect("create failed")
        .json()
        .await
        .expect("parse create");
    let id = created["data"]["id"].as_str().unwrap().to_string();

    let resp = client
        .put(format!("{base_url}/api/dashboards/{id}"))
        .json(&json!({
            "widgets": [{
                "widget_type": "module",
                "module_id": "com.does.not.exist",
                "config_json": {},
                "grid_x": 0, "grid_y": 0, "grid_w": 4, "grid_h": 3,
                "sort_order": 0
            }]
        }))
        .send()
        .await
        .expect("update failed");
    assert_eq!(resp.status(), 400);
}

/// PUT /api/dashboards/{id} that tries to unset the default flag on the only
/// (default) dashboard is rejected with 400.
#[tokio::test]
async fn update_dashboard_unset_default_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let created: Value = client
        .post(format!("{base_url}/api/dashboards"))
        .json(&json!({ "name": "Only" }))
        .send()
        .await
        .expect("create failed")
        .json()
        .await
        .expect("parse create");
    let id = created["data"]["id"].as_str().unwrap().to_string();

    let resp = client
        .put(format!("{base_url}/api/dashboards/{id}"))
        .json(&json!({ "is_default": false }))
        .send()
        .await
        .expect("update failed");
    assert_eq!(resp.status(), 400);
}

/// PUT /api/dashboards/{id} from a member is blocked by require_admin (403).
#[tokio::test]
async fn update_dashboard_member_is_403() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let id = {
        let created: Value = admin
            .post(format!("{base_url}/api/dashboards"))
            .json(&json!({ "name": "Admin owned" }))
            .send()
            .await
            .expect("create failed")
            .json()
            .await
            .expect("parse create");
        created["data"]["id"].as_str().unwrap().to_string()
    };
    let member = login_as_new_user(&admin, &base_url, "editor", "member").await;

    let resp = member
        .put(format!("{base_url}/api/dashboards/{id}"))
        .json(&json!({ "name": "Hijack" }))
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 403);
}

/// DELETE /api/dashboards/{id} removes a non-default dashboard, while the
/// default cannot be deleted (400).
#[tokio::test]
async fn delete_dashboard_default_blocked_non_default_ok() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // First created dashboard becomes the default.
    let first: Value = client
        .post(format!("{base_url}/api/dashboards"))
        .json(&json!({ "name": "Primary" }))
        .send()
        .await
        .expect("create failed")
        .json()
        .await
        .expect("parse first");
    let default_id = first["data"]["id"].as_str().unwrap().to_string();

    // Second dashboard is not the default and can be deleted.
    let second: Value = client
        .post(format!("{base_url}/api/dashboards"))
        .json(&json!({ "name": "Secondary" }))
        .send()
        .await
        .expect("create failed")
        .json()
        .await
        .expect("parse second");
    let extra_id = second["data"]["id"].as_str().unwrap().to_string();

    // Deleting the default is rejected.
    let del_default = client
        .delete(format!("{base_url}/api/dashboards/{default_id}"))
        .send()
        .await
        .expect("delete default failed");
    assert_eq!(del_default.status(), 400);

    // Deleting the non-default succeeds.
    let del_extra = client
        .delete(format!("{base_url}/api/dashboards/{extra_id}"))
        .send()
        .await
        .expect("delete extra failed");
    assert_eq!(del_extra.status(), 200);
}

/// DELETE /api/dashboards/{id} for an unknown id returns 404.
#[tokio::test]
async fn delete_dashboard_unknown_id_is_404() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .delete(format!("{base_url}/api/dashboards/no-such-dashboard"))
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 404);
}

/// DELETE /api/dashboards/{id} from a member is blocked by require_admin (403).
#[tokio::test]
async fn delete_dashboard_member_is_403() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let id = {
        let created: Value = admin
            .post(format!("{base_url}/api/dashboards"))
            .json(&json!({ "name": "Locked" }))
            .send()
            .await
            .expect("create failed")
            .json()
            .await
            .expect("parse create");
        created["data"]["id"].as_str().unwrap().to_string()
    };
    let member = login_as_new_user(&admin, &base_url, "remover", "member").await;

    let resp = member
        .delete(format!("{base_url}/api/dashboards/{id}"))
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 403);
}

// `create_server` is part of the shared harness but the dashboard/widget-module
// routers take no server id; reference it here so the import isn't flagged.
#[tokio::test]
async fn create_server_helper_usable_alongside_dashboards() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    // Sanity: the harness can seed a server, and dashboards remain independent.
    let server_id = create_server(&client, &base_url, "probe-host").await;
    assert!(!server_id.is_empty());

    let resp = client
        .get(format!("{base_url}/api/dashboards"))
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 200);
}

// ===================== widget-modules =====================

/// GET /api/widget-modules is a read route: a member can list installed
/// modules (empty by default in a fresh DB).
#[tokio::test]
async fn list_widget_modules_allowed_for_member() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "viewer", "member").await;

    let resp = member
        .get(format!("{base_url}/api/widget-modules"))
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.expect("parse body");
    assert!(body["data"].is_array());
}

/// GET /api/widget-modules without a session cookie is rejected (401).
#[tokio::test]
async fn list_widget_modules_unauthenticated_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let resp = anon
        .get(format!("{base_url}/api/widget-modules"))
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 401);
}

/// GET /api/widget-modules/{id}/{asset} for an unknown module returns 404.
#[tokio::test]
async fn serve_asset_unknown_module_is_404() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .get(format!("{base_url}/api/widget-modules/com.absent/index.js"))
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 404);
}

/// POST /api/widget-modules installs a valid single-file widget via multipart
/// for an admin, then GET lists it and DELETE uninstalls it.
#[tokio::test]
async fn install_uninstall_single_file_widget_admin() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let code = widget_js("com.test.router-widget");
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(code.into_bytes()).file_name("w.js"),
    );

    let install = client
        .post(format!("{base_url}/api/widget-modules"))
        .multipart(form)
        .send()
        .await
        .expect("install failed");
    assert_eq!(install.status(), 200);
    let install_body: Value = install.json().await.expect("parse install");
    assert_eq!(install_body["data"]["id"], "com.test.router-widget");
    assert_eq!(install_body["data"]["version"], "1.0.0");

    let list: Value = client
        .get(format!("{base_url}/api/widget-modules"))
        .send()
        .await
        .expect("list failed")
        .json()
        .await
        .expect("parse list");
    assert!(
        list["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == "com.test.router-widget")
    );

    // Uninstall returns 204 No Content for a non-builtin module.
    let del = client
        .delete(format!("{base_url}/api/widget-modules/com.test.router-widget"))
        .send()
        .await
        .expect("delete failed");
    assert_eq!(del.status(), 204);
}

/// POST /api/widget-modules with a JS file lacking a JSDoc manifest is rejected
/// with 400.
#[tokio::test]
async fn install_widget_invalid_manifest_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(b"export default {};".to_vec()).file_name("bad.js"),
    );

    let resp = client
        .post(format!("{base_url}/api/widget-modules"))
        .multipart(form)
        .send()
        .await
        .expect("install failed");
    assert_eq!(resp.status(), 400);
}

/// POST /api/widget-modules with neither a `?url=` nor a multipart file is a
/// 400 bad request.
#[tokio::test]
async fn install_widget_no_source_is_400() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .post(format!("{base_url}/api/widget-modules"))
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 400);
}

/// POST /api/widget-modules from a member is blocked by require_admin (403).
#[tokio::test]
async fn install_widget_member_is_403() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "installer", "member").await;

    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(widget_js("com.test.member").into_bytes())
            .file_name("m.js"),
    );

    let resp = member
        .post(format!("{base_url}/api/widget-modules"))
        .multipart(form)
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 403);
}

/// POST /api/widget-modules without a session cookie is rejected (401).
#[tokio::test]
async fn install_widget_unauthenticated_is_401() {
    let (base_url, _tmp) = start_test_server().await;
    let anon = http_client();

    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(widget_js("com.test.anon").into_bytes()).file_name("a.js"),
    );

    let resp = anon
        .post(format!("{base_url}/api/widget-modules"))
        .multipart(form)
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 401);
}

/// DELETE /api/widget-modules/{id} for an unknown module returns 404.
#[tokio::test]
async fn uninstall_widget_unknown_id_is_404() {
    let (base_url, _tmp) = start_test_server().await;
    let client = http_client();
    login_admin(&client, &base_url).await;

    let resp = client
        .delete(format!("{base_url}/api/widget-modules/com.never.installed"))
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 404);
}

/// DELETE /api/widget-modules/{id} from a member is blocked by require_admin
/// (403) — checked before the not-found lookup runs.
#[tokio::test]
async fn uninstall_widget_member_is_403() {
    let (base_url, _tmp) = start_test_server().await;
    let admin = http_client();
    login_admin(&admin, &base_url).await;
    let member = login_as_new_user(&admin, &base_url, "uninstaller", "member").await;

    let resp = member
        .delete(format!("{base_url}/api/widget-modules/com.anything"))
        .send()
        .await
        .expect("request failed");
    assert_eq!(resp.status(), 403);
}

// NOTE: The `?url=` install path (POST /api/widget-modules?url=...) reaches an
// outbound HTTP fetch with an SSRF guard. Driving its success path requires an
// external reachable host, which violates the no-network determinism rule, so
// it is not exercised here; its rejection paths (loopback / metadata / private
// CIDR -> 400) are already covered in `widget_module_integration.rs`.
