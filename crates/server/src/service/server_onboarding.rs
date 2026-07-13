use std::sync::Arc;

use base64::Engine;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, TransactionTrait,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::entity::{server, server_onboarding_request, server_tag};
use crate::error::AppError;
use crate::service::agent_authority::{
    Actor, AgentAuthority, IssuedOffer, OfferTtl, OutstandingOffer, RequestSource, ServerId,
    StateError,
};
use crate::service::network_probe::NetworkProbeService;
use crate::service::server_tag as server_tag_service;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct OnboardingRequestId(String);

impl OnboardingRequestId {
    pub fn parse(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() || value.len() > 128 || value.chars().any(char::is_whitespace) {
            return Err(
                "onboarding request id must be 1-128 non-whitespace characters".to_string(),
            );
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ServerProfile {
    pub name: String,
    pub group_id: Option<String>,
    pub tags: Vec<String>,
    pub remark: Option<String>,
    pub public_remark: Option<String>,
    pub price: Option<f64>,
    pub currency: Option<String>,
    pub billing_cycle: Option<String>,
    pub billing_start_day: Option<i32>,
    pub expired_at: Option<DateTime<Utc>>,
    pub traffic_limit: Option<i64>,
    pub traffic_limit_type: Option<String>,
}

#[derive(Clone, Debug)]
pub struct OnboardServer {
    pub actor_id: String,
    pub request_id: OnboardingRequestId,
    pub source: RequestSource,
    pub profile: ServerProfile,
    pub offer_ttl: OfferTtl,
}

#[derive(Clone, Debug)]
pub enum OnboardingResult {
    Created {
        server_id: ServerId,
        enrollment: IssuedOffer,
    },
    Replayed {
        server_id: ServerId,
        outstanding_offer: Option<OutstandingOffer>,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum OnboardingError {
    #[error("invalid onboarding input: {0}")]
    Invalid(String),
    #[error("onboarding validation failed: {0}")]
    Validation(String),
    #[error("server limit reached ({0})")]
    LimitReached(u32),
    #[error("onboarding request id was already used with different input")]
    IdempotencyConflict,
    #[error("server onboarding store failed: {0}")]
    Store(#[from] AppError),
}

#[derive(Clone)]
pub struct ServerOnboarding {
    db: DatabaseConnection,
    authority: Arc<AgentAuthority>,
    max_servers: u32,
    request_locks: Arc<DashMap<String, Arc<Mutex<()>>>>,
}

struct RequestLockCleanup {
    key: String,
    lock: std::sync::Weak<Mutex<()>>,
    registry: Arc<DashMap<String, Arc<Mutex<()>>>>,
}

impl Drop for RequestLockCleanup {
    fn drop(&mut self) {
        self.registry.remove_if(&self.key, |_, current| {
            std::sync::Weak::ptr_eq(&Arc::downgrade(current), &self.lock)
                && self.lock.strong_count() == 2
        });
    }
}

impl ServerOnboarding {
    pub fn new(db: DatabaseConnection, authority: Arc<AgentAuthority>, max_servers: u32) -> Self {
        Self {
            db,
            authority,
            max_servers,
            request_locks: Arc::new(DashMap::new()),
        }
    }

    pub async fn onboard(&self, input: OnboardServer) -> Result<OnboardingResult, OnboardingError> {
        let normalized = NormalizedProfile::from_input(input.profile)?;
        let input_hash = normalized.hash(input.offer_ttl)?;
        if input.actor_id.trim().is_empty() {
            return Err(OnboardingError::Invalid(
                "actor id must not be empty".to_string(),
            ));
        }

        let lock_key = format!("{}:{}", input.actor_id, input.request_id.as_str());
        let request_lock = self
            .request_locks
            .entry(lock_key.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let _cleanup = RequestLockCleanup {
            key: lock_key,
            lock: Arc::downgrade(&request_lock),
            registry: self.request_locks.clone(),
        };
        let _guard = request_lock.lock().await;

        if let Some(existing) = self
            .find_request(&input.actor_id, input.request_id.as_str())
            .await?
        {
            if existing.normalized_input_hash != input_hash {
                return Err(OnboardingError::IdempotencyConflict);
            }
            let server_id = ServerId::parse(existing.server_id).map_err(|error| {
                AppError::Internal(format!("invalid stored server id: {error}"))
            })?;
            let state = self
                .authority
                .state(server_id.clone())
                .await
                .map_err(map_state_error)?;
            return Ok(OnboardingResult::Replayed {
                server_id,
                outstanding_offer: state.outstanding_offer,
            });
        }

        let default_target_ids = NetworkProbeService::get_setting(&self.db)
            .await?
            .default_target_ids;
        let tx = self.db.begin().await.map_err(AppError::from)?;
        if self.max_servers > 0 {
            let count = server::Entity::find().count(&tx).await?;
            if count >= u64::from(self.max_servers) {
                tx.rollback().await.map_err(AppError::from)?;
                return Err(OnboardingError::LimitReached(self.max_servers));
            }
        }

        let server_id = ServerId::parse(Uuid::new_v4().to_string())
            .map_err(|error| AppError::Internal(format!("generated invalid server id: {error}")))?;
        let now = Utc::now();
        let row = server::ActiveModel {
            id: Set(server_id.as_str().to_string()),
            token_hash: Set(None),
            token_prefix: Set(None),
            name: Set(normalized.name.clone()),
            cpu_name: Set(None),
            cpu_cores: Set(None),
            cpu_arch: Set(None),
            os: Set(None),
            kernel_version: Set(None),
            mem_total: Set(None),
            swap_total: Set(None),
            disk_total: Set(None),
            ipv4: Set(None),
            ipv6: Set(None),
            region: Set(None),
            country_code: Set(None),
            geo_manual: Set(false),
            virtualization: Set(None),
            agent_version: Set(None),
            group_id: Set(normalized.group_id.clone()),
            weight: Set(0),
            hidden: Set(false),
            remark: Set(normalized.remark.clone()),
            public_remark: Set(normalized.public_remark.clone()),
            price: Set(normalized.price),
            billing_cycle: Set(normalized.billing_cycle.clone()),
            currency: Set(normalized.currency.clone()),
            expired_at: Set(normalized.expired_at),
            traffic_limit: Set(normalized.traffic_limit),
            traffic_limit_type: Set(normalized.traffic_limit_type.clone()),
            billing_start_day: Set(normalized.billing_start_day),
            capabilities: Set(serverbee_common::constants::CAP_DEFAULT as i32),
            protocol_version: Set(1),
            features: Set("[]".to_string()),
            last_remote_addr: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&tx)
        .await?;

        for tag in &normalized.tags {
            server_tag::ActiveModel {
                server_id: Set(server_id.as_str().to_string()),
                tag: Set(tag.clone()),
            }
            .insert(&tx)
            .await?;
        }
        NetworkProbeService::apply_defaults_tx(&tx, server_id.as_str(), &default_target_ids)
            .await?;
        let actor = Actor::User {
            id: input.actor_id.clone(),
        };
        let enrollment = self
            .authority
            .issue_initial_offer_tx(&tx, &row, &actor, &input.source, input.offer_ttl)
            .await?;
        server_onboarding_request::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            actor_id: Set(input.actor_id),
            request_id: Set(input.request_id.as_str().to_string()),
            normalized_input_hash: Set(input_hash),
            server_id: Set(server_id.as_str().to_string()),
            created_at: Set(now),
        }
        .insert(&tx)
        .await?;
        tx.commit().await.map_err(AppError::from)?;
        self.authority.broadcast_issued_offer_state(
            server_id.as_str(),
            crate::service::agent_authority::AuthorityStatus::Unclaimed,
            &enrollment,
        );

        Ok(OnboardingResult::Created {
            server_id,
            enrollment,
        })
    }

    async fn find_request(
        &self,
        actor_id: &str,
        request_id: &str,
    ) -> Result<Option<server_onboarding_request::Model>, AppError> {
        Ok(server_onboarding_request::Entity::find()
            .filter(server_onboarding_request::Column::ActorId.eq(actor_id))
            .filter(server_onboarding_request::Column::RequestId.eq(request_id))
            .one(&self.db)
            .await?)
    }
}

#[derive(Serialize)]
struct NormalizedProfile {
    name: String,
    group_id: Option<String>,
    tags: Vec<String>,
    remark: Option<String>,
    public_remark: Option<String>,
    price: Option<f64>,
    currency: Option<String>,
    billing_cycle: Option<String>,
    billing_start_day: Option<i32>,
    expired_at: Option<DateTime<Utc>>,
    traffic_limit: Option<i64>,
    traffic_limit_type: Option<String>,
}

impl NormalizedProfile {
    fn from_input(profile: ServerProfile) -> Result<Self, OnboardingError> {
        let name = profile.name.trim().to_string();
        if name.is_empty() {
            return Err(OnboardingError::Invalid("name is required".to_string()));
        }
        if profile
            .price
            .is_some_and(|price| !price.is_finite() || price < 0.0)
        {
            return Err(OnboardingError::Invalid(
                "price must be finite and greater than or equal to 0".to_string(),
            ));
        }
        if profile
            .billing_cycle
            .as_deref()
            .is_some_and(|cycle| !matches!(cycle, "monthly" | "quarterly" | "yearly"))
        {
            return Err(OnboardingError::Invalid(
                "billing_cycle must be monthly, quarterly, or yearly".to_string(),
            ));
        }
        if profile
            .traffic_limit_type
            .as_deref()
            .is_some_and(|kind| !matches!(kind, "sum" | "up" | "down"))
        {
            return Err(OnboardingError::Invalid(
                "traffic_limit_type must be sum, up, or down".to_string(),
            ));
        }
        if profile
            .billing_start_day
            .is_some_and(|day| !(1..=28).contains(&day))
        {
            return Err(OnboardingError::Invalid(
                "billing_start_day must be between 1 and 28".to_string(),
            ));
        }

        Ok(Self {
            name,
            group_id: normalize_optional(profile.group_id),
            tags: server_tag_service::validate_tags(&profile.tags)
                .map_err(|error| OnboardingError::Validation(error.to_string()))?,
            remark: normalize_optional(profile.remark),
            public_remark: normalize_optional(profile.public_remark),
            price: profile.price,
            currency: normalize_optional(profile.currency),
            billing_cycle: normalize_optional(profile.billing_cycle),
            billing_start_day: profile.billing_start_day,
            expired_at: profile.expired_at,
            traffic_limit: profile.traffic_limit,
            traffic_limit_type: normalize_optional(profile.traffic_limit_type),
        })
    }

    fn hash(&self, offer_ttl: OfferTtl) -> Result<String, AppError> {
        let canonical = serde_json::to_vec(&NormalizedRequest {
            profile: self,
            offer_ttl_seconds: offer_ttl.value(),
        })
        .map_err(|error| AppError::Internal(format!("serialize onboarding input: {error}")))?;
        Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(canonical)))
    }
}

#[derive(Serialize)]
struct NormalizedRequest<'a> {
    profile: &'a NormalizedProfile,
    offer_ttl_seconds: i64,
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    })
}

fn map_state_error(error: StateError) -> OnboardingError {
    match error {
        StateError::NotFound => OnboardingError::Store(AppError::Internal(
            "idempotent onboarding target no longer exists".to_string(),
        )),
        StateError::Store(error) => OnboardingError::Store(error),
    }
}

impl From<sea_orm::DbErr> for OnboardingError {
    fn from(error: sea_orm::DbErr) -> Self {
        Self::Store(AppError::from(error))
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::{ConnectionTrait, EntityTrait, PaginatorTrait};
    use tokio::sync::broadcast;

    use super::*;
    use crate::entity::{agent_authority_event, enrollment_offer};
    use crate::service::agent_manager::AgentManager;
    use crate::test_utils::setup_test_db;

    struct Fixture {
        onboarding: ServerOnboarding,
        db: DatabaseConnection,
        _tmp: tempfile::TempDir,
    }

    async fn fixture() -> Fixture {
        let (db, tmp) = setup_test_db().await;
        let (browser_tx, _) = broadcast::channel(8);
        let manager = Arc::new(AgentManager::new(browser_tx));
        let authority = Arc::new(AgentAuthority::new(db.clone(), manager));
        Fixture {
            onboarding: ServerOnboarding::new(db.clone(), authority, 0),
            db,
            _tmp: tmp,
        }
    }

    fn input(request_id: &str, name: &str) -> OnboardServer {
        OnboardServer {
            actor_id: "user-1".to_string(),
            request_id: OnboardingRequestId::parse(request_id).expect("request id"),
            source: RequestSource::parse("api:create-server").expect("source"),
            profile: ServerProfile {
                name: name.to_string(),
                group_id: None,
                tags: vec!["edge".to_string(), "prod".to_string()],
                remark: None,
                public_remark: None,
                price: None,
                currency: None,
                billing_cycle: None,
                billing_start_day: None,
                expired_at: None,
                traffic_limit: None,
                traffic_limit_type: None,
            },
            offer_ttl: OfferTtl::default(),
        }
    }

    #[tokio::test]
    async fn serial_replay_returns_same_server_without_plaintext() {
        let fixture = fixture().await;
        let first = fixture
            .onboarding
            .onboard(input("request-1", " Server One "))
            .await
            .expect("create");
        let second = fixture
            .onboarding
            .onboard(input("request-1", "Server One"))
            .await
            .expect("replay");

        let (created_id, offer_id) = match first {
            OnboardingResult::Created {
                server_id,
                enrollment,
            } => (server_id, enrollment.id),
            OnboardingResult::Replayed { .. } => panic!("first result must create"),
        };
        match second {
            OnboardingResult::Replayed {
                server_id,
                outstanding_offer,
            } => {
                assert_eq!(server_id, created_id);
                assert_eq!(outstanding_offer.map(|offer| offer.id), Some(offer_id));
            }
            OnboardingResult::Created { .. } => panic!("replay must not create"),
        }
        assert_eq!(
            server::Entity::find()
                .count(&fixture.db)
                .await
                .expect("count"),
            1
        );
    }

    #[tokio::test]
    async fn same_request_with_different_normalized_input_conflicts() {
        let fixture = fixture().await;
        fixture
            .onboarding
            .onboard(input("request-1", "Server One"))
            .await
            .expect("create");

        let result = fixture
            .onboarding
            .onboard(input("request-1", "Server Two"))
            .await;

        assert!(matches!(result, Err(OnboardingError::IdempotencyConflict)));
        assert_eq!(
            server::Entity::find()
                .count(&fixture.db)
                .await
                .expect("count"),
            1
        );
    }

    #[tokio::test]
    async fn concurrent_replay_creates_exactly_one_server() {
        let fixture = fixture().await;
        let first = fixture.onboarding.clone();
        let second = fixture.onboarding.clone();

        let (first, second) = tokio::join!(
            first.onboard(input("request-1", "Server One")),
            second.onboard(input("request-1", "Server One"))
        );

        assert!(first.is_ok());
        assert!(second.is_ok());
        assert_eq!(
            server::Entity::find()
                .count(&fixture.db)
                .await
                .expect("count"),
            1
        );
        assert_eq!(
            server_onboarding_request::Entity::find()
                .count(&fixture.db)
                .await
                .expect("count requests"),
            1
        );
        assert!(fixture.onboarding.request_locks.is_empty());
    }

    #[tokio::test]
    async fn authority_event_failure_rolls_back_entire_onboarding() {
        let fixture = fixture().await;
        fixture
            .db
            .execute_unprepared(
                "CREATE TRIGGER reject_authority_events BEFORE INSERT ON agent_authority_events \
                 BEGIN SELECT RAISE(ABORT, 'forced authority event failure'); END",
            )
            .await
            .expect("create failure trigger");

        let result = fixture
            .onboarding
            .onboard(input("request-1", "Server One"))
            .await;

        assert!(matches!(result, Err(OnboardingError::Store(_))));
        assert_eq!(
            server::Entity::find()
                .count(&fixture.db)
                .await
                .expect("servers"),
            0
        );
        assert_eq!(
            enrollment_offer::Entity::find()
                .count(&fixture.db)
                .await
                .expect("offers"),
            0
        );
        assert_eq!(
            agent_authority_event::Entity::find()
                .count(&fixture.db)
                .await
                .expect("events"),
            0
        );
        assert_eq!(
            server_onboarding_request::Entity::find()
                .count(&fixture.db)
                .await
                .expect("requests"),
            0
        );
    }
}
