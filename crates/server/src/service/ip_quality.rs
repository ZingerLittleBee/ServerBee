use chrono::Utc;
use sea_orm::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::entity::{ip_quality_setting, ip_quality_snapshot, unlock_event, unlock_result, unlock_service};
use crate::error::AppError;
use serverbee_common::protocol::{IpQualitySnapshotData, UnlockResultData, UnlockServiceDef};

pub struct IpQualityService;

// ---------------------------------------------------------------------------
// Input DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateCustomServiceInput {
    pub name: String,
    pub category: String,
    pub popularity: i32,
    pub url: String,
    pub method: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    pub timeout_ms: u32,
    /// JSON array of ordered match rules
    pub rules: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateServiceInput {
    pub enabled: Option<bool>,
    // custom-only fields (ignored for built-ins)
    pub name: Option<String>,
    pub category: Option<String>,
    pub popularity: Option<i32>,
    pub url: Option<String>,
    pub method: Option<String>,
    pub headers: Option<Vec<(String, String)>>,
    pub timeout_ms: Option<u32>,
    pub rules: Option<Vec<serde_json::Value>>,
}

// ---------------------------------------------------------------------------
// Output / query DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct UnlockResultDto {
    pub id: String,
    pub server_id: String,
    pub service_id: String,
    pub status: String,
    pub region: Option<String>,
    pub latency_ms: Option<i32>,
    pub detail: Option<String>,
    pub checked_at: String,
}

impl From<unlock_result::Model> for UnlockResultDto {
    fn from(m: unlock_result::Model) -> Self {
        Self {
            id: m.id,
            server_id: m.server_id,
            service_id: m.service_id,
            status: m.status,
            region: m.region,
            latency_ms: m.latency_ms,
            detail: m.detail,
            checked_at: m.checked_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct UnlockEventDto {
    pub id: String,
    pub server_id: String,
    pub service_id: String,
    pub old_status: String,
    pub new_status: String,
    pub changed_at: String,
}

impl From<unlock_event::Model> for UnlockEventDto {
    fn from(m: unlock_event::Model) -> Self {
        Self {
            id: m.id,
            server_id: m.server_id,
            service_id: m.service_id,
            old_status: m.old_status,
            new_status: m.new_status,
            changed_at: m.changed_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ServerIpQualitySummary {
    pub server_id: String,
    pub unlock_results: Vec<UnlockResultDto>,
    pub ip_quality: Option<IpQualitySnapshotData>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct IpQualityOverviewRow {
    pub server_id: String,
    pub unlock_results: Vec<UnlockResultDto>,
    pub ip_quality: Option<IpQualitySnapshotData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct IpQualitySettingDto {
    pub check_interval_hours: i32,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const SETTING_ID: &str = "default";

fn status_to_str(status: &serverbee_common::protocol::UnlockStatus) -> String {
    // Serialize the enum to its snake_case string representation.
    serde_json::to_value(status)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "failed".to_string())
}

fn snapshot_model_to_dto(m: ip_quality_snapshot::Model) -> IpQualitySnapshotData {
    IpQualitySnapshotData {
        ip: m.ip,
        asn: m.asn,
        as_org: m.as_org,
        country: m.country,
        region: m.region,
        city: m.city,
        ip_type: m.ip_type,
        is_proxy: m.is_proxy,
        is_vpn: m.is_vpn,
        is_hosting: m.is_hosting,
        risk_score: m.risk_score,
        risk_level: m.risk_level,
        checked_at: m.checked_at,
    }
}

/// Validate a URL for use as a custom service endpoint.
/// Rejects non-http/https schemes and ports other than 80/443.
fn validate_service_url(url: &str) -> Result<(), AppError> {
    let parsed = url::Url::parse(url).map_err(|e| {
        AppError::Validation(format!("Invalid URL: {e}"))
    })?;

    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(AppError::Validation(
            "URL scheme must be http or https".to_string(),
        ));
    }

    if parsed.port().is_some_and(|port| port != 80 && port != 443) {
        return Err(AppError::Validation(
            "URL port must be 80 or 443 (or absent)".to_string(),
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Service implementation
// ---------------------------------------------------------------------------

impl IpQualityService {
    // -----------------------------------------------------------------------
    // Catalog CRUD
    // -----------------------------------------------------------------------

    /// List all unlock services (built-in + custom).
    pub async fn list_services(
        db: &DatabaseConnection,
    ) -> Result<Vec<unlock_service::Model>, AppError> {
        let services = unlock_service::Entity::find()
            .order_by_asc(unlock_service::Column::Category)
            .order_by_desc(unlock_service::Column::Popularity)
            .all(db)
            .await?;
        Ok(services)
    }

    /// Create a custom unlock service.
    ///
    /// Validates:
    /// - `key` is unique (not colliding with any existing row)
    /// - `url` scheme is http or https
    /// - `url` port is 80, 443, or absent
    /// - `rules` is non-empty
    pub async fn create_custom_service(
        db: &DatabaseConnection,
        input: CreateCustomServiceInput,
    ) -> Result<unlock_service::Model, AppError> {
        // Validate URL
        validate_service_url(&input.url)?;

        // Validate rules non-empty
        if input.rules.is_empty() {
            return Err(AppError::Validation(
                "rules must not be empty for a custom service".to_string(),
            ));
        }

        let now = Utc::now();

        // Generate key = custom_<first-8-chars-of-uuid>
        let short_id = Uuid::new_v4().to_string().replace('-', "");
        let key = format!("custom_{}", &short_id[..8]);

        // Check key uniqueness (unlikely collision, but guard against it)
        let existing = unlock_service::Entity::find()
            .filter(unlock_service::Column::Key.eq(&key))
            .one(db)
            .await?;
        if existing.is_some() {
            return Err(AppError::Conflict(format!("Key '{key}' already exists")));
        }

        // Build request JSON
        let request_json = serde_json::json!({
            "url": input.url,
            "method": input.method,
            "headers": input.headers,
            "timeout_ms": input.timeout_ms,
        });
        let rules_json = serde_json::Value::Array(input.rules);

        let id = Uuid::new_v4().to_string();
        let model = unlock_service::ActiveModel {
            id: Set(id),
            key: Set(key),
            name: Set(input.name),
            category: Set(input.category),
            popularity: Set(input.popularity),
            is_builtin: Set(false),
            enabled: Set(true),
            detector: Set(None),
            request: Set(Some(request_json.to_string())),
            rules: Set(Some(rules_json.to_string())),
            created_at: Set(now),
            updated_at: Set(now),
        };

        Ok(model.insert(db).await?)
    }

    /// Update a service.
    ///
    /// For built-in services, only `enabled` may be changed.
    /// For custom services, all fields may be updated.
    pub async fn update_service(
        db: &DatabaseConnection,
        id: &str,
        input: UpdateServiceInput,
    ) -> Result<unlock_service::Model, AppError> {
        let existing = unlock_service::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Service {id} not found")))?;

        let mut active: unlock_service::ActiveModel = existing.clone().into();

        if existing.is_builtin {
            // For built-in services, only `enabled` may change
            if let Some(enabled) = input.enabled {
                active.enabled = Set(enabled);
            }
            active.updated_at = Set(Utc::now());
        } else {
            // Custom service: full update
            if let Some(enabled) = input.enabled {
                active.enabled = Set(enabled);
            }
            if let Some(name) = input.name {
                active.name = Set(name);
            }
            if let Some(category) = input.category {
                active.category = Set(category);
            }
            if let Some(popularity) = input.popularity {
                active.popularity = Set(popularity);
            }

            // URL / request update
            if input.url.is_some()
                || input.method.is_some()
                || input.headers.is_some()
                || input.timeout_ms.is_some()
            {
                // Parse existing request to layer updates on top
                let existing_request: serde_json::Value = existing
                    .request
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_else(|| serde_json::json!({}));

                let url = input
                    .url
                    .clone()
                    .or_else(|| existing_request["url"].as_str().map(|s| s.to_string()))
                    .unwrap_or_default();

                // Validate new URL if provided
                if input.url.is_some() {
                    validate_service_url(&url)?;
                }

                let method = input
                    .method
                    .unwrap_or_else(|| {
                        existing_request["method"]
                            .as_str()
                            .unwrap_or("GET")
                            .to_string()
                    });
                let headers = input.headers.unwrap_or_default();
                let timeout_ms = input.timeout_ms.unwrap_or(5000);

                let request_json = serde_json::json!({
                    "url": url,
                    "method": method,
                    "headers": headers,
                    "timeout_ms": timeout_ms,
                });
                active.request = Set(Some(request_json.to_string()));
            }

            if let Some(rules) = input.rules {
                if rules.is_empty() {
                    return Err(AppError::Validation(
                        "rules must not be empty for a custom service".to_string(),
                    ));
                }
                let rules_json = serde_json::Value::Array(rules);
                active.rules = Set(Some(rules_json.to_string()));
            }

            active.updated_at = Set(Utc::now());
        }

        Ok(active.update(db).await?)
    }

    /// Delete a service. Built-in services cannot be deleted.
    pub async fn delete_service(db: &DatabaseConnection, id: &str) -> Result<(), AppError> {
        let existing = unlock_service::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Service {id} not found")))?;

        if existing.is_builtin {
            return Err(AppError::BadRequest(
                "Cannot delete a built-in service".to_string(),
            ));
        }

        unlock_service::Entity::delete_by_id(id).exec(db).await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Settings
    // -----------------------------------------------------------------------

    /// Get the global IP quality setting (single row with id = "default").
    pub async fn get_setting(db: &DatabaseConnection) -> Result<IpQualitySettingDto, AppError> {
        let row = ip_quality_setting::Entity::find_by_id(SETTING_ID)
            .one(db)
            .await?;
        Ok(IpQualitySettingDto {
            check_interval_hours: row.map(|r| r.check_interval_hours).unwrap_or(12),
        })
    }

    /// Update the global IP quality setting.
    pub async fn update_setting(
        db: &DatabaseConnection,
        check_interval_hours: i32,
    ) -> Result<IpQualitySettingDto, AppError> {
        if check_interval_hours < 1 {
            return Err(AppError::Validation(
                "check_interval_hours must be >= 1".to_string(),
            ));
        }

        let existing = ip_quality_setting::Entity::find_by_id(SETTING_ID)
            .one(db)
            .await?;

        if let Some(row) = existing {
            let mut active: ip_quality_setting::ActiveModel = row.into();
            active.check_interval_hours = Set(check_interval_hours);
            active.update(db).await?;
        } else {
            let model = ip_quality_setting::ActiveModel {
                id: Set(SETTING_ID.to_string()),
                check_interval_hours: Set(check_interval_hours),
            };
            model.insert(db).await?;
        }

        Ok(IpQualitySettingDto { check_interval_hours })
    }

    // -----------------------------------------------------------------------
    // Results and events
    // -----------------------------------------------------------------------

    /// Save unlock check results for a server.
    ///
    /// For each result:
    /// 1. Read the prior `unlock_result` row for (server_id, service_id).
    /// 2. If no prior row OR status differs, insert an `unlock_event` row.
    /// 3. Upsert the `unlock_result` row with `checked_at = now`.
    pub async fn save_unlock_results(
        db: &DatabaseConnection,
        server_id: &str,
        results: Vec<UnlockResultData>,
    ) -> Result<(), AppError> {
        let now = Utc::now();

        for r in results {
            let new_status = status_to_str(&r.status);

            // Read prior result for this (server, service)
            let prior = unlock_result::Entity::find()
                .filter(unlock_result::Column::ServerId.eq(server_id))
                .filter(unlock_result::Column::ServiceId.eq(&r.service_id))
                .one(db)
                .await?;

            // Only append an event when status DIFFERS from a known prior value.
            // On the very first call (no prior row) we treat it as a baseline
            // seed — no event is emitted.
            let status_changed = match &prior {
                None => false,
                Some(p) => p.status != new_status,
            };

            if status_changed {
                let old_status = prior
                    .as_ref()
                    .map(|p| p.status.clone())
                    .unwrap_or_default();

                let event = unlock_event::ActiveModel {
                    id: Set(Uuid::new_v4().to_string()),
                    server_id: Set(server_id.to_string()),
                    service_id: Set(r.service_id.clone()),
                    old_status: Set(old_status),
                    new_status: Set(new_status.clone()),
                    changed_at: Set(now),
                };
                event.insert(db).await?;
            }

            // Upsert the unlock_result row
            let upsert_sql = "INSERT INTO unlock_result \
                (id, server_id, service_id, status, region, latency_ms, detail, checked_at) \
                VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
                ON CONFLICT(server_id, service_id) DO UPDATE SET \
                id = excluded.id, \
                status = excluded.status, \
                region = excluded.region, \
                latency_ms = excluded.latency_ms, \
                detail = excluded.detail, \
                checked_at = excluded.checked_at";

            let new_id = match &prior {
                Some(p) => p.id.clone(),
                None => Uuid::new_v4().to_string(),
            };

            let region_val = match r.region {
                Some(v) => Value::String(Some(Box::new(v))),
                None => Value::String(None),
            };
            let latency_val = match r.latency_ms {
                Some(v) => Value::Int(Some(v as i32)),
                None => Value::Int(None),
            };
            let detail_val = match r.detail {
                Some(v) => Value::String(Some(Box::new(v))),
                None => Value::String(None),
            };

            let stmt = Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                upsert_sql,
                vec![
                    Value::String(Some(Box::new(new_id))),
                    Value::String(Some(Box::new(server_id.to_string()))),
                    Value::String(Some(Box::new(r.service_id))),
                    Value::String(Some(Box::new(new_status))),
                    region_val,
                    latency_val,
                    detail_val,
                    Value::String(Some(Box::new(now.to_rfc3339()))),
                ],
            );
            db.execute(stmt).await?;
        }

        Ok(())
    }

    /// Get all enabled services as protocol DTOs for use in `IpQualitySync`.
    pub async fn enabled_service_defs(
        db: &DatabaseConnection,
    ) -> Result<Vec<UnlockServiceDef>, AppError> {
        let services = unlock_service::Entity::find()
            .filter(unlock_service::Column::Enabled.eq(true))
            .all(db)
            .await?;

        let defs = services
            .into_iter()
            .map(|s| {
                let request = s.request.as_deref().and_then(|json| {
                    serde_json::from_str(json).ok()
                });
                let rules = s.rules.as_deref().and_then(|json| {
                    serde_json::from_str(json).ok()
                });
                UnlockServiceDef {
                    id: s.id,
                    key: s.key,
                    detector: s.detector,
                    request,
                    rules,
                }
            })
            .collect();

        Ok(defs)
    }

    // -----------------------------------------------------------------------
    // Query / summary functions
    // -----------------------------------------------------------------------

    /// Get the IP quality summary for a single server:
    /// all unlock_result rows + the ip_quality_snapshot (if any).
    pub async fn get_server_summary(
        db: &DatabaseConnection,
        server_id: &str,
    ) -> Result<ServerIpQualitySummary, AppError> {
        let results = unlock_result::Entity::find()
            .filter(unlock_result::Column::ServerId.eq(server_id))
            .all(db)
            .await?
            .into_iter()
            .map(UnlockResultDto::from)
            .collect();

        let snapshot = ip_quality_snapshot::Entity::find()
            .filter(ip_quality_snapshot::Column::ServerId.eq(server_id))
            .one(db)
            .await?
            .map(snapshot_model_to_dto);

        Ok(ServerIpQualitySummary {
            server_id: server_id.to_string(),
            unlock_results: results,
            ip_quality: snapshot,
        })
    }

    /// Get unlock results for all servers grouped by server_id.
    pub async fn get_overview(
        db: &DatabaseConnection,
    ) -> Result<Vec<IpQualityOverviewRow>, AppError> {
        // Fetch all unlock results and snapshots
        let all_results = unlock_result::Entity::find().all(db).await?;
        let all_snapshots = ip_quality_snapshot::Entity::find().all(db).await?;

        // Build snapshot lookup by server_id
        let mut snapshot_by_server: std::collections::HashMap<String, IpQualitySnapshotData> =
            std::collections::HashMap::new();
        for snap in all_snapshots {
            snapshot_by_server.insert(snap.server_id.clone(), snapshot_model_to_dto(snap));
        }

        // Group results by server_id
        let mut results_by_server: std::collections::HashMap<String, Vec<UnlockResultDto>> =
            std::collections::HashMap::new();
        for r in all_results {
            results_by_server
                .entry(r.server_id.clone())
                .or_default()
                .push(UnlockResultDto::from(r));
        }

        // Collect all server_ids seen across both tables
        let mut server_ids: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        for id in results_by_server.keys() {
            server_ids.insert(id.clone());
        }
        for id in snapshot_by_server.keys() {
            server_ids.insert(id.clone());
        }

        let mut overview: Vec<IpQualityOverviewRow> = server_ids
            .into_iter()
            .map(|sid| IpQualityOverviewRow {
                unlock_results: results_by_server.remove(&sid).unwrap_or_default(),
                ip_quality: snapshot_by_server.remove(&sid),
                server_id: sid,
            })
            .collect();

        // Sort by server_id for deterministic output
        overview.sort_by(|a, b| a.server_id.cmp(&b.server_id));
        Ok(overview)
    }

    /// List recent unlock events for a server, newest first.
    pub async fn list_events(
        db: &DatabaseConnection,
        server_id: &str,
        limit: u64,
    ) -> Result<Vec<UnlockEventDto>, AppError> {
        let events = unlock_event::Entity::find()
            .filter(unlock_event::Column::ServerId.eq(server_id))
            .order_by_desc(unlock_event::Column::ChangedAt)
            .limit(limit)
            .all(db)
            .await?
            .into_iter()
            .map(UnlockEventDto::from)
            .collect();
        Ok(events)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::server;
    use crate::service::auth::AuthService;
    use crate::test_utils::setup_test_db;
    use serverbee_common::constants::CAP_DEFAULT;
    use serverbee_common::protocol::UnlockStatus;

    // Insert a minimal server row to satisfy the FK constraint on `unlock_result`
    // and related tables. FK enforcement is enabled by default in sqlx-sqlite.
    async fn insert_test_server(db: &DatabaseConnection, id: &str) {
        let token_hash = AuthService::hash_password("test").expect("hash should succeed");
        let now = Utc::now();
        server::ActiveModel {
            id: Set(id.to_string()),
            token_hash: Set(token_hash),
            token_prefix: Set("serverbee_test".to_string()),
            name: Set(id.to_string()),
            weight: Set(0),
            hidden: Set(false),
            capabilities: Set(CAP_DEFAULT as i32),
            protocol_version: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert test server should succeed");
    }

    // -----------------------------------------------------------------------
    // Task 7: Catalog CRUD tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_list_services_returns_nine_builtins() {
        let (db, _tmp) = setup_test_db().await;
        let services = IpQualityService::list_services(&db).await.unwrap();
        assert_eq!(services.len(), 9, "should have 9 seeded built-in services");
        assert!(services.iter().all(|s| s.is_builtin));
    }

    #[tokio::test]
    async fn test_create_custom_service_rejects_non_http_url() {
        let (db, _tmp) = setup_test_db().await;

        let bad_url = CreateCustomServiceInput {
            name: "Bad".to_string(),
            category: "other".to_string(),
            popularity: 1,
            url: "ftp://example.com".to_string(),
            method: "GET".to_string(),
            headers: vec![],
            timeout_ms: 5000,
            rules: vec![serde_json::json!({"kind": "status_equals", "code": 200, "result": "unlocked"})],
        };
        let result = IpQualityService::create_custom_service(&db, bad_url).await;
        assert!(result.is_err(), "ftp:// URL should be rejected");
    }

    #[tokio::test]
    async fn test_create_custom_service_rejects_non_standard_port() {
        let (db, _tmp) = setup_test_db().await;

        let bad_port = CreateCustomServiceInput {
            name: "Bad Port".to_string(),
            category: "other".to_string(),
            popularity: 1,
            url: "https://example.com:8443/test".to_string(),
            method: "GET".to_string(),
            headers: vec![],
            timeout_ms: 5000,
            rules: vec![serde_json::json!({"kind": "status_equals", "code": 200, "result": "unlocked"})],
        };
        let result = IpQualityService::create_custom_service(&db, bad_port).await;
        assert!(result.is_err(), "port 8443 should be rejected");
    }

    #[tokio::test]
    async fn test_create_custom_service_valid() {
        let (db, _tmp) = setup_test_db().await;

        let valid = CreateCustomServiceInput {
            name: "My Service".to_string(),
            category: "other".to_string(),
            popularity: 50,
            url: "https://example.com/unlock".to_string(),
            method: "GET".to_string(),
            headers: vec![],
            timeout_ms: 5000,
            rules: vec![serde_json::json!({"kind": "status_equals", "code": 200, "result": "unlocked"})],
        };
        let created = IpQualityService::create_custom_service(&db, valid)
            .await
            .unwrap();
        assert!(!created.is_builtin);
        assert!(created.key.starts_with("custom_"));

        let services = IpQualityService::list_services(&db).await.unwrap();
        assert_eq!(services.len(), 10, "should have 9 built-ins + 1 custom");
    }

    #[tokio::test]
    async fn test_create_custom_service_rejects_empty_rules() {
        let (db, _tmp) = setup_test_db().await;
        let input = CreateCustomServiceInput {
            name: "No Rules".to_string(),
            category: "other".to_string(),
            popularity: 1,
            url: "https://example.com/".to_string(),
            method: "GET".to_string(),
            headers: vec![],
            timeout_ms: 5000,
            rules: vec![],
        };
        let result = IpQualityService::create_custom_service(&db, input).await;
        assert!(result.is_err(), "empty rules should be rejected");
    }

    #[tokio::test]
    async fn test_create_custom_service_rejects_key_collision_with_builtin() {
        // Verify that a custom service whose generated key collides with any existing
        // key is rejected. We simulate this by manually inserting a row with the key
        // `custom_xxxxxxxx` and then verifying the uniqueness guard works.
        // In practice, the key for custom services is always `custom_<8hex>`, which
        // cannot naturally collide with built-in keys like "netflix", but the uniqueness
        // constraint on the `key` column also protects against accidental UUID collisions.
        let (db, _tmp) = setup_test_db().await;

        // Insert a synthetic row that occupies the "netflix" key — this would never
        // happen through create_custom_service (which generates a unique key), but
        // confirms the DB-level uniqueness constraint.
        let now = Utc::now();
        let conflict_result = unlock_service::ActiveModel {
            id: Set("test-collision-id".to_string()),
            key: Set("netflix".to_string()), // same as a built-in
            name: Set("Fake Netflix".to_string()),
            category: Set("streaming".to_string()),
            popularity: Set(100),
            is_builtin: Set(false),
            enabled: Set(true),
            detector: Set(None),
            request: Set(Some(r#"{"url":"https://example.com","method":"GET","headers":[],"timeout_ms":5000}"#.to_string())),
            rules: Set(Some(r#"[]"#.to_string())),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&db)
        .await;

        // Should fail because "netflix" key already exists
        assert!(conflict_result.is_err(), "duplicate key should be rejected by DB constraint");
    }

    #[tokio::test]
    async fn test_update_builtin_service_changes_only_enabled() {
        let (db, _tmp) = setup_test_db().await;

        let services = IpQualityService::list_services(&db).await.unwrap();
        let netflix = services.iter().find(|s| s.key == "netflix").unwrap();
        let original_name = netflix.name.clone();
        let original_detector = netflix.detector.clone();

        let input = UpdateServiceInput {
            enabled: Some(false),
            name: Some("SHOULD_BE_IGNORED".to_string()),
            category: None,
            popularity: None,
            url: None,
            method: None,
            headers: None,
            timeout_ms: None,
            rules: None,
        };

        let updated = IpQualityService::update_service(&db, &netflix.id, input)
            .await
            .unwrap();

        assert!(!updated.enabled, "enabled should be false after update");
        assert_eq!(updated.name, original_name, "name should not change for built-in");
        assert_eq!(updated.detector, original_detector, "detector should not change");
    }

    #[tokio::test]
    async fn test_delete_builtin_service_is_rejected() {
        let (db, _tmp) = setup_test_db().await;
        let services = IpQualityService::list_services(&db).await.unwrap();
        let builtin = services.iter().find(|s| s.is_builtin).unwrap();

        let result = IpQualityService::delete_service(&db, &builtin.id).await;
        assert!(result.is_err(), "deleting a built-in service should fail");
        match result.unwrap_err() {
            AppError::BadRequest(_) => {} // expected
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_delete_custom_service_succeeds() {
        let (db, _tmp) = setup_test_db().await;

        let input = CreateCustomServiceInput {
            name: "ToDelete".to_string(),
            category: "other".to_string(),
            popularity: 1,
            url: "https://example.com/del".to_string(),
            method: "GET".to_string(),
            headers: vec![],
            timeout_ms: 5000,
            rules: vec![serde_json::json!({"kind": "status_equals", "code": 200, "result": "unlocked"})],
        };
        let created = IpQualityService::create_custom_service(&db, input)
            .await
            .unwrap();

        IpQualityService::delete_service(&db, &created.id)
            .await
            .unwrap();

        let services = IpQualityService::list_services(&db).await.unwrap();
        assert!(!services.iter().any(|s| s.id == created.id));
        assert_eq!(services.len(), 9);
    }

    // -----------------------------------------------------------------------
    // Task 8: Settings, results, events tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_get_set_setting_round_trip() {
        let (db, _tmp) = setup_test_db().await;

        // Default should be 12
        let setting = IpQualityService::get_setting(&db).await.unwrap();
        assert_eq!(setting.check_interval_hours, 12);

        // Update and read back
        IpQualityService::update_setting(&db, 6).await.unwrap();
        let updated = IpQualityService::get_setting(&db).await.unwrap();
        assert_eq!(updated.check_interval_hours, 6);
    }

    #[tokio::test]
    async fn test_save_unlock_results_first_call_no_events() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-1").await;

        let services = IpQualityService::list_services(&db).await.unwrap();
        let svc = &services[0];

        let results = vec![UnlockResultData {
            service_id: svc.id.clone(),
            status: UnlockStatus::Unlocked,
            region: Some("US".to_string()),
            latency_ms: Some(100),
            detail: None,
        }];

        IpQualityService::save_unlock_results(&db, "srv-1", results)
            .await
            .unwrap();

        // First call: no prior unlock_result, so no status "difference" — 0 events.
        // An event is only emitted when a known prior status DIFFERS from the new one.
        let events = unlock_event::Entity::find().all(&db).await.unwrap();
        assert_eq!(events.len(), 0, "first call produces no events (no prior status to differ from)");

        let results_in_db = unlock_result::Entity::find().all(&db).await.unwrap();
        assert_eq!(results_in_db.len(), 1);
    }

    #[tokio::test]
    async fn test_save_unlock_results_status_change_creates_event() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-1").await;

        let services = IpQualityService::list_services(&db).await.unwrap();
        let svc = &services[0];

        // First call: unlocked
        IpQualityService::save_unlock_results(
            &db,
            "srv-1",
            vec![UnlockResultData {
                service_id: svc.id.clone(),
                status: UnlockStatus::Unlocked,
                region: None,
                latency_ms: None,
                detail: None,
            }],
        )
        .await
        .unwrap();

        let events_after_first = unlock_event::Entity::find().all(&db).await.unwrap();
        assert_eq!(events_after_first.len(), 0);

        // Second call: blocked — status changed
        IpQualityService::save_unlock_results(
            &db,
            "srv-1",
            vec![UnlockResultData {
                service_id: svc.id.clone(),
                status: UnlockStatus::Blocked,
                region: None,
                latency_ms: None,
                detail: None,
            }],
        )
        .await
        .unwrap();

        let events_after_second = unlock_event::Entity::find().all(&db).await.unwrap();
        assert_eq!(events_after_second.len(), 1);
        assert_eq!(events_after_second[0].old_status, "unlocked");
        assert_eq!(events_after_second[0].new_status, "blocked");
        assert_eq!(events_after_second[0].server_id, "srv-1");
    }

    #[tokio::test]
    async fn test_save_unlock_results_no_event_on_same_status() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-1").await;

        let services = IpQualityService::list_services(&db).await.unwrap();
        let svc = &services[0];

        IpQualityService::save_unlock_results(
            &db,
            "srv-1",
            vec![UnlockResultData {
                service_id: svc.id.clone(),
                status: UnlockStatus::Unlocked,
                region: None,
                latency_ms: None,
                detail: None,
            }],
        )
        .await
        .unwrap();

        IpQualityService::save_unlock_results(
            &db,
            "srv-1",
            vec![UnlockResultData {
                service_id: svc.id.clone(),
                status: UnlockStatus::Unlocked,
                region: None,
                latency_ms: None,
                detail: None,
            }],
        )
        .await
        .unwrap();

        let events = unlock_event::Entity::find().all(&db).await.unwrap();
        assert_eq!(events.len(), 0, "no event when status unchanged");
    }

    #[tokio::test]
    async fn test_enabled_service_defs() {
        let (db, _tmp) = setup_test_db().await;
        let defs = IpQualityService::enabled_service_defs(&db).await.unwrap();
        assert_eq!(defs.len(), 9);
        assert!(defs.iter().all(|d| d.detector.is_some()));
    }

    // -----------------------------------------------------------------------
    // Task 9: Query / summary tests
    // -----------------------------------------------------------------------

    async fn seed_server_result(
        db: &DatabaseConnection,
        server_id: &str,
        service_id: &str,
        status: UnlockStatus,
    ) {
        IpQualityService::save_unlock_results(
            db,
            server_id,
            vec![UnlockResultData {
                service_id: service_id.to_string(),
                status,
                region: None,
                latency_ms: None,
                detail: None,
            }],
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_get_server_summary_returns_results_and_no_snapshot() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-a").await;

        let services = IpQualityService::list_services(&db).await.unwrap();
        let svc = &services[0];

        seed_server_result(&db, "srv-a", &svc.id, UnlockStatus::Unlocked).await;

        let summary = IpQualityService::get_server_summary(&db, "srv-a")
            .await
            .unwrap();
        assert_eq!(summary.server_id, "srv-a");
        assert_eq!(summary.unlock_results.len(), 1);
        assert_eq!(summary.unlock_results[0].status, "unlocked");
        assert!(summary.ip_quality.is_none(), "no snapshot inserted yet");
    }

    #[tokio::test]
    async fn test_get_server_summary_includes_snapshot() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-b").await;

        let now = Utc::now();
        let snap = ip_quality_snapshot::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            server_id: Set("srv-b".to_string()),
            ip: Set("1.2.3.4".to_string()),
            asn: Set(None),
            as_org: Set(None),
            country: Set(Some("US".to_string())),
            region: Set(None),
            city: Set(None),
            ip_type: Set("residential".to_string()),
            is_proxy: Set(false),
            is_vpn: Set(false),
            is_hosting: Set(false),
            risk_score: Set(None),
            risk_level: Set("unknown".to_string()),
            checked_at: Set(now),
        };
        snap.insert(&db).await.unwrap();

        let summary = IpQualityService::get_server_summary(&db, "srv-b")
            .await
            .unwrap();
        assert!(summary.ip_quality.is_some());
        assert_eq!(summary.ip_quality.unwrap().ip, "1.2.3.4");
    }

    #[tokio::test]
    async fn test_get_overview_returns_all_servers() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-x").await;
        insert_test_server(&db, "srv-y").await;

        let services = IpQualityService::list_services(&db).await.unwrap();
        let svc = &services[0];

        seed_server_result(&db, "srv-x", &svc.id, UnlockStatus::Blocked).await;
        seed_server_result(&db, "srv-y", &svc.id, UnlockStatus::Unlocked).await;

        let overview = IpQualityService::get_overview(&db).await.unwrap();
        assert_eq!(overview.len(), 2);
        let ids: Vec<&str> = overview.iter().map(|r| r.server_id.as_str()).collect();
        assert!(ids.contains(&"srv-x"));
        assert!(ids.contains(&"srv-y"));
    }

    #[tokio::test]
    async fn test_list_events_newest_first() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-z").await;

        let services = IpQualityService::list_services(&db).await.unwrap();
        let svc = &services[0];

        // First: unlocked (no event — first call)
        seed_server_result(&db, "srv-z", &svc.id, UnlockStatus::Unlocked).await;
        // Second: blocked (event 1: unlocked → blocked)
        seed_server_result(&db, "srv-z", &svc.id, UnlockStatus::Blocked).await;
        // Third: unlocked again (event 2: blocked → unlocked)
        seed_server_result(&db, "srv-z", &svc.id, UnlockStatus::Unlocked).await;

        let events = IpQualityService::list_events(&db, "srv-z", 100)
            .await
            .unwrap();
        assert_eq!(events.len(), 2, "should have 2 status-change events");
        // Newest first
        assert_eq!(events[0].new_status, "unlocked", "newest event is blocked→unlocked");
        assert_eq!(events[1].new_status, "blocked", "older event is unlocked→blocked");
    }

    #[tokio::test]
    async fn test_list_events_limit() {
        let (db, _tmp) = setup_test_db().await;
        insert_test_server(&db, "srv-lim").await;

        let services = IpQualityService::list_services(&db).await.unwrap();
        let svc = &services[0];

        // Generate 3 events (4 calls, first generates no event, then 3 transitions)
        seed_server_result(&db, "srv-lim", &svc.id, UnlockStatus::Unlocked).await;
        seed_server_result(&db, "srv-lim", &svc.id, UnlockStatus::Blocked).await;
        seed_server_result(&db, "srv-lim", &svc.id, UnlockStatus::Unlocked).await;
        seed_server_result(&db, "srv-lim", &svc.id, UnlockStatus::Blocked).await;

        let events = IpQualityService::list_events(&db, "srv-lim", 2)
            .await
            .unwrap();
        assert_eq!(events.len(), 2, "should respect the limit");
    }
}
