mod model;

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{Duration, Utc};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    DatabaseTransaction, EntityTrait, QueryFilter, QueryOrder, QuerySelect, TransactionTrait,
};
use serverbee_common::types::{
    AgentAuthorityStateSummary, AgentAuthorityStatus, OutstandingEnrollmentSummary,
};
use uuid::Uuid;

use crate::entity::{agent_authority_event, enrollment_offer, server};
use crate::error::AppError;
use crate::service::agent_manager::AgentManager;
use crate::service::auth::AuthService;
use crate::service::server::ServerService;

pub use model::*;

#[derive(Clone)]
pub struct AgentAuthority {
    db: DatabaseConnection,
    agent_manager: Arc<AgentManager>,
}

impl AgentAuthority {
    pub fn new(db: DatabaseConnection, agent_manager: Arc<AgentManager>) -> Self {
        Self { db, agent_manager }
    }

    pub async fn issue_offer_for_unclaimed(
        &self,
        input: IssueOfferForUnclaimed,
    ) -> Result<IssuedOffer, IssueOfferError> {
        let server_lock = self
            .agent_manager
            .server_lifecycle_lock(input.server_id.as_str());
        let _guard = server_lock.lock().await;
        let tx = self.db.begin().await.map_err(AppError::from)?;
        let server = server::Entity::find_by_id(input.server_id.as_str())
            .one(&tx)
            .await
            .map_err(AppError::from)?
            .ok_or(IssueOfferError::NotFound)?;
        if authority_status(&server) == AuthorityStatus::Claimed {
            return Err(IssueOfferError::AlreadyClaimed);
        }

        expire_elapsed_outstanding(&tx, &server, &input.actor, &input.source).await?;
        if let Some(current) = find_outstanding(&tx, &server.id, Utc::now()).await? {
            return Err(IssueOfferError::OutstandingExists(to_outstanding(current)?));
        }

        let issued = mint_offer(&tx, &server.id, &input.actor, input.ttl).await?;
        insert_event(
            &tx,
            EventInput {
                server: &server,
                actor: &input.actor,
                source: &input.source,
                offer_id: Some(issued.id.as_str()),
                transition: AuthorityTransition::OfferIssued,
                mode: None,
                offer_outcome: None,
                authority_before: AuthorityStatus::Unclaimed,
                authority_after: AuthorityStatus::Unclaimed,
            },
        )
        .await?;
        tx.commit().await.map_err(AppError::from)?;
        self.broadcast_issued_offer_state(&server.id, AuthorityStatus::Unclaimed, &issued);
        Ok(issued)
    }

    pub async fn claim(&self, input: ClaimAgent) -> Result<ClaimReceipt, ClaimError> {
        let Some(candidate) = find_offer_for_code(&self.db, &input.code).await? else {
            return Err(ClaimError::Rejected);
        };
        let token_hash = AuthService::hash_password(input.proposed_run_token.expose())?;
        let token_prefix = input.proposed_run_token.expose()[..8].to_string();
        let server_lock = self
            .agent_manager
            .server_lifecycle_lock(&candidate.target_server_id);
        let _guard = server_lock.lock().await;
        let tx = self.db.begin().await.map_err(AppError::from)?;

        let Some(offer) = enrollment_offer::Entity::find_by_id(&candidate.id)
            .one(&tx)
            .await
            .map_err(AppError::from)?
        else {
            return Err(ClaimError::Rejected);
        };
        if offer.outcome.is_some()
            || !AuthService::verify_password(input.code.expose(), &offer.code_hash)?
        {
            return Err(ClaimError::Rejected);
        }
        let server = server::Entity::find_by_id(&offer.target_server_id)
            .one(&tx)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| AppError::Internal("enrollment offer target vanished".to_string()))?;
        let before = authority_status(&server);
        if offer.expires_at <= Utc::now() {
            terminalize_offer(
                &tx,
                offer,
                OfferOutcome::Expired,
                None,
                &server,
                &Actor::Agent,
                &input.source,
                before,
                before,
            )
            .await?;
            tx.commit().await.map_err(AppError::from)?;
            return Err(ClaimError::Rejected);
        }

        let server_id = server.id.clone();
        self.agent_manager.remove_connection(&server_id);
        let now = Utc::now();
        let offer_id = offer.id.clone();
        let mut active_offer: enrollment_offer::ActiveModel = offer.into();
        active_offer.outcome = Set(Some(OfferOutcome::Consumed.as_str().to_string()));
        active_offer.terminal_at = Set(Some(now));
        active_offer.successor_offer_id = Set(None);
        active_offer.update(&tx).await.map_err(AppError::from)?;

        let mut active_server: server::ActiveModel = server.clone().into();
        active_server.token_hash = Set(Some(token_hash));
        active_server.token_prefix = Set(Some(token_prefix));
        active_server.last_remote_addr = Set(input.remote_addr.clone());
        active_server.updated_at = Set(now);
        active_server.update(&tx).await.map_err(AppError::from)?;

        insert_event(
            &tx,
            EventInput {
                server: &server,
                actor: &Actor::Agent,
                source: &input.source,
                offer_id: Some(&offer_id),
                transition: AuthorityTransition::OfferConsumed,
                mode: None,
                offer_outcome: Some(OfferOutcome::Consumed),
                authority_before: before,
                authority_after: AuthorityStatus::Claimed,
            },
        )
        .await?;
        tx.commit().await.map_err(AppError::from)?;
        self.broadcast_authority_state(&server_id, AuthorityStatus::Claimed, None);

        Ok(ClaimReceipt {
            server_id: ServerId::parse(server_id).map_err(|error| {
                AppError::Internal(format!("invalid stored server id: {error}"))
            })?,
        })
    }

    pub async fn begin_reenrollment(
        &self,
        input: BeginReenrollment,
    ) -> Result<IssuedOffer, ReenrollmentError> {
        let server_lock = self
            .agent_manager
            .server_lifecycle_lock(input.server_id.as_str());
        let _guard = server_lock.lock().await;
        let tx = self.db.begin().await.map_err(AppError::from)?;
        let server = server::Entity::find_by_id(input.server_id.as_str())
            .one(&tx)
            .await
            .map_err(AppError::from)?
            .ok_or(ReenrollmentError::NotFound)?;
        if authority_status(&server) == AuthorityStatus::Unclaimed {
            return Err(ReenrollmentError::Unclaimed);
        }

        expire_elapsed_outstanding(&tx, &server, &input.actor, &input.source).await?;
        if let Some(current) = find_outstanding(&tx, &server.id, Utc::now()).await? {
            return Err(ReenrollmentError::OutstandingExists(to_outstanding(
                current,
            )?));
        }

        let authority_after = match input.mode {
            ReenrollmentMode::Graceful => AuthorityStatus::Claimed,
            ReenrollmentMode::Emergency => {
                self.agent_manager.remove_connection(&server.id);
                let mut active: server::ActiveModel = server.clone().into();
                active.token_hash = Set(None);
                active.token_prefix = Set(None);
                active.updated_at = Set(Utc::now());
                active.update(&tx).await.map_err(AppError::from)?;
                AuthorityStatus::Unclaimed
            }
        };
        let issued = mint_offer(&tx, &server.id, &input.actor, input.ttl).await?;
        insert_event(
            &tx,
            EventInput {
                server: &server,
                actor: &input.actor,
                source: &input.source,
                offer_id: Some(issued.id.as_str()),
                transition: AuthorityTransition::ReenrollmentStarted,
                mode: Some(input.mode),
                offer_outcome: None,
                authority_before: AuthorityStatus::Claimed,
                authority_after,
            },
        )
        .await?;
        tx.commit().await.map_err(AppError::from)?;
        self.broadcast_issued_offer_state(&server.id, authority_after, &issued);
        Ok(issued)
    }

    pub async fn replace_offer(
        &self,
        input: ReplaceOffer,
    ) -> Result<IssuedOffer, ReplaceOfferError> {
        let server_lock = self
            .agent_manager
            .server_lifecycle_lock(input.server_id.as_str());
        let _guard = server_lock.lock().await;
        let tx = self.db.begin().await.map_err(AppError::from)?;
        let server = server::Entity::find_by_id(input.server_id.as_str())
            .one(&tx)
            .await
            .map_err(AppError::from)?
            .ok_or(ReplaceOfferError::ServerNotFound)?;
        let offer = enrollment_offer::Entity::find_by_id(input.offer_id.as_str())
            .one(&tx)
            .await
            .map_err(AppError::from)?
            .filter(|offer| offer.target_server_id == server.id)
            .ok_or(ReplaceOfferError::OfferNotFound)?;

        if let Some(outcome) = offer.outcome.as_deref() {
            let current = find_outstanding(&tx, &server.id, Utc::now())
                .await?
                .map(to_outstanding)
                .transpose()?;
            return Err(ReplaceOfferError::NotOutstanding {
                outcome: parse_outcome(outcome)?,
                current,
            });
        }
        if offer.expires_at <= Utc::now() {
            let before = authority_status(&server);
            terminalize_offer(
                &tx,
                offer,
                OfferOutcome::Expired,
                None,
                &server,
                &input.actor,
                &input.source,
                before,
                before,
            )
            .await?;
            tx.commit().await.map_err(AppError::from)?;
            return Err(ReplaceOfferError::NotOutstanding {
                outcome: OfferOutcome::Expired,
                current: None,
            });
        }

        let current = find_outstanding(&tx, &server.id, Utc::now()).await?;
        if current.as_ref().map(|current| current.id.as_str()) != Some(input.offer_id.as_str()) {
            return Err(ReplaceOfferError::Stale {
                current: current.map(to_outstanding).transpose()?,
            });
        }

        let successor = prepare_offer(&server.id, &input.actor, input.ttl)?;
        let now = Utc::now();
        let old_offer_id = offer.id.clone();
        let mut active: enrollment_offer::ActiveModel = offer.into();
        active.outcome = Set(Some(OfferOutcome::Replaced.as_str().to_string()));
        active.terminal_at = Set(Some(now));
        active.successor_offer_id = Set(Some(successor.id.clone()));
        active.update(&tx).await.map_err(AppError::from)?;
        let issued = insert_prepared_offer(&tx, successor).await?;
        let status = authority_status(&server);
        insert_event(
            &tx,
            EventInput {
                server: &server,
                actor: &input.actor,
                source: &input.source,
                offer_id: Some(&old_offer_id),
                transition: AuthorityTransition::OfferReplaced,
                mode: None,
                offer_outcome: Some(OfferOutcome::Replaced),
                authority_before: status,
                authority_after: status,
            },
        )
        .await?;
        tx.commit().await.map_err(AppError::from)?;
        self.broadcast_issued_offer_state(&server.id, status, &issued);
        Ok(issued)
    }

    pub async fn revoke_offer(&self, input: RevokeOffer) -> Result<RevokedOffer, RevokeOfferError> {
        let server_lock = self
            .agent_manager
            .server_lifecycle_lock(input.server_id.as_str());
        let _guard = server_lock.lock().await;
        let tx = self.db.begin().await.map_err(AppError::from)?;
        let server = server::Entity::find_by_id(input.server_id.as_str())
            .one(&tx)
            .await
            .map_err(AppError::from)?
            .ok_or(RevokeOfferError::ServerNotFound)?;
        let offer = enrollment_offer::Entity::find_by_id(input.offer_id.as_str())
            .one(&tx)
            .await
            .map_err(AppError::from)?
            .filter(|offer| offer.target_server_id == server.id)
            .ok_or(RevokeOfferError::OfferNotFound)?;

        if let Some(outcome) = offer.outcome.as_deref() {
            let outcome = parse_outcome(outcome)?;
            if outcome == OfferOutcome::Revoked {
                return Ok(RevokedOffer {
                    offer_id: input.offer_id,
                    already_revoked: true,
                });
            }
            return Err(RevokeOfferError::Terminal(outcome));
        }

        let status = authority_status(&server);
        if offer.expires_at <= Utc::now() {
            terminalize_offer(
                &tx,
                offer,
                OfferOutcome::Expired,
                None,
                &server,
                &input.actor,
                &input.source,
                status,
                status,
            )
            .await?;
            tx.commit().await.map_err(AppError::from)?;
            self.broadcast_authority_state(&server.id, status, None);
            return Err(RevokeOfferError::Terminal(OfferOutcome::Expired));
        }

        terminalize_offer(
            &tx,
            offer,
            OfferOutcome::Revoked,
            None,
            &server,
            &input.actor,
            &input.source,
            status,
            status,
        )
        .await?;
        tx.commit().await.map_err(AppError::from)?;
        self.broadcast_authority_state(&server.id, status, None);
        Ok(RevokedOffer {
            offer_id: input.offer_id,
            already_revoked: false,
        })
    }

    pub async fn revoke_authority(
        &self,
        input: RevokeAuthority,
    ) -> Result<RevocationReceipt, RevokeAuthorityError> {
        let server_lock = self
            .agent_manager
            .server_lifecycle_lock(input.server_id.as_str());
        let _guard = server_lock.lock().await;
        let tx = self.db.begin().await.map_err(AppError::from)?;
        let server = server::Entity::find_by_id(input.server_id.as_str())
            .one(&tx)
            .await
            .map_err(AppError::from)?
            .ok_or(RevokeAuthorityError::NotFound)?;
        let before = authority_status(&server);
        let open_offer = find_open_offer(&tx, &server.id).await?;
        if before == AuthorityStatus::Unclaimed && open_offer.is_none() {
            self.agent_manager.remove_connection(&server.id);
            self.broadcast_authority_state(&server.id, AuthorityStatus::Unclaimed, None);
            return Ok(RevocationReceipt {
                server_id: input.server_id,
                changed: false,
            });
        }

        self.agent_manager.remove_connection(&server.id);
        if let Some(offer) = open_offer {
            let outcome = if offer.expires_at <= Utc::now() {
                OfferOutcome::Expired
            } else {
                OfferOutcome::Revoked
            };
            terminalize_offer(
                &tx,
                offer,
                outcome,
                None,
                &server,
                &input.actor,
                &input.source,
                before,
                before,
            )
            .await?;
        }
        if before == AuthorityStatus::Claimed {
            let mut active: server::ActiveModel = server.clone().into();
            active.token_hash = Set(None);
            active.token_prefix = Set(None);
            active.updated_at = Set(Utc::now());
            active.update(&tx).await.map_err(AppError::from)?;
            insert_event(
                &tx,
                EventInput {
                    server: &server,
                    actor: &input.actor,
                    source: &input.source,
                    offer_id: None,
                    transition: AuthorityTransition::AuthorityRevoked,
                    mode: None,
                    offer_outcome: None,
                    authority_before: AuthorityStatus::Claimed,
                    authority_after: AuthorityStatus::Unclaimed,
                },
            )
            .await?;
        }
        tx.commit().await.map_err(AppError::from)?;
        self.broadcast_authority_state(&server.id, AuthorityStatus::Unclaimed, None);
        Ok(RevocationReceipt {
            server_id: input.server_id,
            changed: true,
        })
    }

    pub async fn state(&self, server_id: ServerId) -> Result<AuthorityState, StateError> {
        self.states(std::slice::from_ref(&server_id))
            .await?
            .into_iter()
            .next()
            .ok_or(StateError::NotFound)
    }

    pub async fn states(&self, server_ids: &[ServerId]) -> Result<Vec<AuthorityState>, StateError> {
        if server_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = server_ids
            .iter()
            .map(|server_id| server_id.as_str().to_string())
            .collect();
        let servers = server::Entity::find()
            .filter(server::Column::Id.is_in(ids.iter().cloned()))
            .all(&self.db)
            .await
            .map_err(AppError::from)?;
        let offers = enrollment_offer::Entity::find()
            .filter(enrollment_offer::Column::TargetServerId.is_in(ids))
            .filter(enrollment_offer::Column::Outcome.is_null())
            .filter(enrollment_offer::Column::ExpiresAt.gt(Utc::now()))
            .all(&self.db)
            .await
            .map_err(AppError::from)?;
        let mut offers_by_server: HashMap<String, OutstandingOffer> = offers
            .into_iter()
            .map(|offer| {
                let server_id = offer.target_server_id.clone();
                to_outstanding(offer).map(|offer| (server_id, offer))
            })
            .collect::<Result<_, _>>()?;

        servers
            .into_iter()
            .map(|server| {
                Ok(AuthorityState {
                    server_id: ServerId::parse(server.id.clone()).map_err(|error| {
                        AppError::Internal(format!("invalid stored server id: {error}"))
                    })?,
                    authority: authority_status(&server),
                    outstanding_offer: offers_by_server.remove(&server.id),
                })
            })
            .collect()
    }

    pub async fn history(&self, query: HistoryQuery) -> Result<Vec<AuthorityEvent>, HistoryError> {
        let limit = query.limit.clamp(1, 500);
        let rows = agent_authority_event::Entity::find()
            .filter(agent_authority_event::Column::ServerId.eq(query.server_id.as_str()))
            .order_by_desc(agent_authority_event::Column::CreatedAt)
            .limit(limit)
            .all(&self.db)
            .await
            .map_err(AppError::from)?;
        rows.into_iter().map(to_authority_event).collect()
    }

    pub async fn preflight_connection(
        &self,
        token: PresentedRunToken,
    ) -> Result<PendingAdmission, AdmissionError> {
        let server = AuthService::validate_agent_token(&self.db, token.expose())
            .await?
            .ok_or(AdmissionError::Rejected)?;
        Ok(PendingAdmission {
            authority: self.clone(),
            expected_server_id: ServerId::parse(server.id).map_err(|error| {
                AppError::Internal(format!("invalid stored server id: {error}"))
            })?,
            token,
        })
    }

    pub(crate) async fn issue_initial_offer_tx(
        &self,
        tx: &DatabaseTransaction,
        server: &server::Model,
        actor: &Actor,
        source: &RequestSource,
        ttl: OfferTtl,
    ) -> Result<IssuedOffer, AppError> {
        let issued = mint_offer(tx, &server.id, actor, ttl).await?;
        insert_event(
            tx,
            EventInput {
                server,
                actor,
                source,
                offer_id: Some(issued.id.as_str()),
                transition: AuthorityTransition::InitialOfferIssued,
                mode: None,
                offer_outcome: None,
                authority_before: AuthorityStatus::Unclaimed,
                authority_after: AuthorityStatus::Unclaimed,
            },
        )
        .await?;
        Ok(issued)
    }

    pub(crate) fn broadcast_issued_offer_state(
        &self,
        server_id: &str,
        status: AuthorityStatus,
        offer: &IssuedOffer,
    ) {
        self.broadcast_authority_state(
            server_id,
            status,
            Some(OutstandingEnrollmentSummary {
                id: offer.id.as_str().to_string(),
                code_prefix: offer.code_prefix.clone(),
                expires_at: offer.expires_at.to_rfc3339(),
                created_at: offer.created_at.to_rfc3339(),
            }),
        );
    }

    fn broadcast_authority_state(
        &self,
        server_id: &str,
        status: AuthorityStatus,
        outstanding_offer: Option<OutstandingEnrollmentSummary>,
    ) {
        self.agent_manager.broadcast_agent_authority_changed(
            server_id.to_string(),
            AgentAuthorityStateSummary {
                status: match status {
                    AuthorityStatus::Claimed => AgentAuthorityStatus::Claimed,
                    AuthorityStatus::Unclaimed => AgentAuthorityStatus::Unclaimed,
                },
                outstanding_offer,
            },
        );
    }

    pub(crate) async fn record_server_deleted_tx(
        &self,
        tx: &DatabaseTransaction,
        server: &server::Model,
        actor: &Actor,
        source: &RequestSource,
    ) -> Result<(), AppError> {
        let status = authority_status(server);
        insert_event(
            tx,
            EventInput {
                server,
                actor,
                source,
                offer_id: None,
                transition: AuthorityTransition::ServerDeleted,
                mode: None,
                offer_outcome: None,
                authority_before: status,
                authority_after: AuthorityStatus::Unclaimed,
            },
        )
        .await
    }

    pub(crate) async fn delete_servers(
        &self,
        server_ids: &[ServerId],
        actor: &Actor,
        source: &RequestSource,
    ) -> Result<Vec<server::Model>, AppError> {
        let mut ids: Vec<String> = server_ids
            .iter()
            .map(|server_id| server_id.as_str().to_string())
            .collect();
        ids.sort();
        ids.dedup();
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let locks: Vec<_> = ids
            .iter()
            .map(|server_id| self.agent_manager.server_lifecycle_lock(server_id))
            .collect();
        let mut guards = Vec::with_capacity(locks.len());
        for lock in &locks {
            guards.push(lock.lock().await);
        }

        let tx = self.db.begin().await?;
        let rows = server::Entity::find()
            .filter(server::Column::Id.is_in(ids.iter().cloned()))
            .all(&tx)
            .await?;
        for row in &rows {
            self.agent_manager.remove_connection(&row.id);
        }
        for row in &rows {
            self.record_server_deleted_tx(&tx, row, actor, source)
                .await?;
        }
        let existing_ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
        if !existing_ids.is_empty() {
            ServerService::delete_server_scoped_rows(&tx, &existing_ids).await?;
            server::Entity::delete_many()
                .filter(server::Column::Id.is_in(existing_ids.iter().cloned()))
                .exec(&tx)
                .await?;
        }
        tx.commit().await?;

        for server_id in &existing_ids {
            self.agent_manager.remove_cached_report(server_id);
        }
        drop(guards);
        Ok(rows)
    }
}

pub struct PendingAdmission {
    authority: AgentAuthority,
    expected_server_id: ServerId,
    token: PresentedRunToken,
}

impl std::fmt::Debug for PendingAdmission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingAdmission")
            .field("expected_server_id", &self.expected_server_id)
            .field("token", &"<redacted>")
            .finish()
    }
}

impl PendingAdmission {
    pub async fn admit(
        self,
        connection: NewConnection,
    ) -> Result<AdmittedConnection, AdmissionError> {
        let server_lock = self
            .authority
            .agent_manager
            .server_lifecycle_lock(self.expected_server_id.as_str());
        let _guard = server_lock.lock().await;
        let server = AuthService::validate_agent_token(&self.authority.db, self.token.expose())
            .await?
            .filter(|server| server.id == self.expected_server_id.as_str())
            .ok_or(AdmissionError::Rejected)?;
        let connection_id = self.authority.agent_manager.add_connection(
            server.id.clone(),
            server.name.clone(),
            connection.tx,
            connection.remote_addr,
        );
        Ok(AdmittedConnection {
            server_id: self.expected_server_id,
            server_name: server.name,
            server_capabilities: server.capabilities,
            connection_id,
        })
    }
}

struct PreparedOffer {
    id: String,
    model: enrollment_offer::ActiveModel,
    code: EnrollmentCode,
}

fn prepare_offer(server_id: &str, actor: &Actor, ttl: OfferTtl) -> Result<PreparedOffer, AppError> {
    let now = Utc::now();
    let plaintext = AuthService::generate_session_token();
    let code = EnrollmentCode::parse(plaintext.clone()).map_err(|error| {
        AppError::Internal(format!("generated invalid enrollment code: {error}"))
    })?;
    let code_hash = AuthService::hash_password(&plaintext)?;
    let code_prefix = plaintext[..8].to_string();
    let id = Uuid::new_v4().to_string();
    Ok(PreparedOffer {
        id: id.clone(),
        model: enrollment_offer::ActiveModel {
            id: Set(id),
            code_hash: Set(code_hash),
            code_prefix: Set(code_prefix),
            target_server_id: Set(server_id.to_string()),
            created_by: Set(actor.offer_creator()),
            expires_at: Set(now + Duration::seconds(ttl.value())),
            outcome: Set(None),
            terminal_at: Set(None),
            successor_offer_id: Set(None),
            created_at: Set(now),
        },
        code,
    })
}

async fn insert_prepared_offer<C: ConnectionTrait>(
    conn: &C,
    prepared: PreparedOffer,
) -> Result<IssuedOffer, AppError> {
    let row = prepared.model.insert(conn).await?;
    Ok(IssuedOffer {
        id: OfferId::parse(row.id)
            .map_err(|error| AppError::Internal(format!("invalid stored offer id: {error}")))?,
        code: prepared.code,
        code_prefix: row.code_prefix,
        expires_at: row.expires_at,
        created_at: row.created_at,
    })
}

async fn mint_offer<C: ConnectionTrait>(
    conn: &C,
    server_id: &str,
    actor: &Actor,
    ttl: OfferTtl,
) -> Result<IssuedOffer, AppError> {
    insert_prepared_offer(conn, prepare_offer(server_id, actor, ttl)?).await
}

async fn find_offer_for_code<C: ConnectionTrait>(
    conn: &C,
    code: &EnrollmentCode,
) -> Result<Option<enrollment_offer::Model>, AppError> {
    let candidates = enrollment_offer::Entity::find()
        .filter(enrollment_offer::Column::CodePrefix.eq(&code.expose()[..8]))
        .filter(enrollment_offer::Column::Outcome.is_null())
        .all(conn)
        .await?;
    for candidate in candidates {
        if AuthService::verify_password(code.expose(), &candidate.code_hash)? {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

async fn find_outstanding<C: ConnectionTrait>(
    conn: &C,
    server_id: &str,
    now: chrono::DateTime<Utc>,
) -> Result<Option<enrollment_offer::Model>, AppError> {
    Ok(enrollment_offer::Entity::find()
        .filter(enrollment_offer::Column::TargetServerId.eq(server_id))
        .filter(enrollment_offer::Column::Outcome.is_null())
        .filter(enrollment_offer::Column::ExpiresAt.gt(now))
        .one(conn)
        .await?)
}

async fn find_open_offer<C: ConnectionTrait>(
    conn: &C,
    server_id: &str,
) -> Result<Option<enrollment_offer::Model>, AppError> {
    Ok(enrollment_offer::Entity::find()
        .filter(enrollment_offer::Column::TargetServerId.eq(server_id))
        .filter(enrollment_offer::Column::Outcome.is_null())
        .one(conn)
        .await?)
}

async fn expire_elapsed_outstanding(
    tx: &DatabaseTransaction,
    server: &server::Model,
    actor: &Actor,
    source: &RequestSource,
) -> Result<(), AppError> {
    let Some(offer) = find_open_offer(tx, &server.id).await? else {
        return Ok(());
    };
    if offer.expires_at > Utc::now() {
        return Ok(());
    }
    let status = authority_status(server);
    terminalize_offer(
        tx,
        offer,
        OfferOutcome::Expired,
        None,
        server,
        actor,
        source,
        status,
        status,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn terminalize_offer(
    tx: &DatabaseTransaction,
    offer: enrollment_offer::Model,
    outcome: OfferOutcome,
    successor_offer_id: Option<String>,
    server: &server::Model,
    actor: &Actor,
    source: &RequestSource,
    authority_before: AuthorityStatus,
    authority_after: AuthorityStatus,
) -> Result<(), AppError> {
    let offer_id = offer.id.clone();
    let mut active: enrollment_offer::ActiveModel = offer.into();
    active.outcome = Set(Some(outcome.as_str().to_string()));
    active.terminal_at = Set(Some(Utc::now()));
    active.successor_offer_id = Set(successor_offer_id);
    active.update(tx).await?;
    insert_event(
        tx,
        EventInput {
            server,
            actor,
            source,
            offer_id: Some(&offer_id),
            transition: match outcome {
                OfferOutcome::Consumed => AuthorityTransition::OfferConsumed,
                OfferOutcome::Revoked => AuthorityTransition::OfferRevoked,
                OfferOutcome::Replaced => AuthorityTransition::OfferReplaced,
                OfferOutcome::Expired => AuthorityTransition::OfferExpired,
            },
            mode: None,
            offer_outcome: Some(outcome),
            authority_before,
            authority_after,
        },
    )
    .await
}

struct EventInput<'a> {
    server: &'a server::Model,
    actor: &'a Actor,
    source: &'a RequestSource,
    offer_id: Option<&'a str>,
    transition: AuthorityTransition,
    mode: Option<ReenrollmentMode>,
    offer_outcome: Option<OfferOutcome>,
    authority_before: AuthorityStatus,
    authority_after: AuthorityStatus,
}

async fn insert_event(tx: &DatabaseTransaction, input: EventInput<'_>) -> Result<(), AppError> {
    agent_authority_event::ActiveModel {
        id: Set(Uuid::new_v4().to_string()),
        server_id: Set(input.server.id.clone()),
        server_name: Set(input.server.name.clone()),
        actor_kind: Set(input.actor.kind().as_str().to_string()),
        actor_id: Set(input.actor.id().map(ToOwned::to_owned)),
        request_source: Set(input.source.as_str().to_string()),
        offer_id: Set(input.offer_id.map(ToOwned::to_owned)),
        transition: Set(input.transition.as_str().to_string()),
        mode: Set(input.mode.map(|mode| mode.as_str().to_string())),
        offer_outcome: Set(input
            .offer_outcome
            .map(|outcome| outcome.as_str().to_string())),
        authority_before: Set(input.authority_before.as_str().to_string()),
        authority_after: Set(input.authority_after.as_str().to_string()),
        created_at: Set(Utc::now()),
    }
    .insert(tx)
    .await?;
    Ok(())
}

fn authority_status(server: &server::Model) -> AuthorityStatus {
    if server.token_hash.is_some() {
        AuthorityStatus::Claimed
    } else {
        AuthorityStatus::Unclaimed
    }
}

fn parse_outcome(value: &str) -> Result<OfferOutcome, AppError> {
    OfferOutcome::parse(value)
        .ok_or_else(|| AppError::Internal(format!("invalid stored offer outcome: {value}")))
}

fn parse_authority(value: &str) -> Result<AuthorityStatus, AppError> {
    match value {
        "claimed" => Ok(AuthorityStatus::Claimed),
        "unclaimed" => Ok(AuthorityStatus::Unclaimed),
        _ => Err(AppError::Internal(format!(
            "invalid stored authority status: {value}"
        ))),
    }
}

fn to_outstanding(row: enrollment_offer::Model) -> Result<OutstandingOffer, AppError> {
    Ok(OutstandingOffer {
        id: OfferId::parse(row.id)
            .map_err(|error| AppError::Internal(format!("invalid stored offer id: {error}")))?,
        code_prefix: row.code_prefix,
        expires_at: row.expires_at,
        created_at: row.created_at,
    })
}

fn to_authority_event(row: agent_authority_event::Model) -> Result<AuthorityEvent, HistoryError> {
    Ok(AuthorityEvent {
        id: row.id,
        server_id: ServerId::parse(row.server_id)
            .map_err(|error| AppError::Internal(format!("invalid stored server id: {error}")))?,
        server_name: row.server_name,
        actor_kind: ActorKind::parse(&row.actor_kind).ok_or_else(|| {
            AppError::Internal(format!("invalid stored actor kind: {}", row.actor_kind))
        })?,
        actor_id: row.actor_id,
        request_source: row.request_source,
        offer_id: row
            .offer_id
            .map(OfferId::parse)
            .transpose()
            .map_err(|error| AppError::Internal(format!("invalid stored offer id: {error}")))?,
        transition: AuthorityTransition::parse(&row.transition).ok_or_else(|| {
            AppError::Internal(format!(
                "invalid stored authority transition: {}",
                row.transition
            ))
        })?,
        mode: match row.mode.as_deref() {
            Some(value) => Some(ReenrollmentMode::parse(value).ok_or_else(|| {
                AppError::Internal(format!("invalid stored re-enrollment mode: {value}"))
            })?),
            None => None,
        },
        offer_outcome: row
            .offer_outcome
            .as_deref()
            .map(parse_outcome)
            .transpose()?,
        authority_before: parse_authority(&row.authority_before)?,
        authority_after: parse_authority(&row.authority_after)?,
        created_at: row.created_at,
    })
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use chrono::{Duration, Utc};
    use sea_orm::{
        ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
        EntityTrait, PaginatorTrait, QueryFilter,
    };
    use serverbee_common::constants::CAP_DEFAULT;
    use tokio::sync::{broadcast, mpsc};

    use super::*;
    use crate::entity::{agent_authority_event, enrollment_offer, server};
    use crate::test_utils::setup_test_db;

    const FIRST_TOKEN: &str = "first-token-0123456789abcdefghijklmnop";
    const SECOND_TOKEN: &str = "second-token-0123456789abcdefghijklmno";

    struct Fixture {
        authority: AgentAuthority,
        db: DatabaseConnection,
        agent_manager: Arc<AgentManager>,
        server_id: ServerId,
        _tmp: tempfile::TempDir,
    }

    impl Fixture {
        async fn issue(&self) -> IssuedOffer {
            self.authority
                .issue_offer_for_unclaimed(IssueOfferForUnclaimed {
                    server_id: self.server_id.clone(),
                    actor: user_actor(),
                    source: api_source(),
                    ttl: OfferTtl::default(),
                })
                .await
                .expect("issue offer")
        }

        async fn claim(&self, code: EnrollmentCode, token: &str) -> ClaimReceipt {
            self.authority
                .claim(ClaimAgent {
                    code,
                    proposed_run_token: ProposedRunToken::parse(token).expect("run token"),
                    source: agent_source(),
                    remote_addr: Some("127.0.0.1".to_string()),
                })
                .await
                .expect("claim")
        }

        async fn claim_initial(&self, token: &str) -> IssuedOffer {
            let offer = self.issue().await;
            self.claim(offer.code.clone(), token).await;
            offer
        }

        fn add_connection(&self) -> u64 {
            let (tx, _rx) = mpsc::channel(1);
            self.agent_manager.add_connection(
                self.server_id.as_str().to_string(),
                "Server One".to_string(),
                tx,
                loopback(),
            )
        }
    }

    async fn authority_with_unclaimed_server() -> Fixture {
        let (db, tmp) = setup_test_db().await;
        let now = Utc::now();
        let server_id = ServerId::parse("server-1").expect("server id");
        server::ActiveModel {
            id: Set(server_id.as_str().to_string()),
            token_hash: Set(None),
            token_prefix: Set(None),
            name: Set("Server One".to_string()),
            weight: Set(0),
            hidden: Set(false),
            capabilities: Set(CAP_DEFAULT as i32),
            protocol_version: Set(1),
            features: Set("[]".to_string()),
            geo_manual: Set(false),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("seed server");
        let (browser_tx, _) = broadcast::channel(8);
        let agent_manager = Arc::new(AgentManager::new(browser_tx));
        Fixture {
            authority: AgentAuthority::new(db.clone(), agent_manager.clone()),
            db,
            agent_manager,
            server_id,
            _tmp: tmp,
        }
    }

    fn user_actor() -> Actor {
        Actor::User {
            id: "user-1".to_string(),
        }
    }

    fn api_source() -> RequestSource {
        RequestSource::parse("api:test").expect("source")
    }

    fn agent_source() -> RequestSource {
        RequestSource::parse("agent:register").expect("source")
    }

    fn loopback() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 9527)
    }

    #[tokio::test]
    async fn issue_offer_for_unclaimed_exposes_one_outstanding_offer() {
        let fixture = authority_with_unclaimed_server().await;

        let issued = fixture
            .authority
            .issue_offer_for_unclaimed(IssueOfferForUnclaimed {
                server_id: fixture.server_id.clone(),
                actor: user_actor(),
                source: api_source(),
                ttl: OfferTtl::default(),
            })
            .await
            .expect("issue offer");

        assert_eq!(issued.code_prefix, &issued.code.expose()[..8]);
        assert_eq!(issued.id.as_str().len(), 36);

        let state = fixture
            .authority
            .state(fixture.server_id.clone())
            .await
            .expect("state");
        assert_eq!(state.authority, AuthorityStatus::Unclaimed);
        assert_eq!(
            state.outstanding_offer.as_ref().map(|offer| &offer.id),
            Some(&issued.id)
        );

        let duplicate = fixture
            .authority
            .issue_offer_for_unclaimed(IssueOfferForUnclaimed {
                server_id: fixture.server_id.clone(),
                actor: user_actor(),
                source: api_source(),
                ttl: OfferTtl::default(),
            })
            .await;
        assert!(matches!(
            duplicate,
            Err(IssueOfferError::OutstandingExists(_))
        ));
    }

    #[tokio::test]
    async fn claim_consumes_offer_and_hashes_agent_proposed_token() {
        let fixture = authority_with_unclaimed_server().await;
        let offer = fixture.issue().await;

        let receipt = fixture.claim(offer.code.clone(), FIRST_TOKEN).await;

        assert_eq!(receipt.server_id, fixture.server_id);
        let stored = server::Entity::find_by_id(fixture.server_id.as_str())
            .one(&fixture.db)
            .await
            .expect("read server")
            .expect("server");
        assert_eq!(stored.token_prefix.as_deref(), Some(&FIRST_TOKEN[..8]));
        assert_ne!(stored.token_hash.as_deref(), Some(FIRST_TOKEN));
        assert!(
            AuthService::verify_password(FIRST_TOKEN, stored.token_hash.as_deref().expect("hash"))
                .expect("verify token")
        );
        let stored_offer = enrollment_offer::Entity::find_by_id(offer.id.as_str())
            .one(&fixture.db)
            .await
            .expect("read offer")
            .expect("offer");
        assert_eq!(stored_offer.outcome.as_deref(), Some("consumed"));
        assert!(stored_offer.terminal_at.is_some());

        let history = fixture
            .authority
            .history(HistoryQuery {
                server_id: fixture.server_id.clone(),
                limit: 10,
            })
            .await
            .expect("history");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].transition, AuthorityTransition::OfferConsumed);
        assert_eq!(history[0].request_source, "agent:register");
        assert_eq!(history[0].authority_before, AuthorityStatus::Unclaimed);
        assert_eq!(history[0].authority_after, AuthorityStatus::Claimed);
    }

    #[tokio::test]
    async fn invalid_claim_does_not_change_authority_offer_or_history() {
        let fixture = authority_with_unclaimed_server().await;
        let offer = fixture.issue().await;
        let before = fixture
            .authority
            .history(HistoryQuery {
                server_id: fixture.server_id.clone(),
                limit: 10,
            })
            .await
            .expect("history");

        let result = fixture
            .authority
            .claim(ClaimAgent {
                code: EnrollmentCode::parse("wrong-code-0123456789").expect("wrong code"),
                proposed_run_token: ProposedRunToken::parse(FIRST_TOKEN).expect("token"),
                source: agent_source(),
                remote_addr: None,
            })
            .await;

        assert!(matches!(result, Err(ClaimError::Rejected)));
        let state = fixture
            .authority
            .state(fixture.server_id.clone())
            .await
            .expect("state");
        assert_eq!(state.authority, AuthorityStatus::Unclaimed);
        assert_eq!(
            state.outstanding_offer.map(|current| current.id),
            Some(offer.id)
        );
        let after = fixture
            .authority
            .history(HistoryQuery {
                server_id: fixture.server_id.clone(),
                limit: 10,
            })
            .await
            .expect("history");
        assert_eq!(after, before);
    }

    #[tokio::test]
    async fn graceful_reenrollment_preserves_authority_until_claim_then_fences_old_connection() {
        let fixture = authority_with_unclaimed_server().await;
        fixture.claim_initial(FIRST_TOKEN).await;
        fixture.add_connection();

        let offer = fixture
            .authority
            .begin_reenrollment(BeginReenrollment {
                server_id: fixture.server_id.clone(),
                mode: ReenrollmentMode::Graceful,
                actor: user_actor(),
                source: api_source(),
                ttl: OfferTtl::default(),
            })
            .await
            .expect("begin graceful re-enrollment");

        let pending = fixture
            .authority
            .state(fixture.server_id.clone())
            .await
            .expect("state");
        assert_eq!(pending.authority, AuthorityStatus::Claimed);
        assert!(fixture.agent_manager.is_online(fixture.server_id.as_str()));
        assert!(
            AuthService::validate_agent_token(&fixture.db, FIRST_TOKEN)
                .await
                .expect("validate old token")
                .is_some()
        );

        fixture.claim(offer.code, SECOND_TOKEN).await;

        assert!(!fixture.agent_manager.is_online(fixture.server_id.as_str()));
        assert!(
            AuthService::validate_agent_token(&fixture.db, FIRST_TOKEN)
                .await
                .expect("validate old token")
                .is_none()
        );
        assert!(
            AuthService::validate_agent_token(&fixture.db, SECOND_TOKEN)
                .await
                .expect("validate new token")
                .is_some()
        );
    }

    #[tokio::test]
    async fn emergency_reenrollment_revokes_authority_and_fences_immediately() {
        let fixture = authority_with_unclaimed_server().await;
        fixture.claim_initial(FIRST_TOKEN).await;
        fixture.add_connection();

        let issued = fixture
            .authority
            .begin_reenrollment(BeginReenrollment {
                server_id: fixture.server_id.clone(),
                mode: ReenrollmentMode::Emergency,
                actor: user_actor(),
                source: api_source(),
                ttl: OfferTtl::default(),
            })
            .await
            .expect("begin emergency re-enrollment");

        let state = fixture
            .authority
            .state(fixture.server_id.clone())
            .await
            .expect("state");
        assert_eq!(state.authority, AuthorityStatus::Unclaimed);
        assert_eq!(
            state.outstanding_offer.map(|offer| offer.id),
            Some(issued.id)
        );
        assert!(!fixture.agent_manager.is_online(fixture.server_id.as_str()));
        assert!(
            AuthService::validate_agent_token(&fixture.db, FIRST_TOKEN)
                .await
                .expect("validate old token")
                .is_none()
        );
    }

    #[tokio::test]
    async fn exact_replacement_links_successor_and_terminal_outcomes_are_immutable() {
        let fixture = authority_with_unclaimed_server().await;
        let original = fixture.issue().await;

        let replacement = fixture
            .authority
            .replace_offer(ReplaceOffer {
                server_id: fixture.server_id.clone(),
                offer_id: original.id.clone(),
                actor: user_actor(),
                source: api_source(),
                ttl: OfferTtl::default(),
            })
            .await
            .expect("replace offer");

        let old_row = enrollment_offer::Entity::find_by_id(original.id.as_str())
            .one(&fixture.db)
            .await
            .expect("read old offer")
            .expect("old offer");
        assert_eq!(old_row.outcome.as_deref(), Some("replaced"));
        assert_eq!(
            old_row.successor_offer_id.as_deref(),
            Some(replacement.id.as_str())
        );

        let stale = fixture
            .authority
            .replace_offer(ReplaceOffer {
                server_id: fixture.server_id.clone(),
                offer_id: original.id,
                actor: user_actor(),
                source: api_source(),
                ttl: OfferTtl::default(),
            })
            .await;
        assert!(matches!(
            stale,
            Err(ReplaceOfferError::NotOutstanding {
                outcome: OfferOutcome::Replaced,
                current: Some(_)
            })
        ));

        let first_revoke = fixture
            .authority
            .revoke_offer(RevokeOffer {
                server_id: fixture.server_id.clone(),
                offer_id: replacement.id.clone(),
                actor: user_actor(),
                source: api_source(),
            })
            .await
            .expect("revoke replacement");
        assert!(!first_revoke.already_revoked);
        let second_revoke = fixture
            .authority
            .revoke_offer(RevokeOffer {
                server_id: fixture.server_id,
                offer_id: replacement.id,
                actor: user_actor(),
                source: api_source(),
            })
            .await
            .expect("repeat revoke");
        assert!(second_revoke.already_revoked);
    }

    #[tokio::test]
    async fn elapsed_offer_is_materialized_as_expired_before_successor_is_issued() {
        let fixture = authority_with_unclaimed_server().await;
        let expired = fixture.issue().await;
        let mut row: enrollment_offer::ActiveModel =
            enrollment_offer::Entity::find_by_id(expired.id.as_str())
                .one(&fixture.db)
                .await
                .expect("read offer")
                .expect("offer")
                .into();
        row.expires_at = Set(Utc::now() - Duration::seconds(1));
        row.update(&fixture.db).await.expect("expire offer");

        let successor = fixture
            .authority
            .issue_offer_for_unclaimed(IssueOfferForUnclaimed {
                server_id: fixture.server_id.clone(),
                actor: user_actor(),
                source: api_source(),
                ttl: OfferTtl::default(),
            })
            .await
            .expect("issue successor");

        let expired_row = enrollment_offer::Entity::find_by_id(expired.id.as_str())
            .one(&fixture.db)
            .await
            .expect("read expired offer")
            .expect("expired offer");
        assert_eq!(expired_row.outcome.as_deref(), Some("expired"));
        assert_eq!(
            fixture
                .authority
                .state(fixture.server_id.clone())
                .await
                .expect("state")
                .outstanding_offer
                .map(|offer| offer.id),
            Some(successor.id)
        );
    }

    #[tokio::test]
    async fn authority_revocation_creates_no_offer_and_is_idempotent() {
        let fixture = authority_with_unclaimed_server().await;
        fixture.claim_initial(FIRST_TOKEN).await;
        fixture.add_connection();

        let first = fixture
            .authority
            .revoke_authority(RevokeAuthority {
                server_id: fixture.server_id.clone(),
                actor: user_actor(),
                source: api_source(),
            })
            .await
            .expect("revoke authority");
        let second = fixture
            .authority
            .revoke_authority(RevokeAuthority {
                server_id: fixture.server_id.clone(),
                actor: user_actor(),
                source: api_source(),
            })
            .await
            .expect("repeat revoke");

        assert!(first.changed);
        assert!(!second.changed);
        let state = fixture
            .authority
            .state(fixture.server_id.clone())
            .await
            .expect("state");
        assert_eq!(state.authority, AuthorityStatus::Unclaimed);
        assert!(state.outstanding_offer.is_none());
        assert!(!fixture.agent_manager.is_online(fixture.server_id.as_str()));
    }

    #[tokio::test]
    async fn authority_revocation_terminalizes_graceful_reenrollment_offer() {
        let fixture = authority_with_unclaimed_server().await;
        fixture.claim_initial(FIRST_TOKEN).await;
        let offer = fixture
            .authority
            .begin_reenrollment(BeginReenrollment {
                server_id: fixture.server_id.clone(),
                actor: user_actor(),
                source: api_source(),
                mode: ReenrollmentMode::Graceful,
                ttl: OfferTtl::default(),
            })
            .await
            .expect("begin graceful re-enrollment");
        let code = offer.code.clone();
        fixture.add_connection();

        fixture
            .authority
            .revoke_authority(RevokeAuthority {
                server_id: fixture.server_id.clone(),
                actor: user_actor(),
                source: api_source(),
            })
            .await
            .expect("revoke authority");

        let state = fixture
            .authority
            .state(fixture.server_id.clone())
            .await
            .expect("state");
        assert_eq!(state.authority, AuthorityStatus::Unclaimed);
        assert!(state.outstanding_offer.is_none());
        let row = enrollment_offer::Entity::find_by_id(offer.id.as_str())
            .one(&fixture.db)
            .await
            .expect("read offer")
            .expect("offer");
        assert_eq!(row.outcome.as_deref(), Some("revoked"));
        assert!(matches!(
            fixture
                .authority
                .claim(ClaimAgent {
                    code,
                    proposed_run_token: ProposedRunToken::parse(SECOND_TOKEN).expect("run token"),
                    source: agent_source(),
                    remote_addr: Some("127.0.0.1".to_string()),
                })
                .await,
            Err(ClaimError::Rejected)
        ));
        assert!(!fixture.agent_manager.is_online(fixture.server_id.as_str()));
    }

    #[tokio::test]
    async fn failed_event_write_rolls_back_credential_transition() {
        let fixture = authority_with_unclaimed_server().await;
        fixture.claim_initial(FIRST_TOKEN).await;
        fixture.add_connection();
        fixture
            .db
            .execute_unprepared(
                "CREATE TRIGGER reject_authority_events BEFORE INSERT ON agent_authority_events \
                 BEGIN SELECT RAISE(ABORT, 'forced authority event failure'); END",
            )
            .await
            .expect("create failure trigger");

        let result = fixture
            .authority
            .revoke_authority(RevokeAuthority {
                server_id: fixture.server_id.clone(),
                actor: user_actor(),
                source: api_source(),
            })
            .await;

        assert!(matches!(result, Err(RevokeAuthorityError::Store(_))));
        assert!(!fixture.agent_manager.is_online(fixture.server_id.as_str()));
        assert!(
            AuthService::validate_agent_token(&fixture.db, FIRST_TOKEN)
                .await
                .expect("validate token")
                .is_some()
        );
        let pending = fixture
            .authority
            .preflight_connection(PresentedRunToken::parse(FIRST_TOKEN).expect("token"))
            .await
            .expect("old authority remains durable");
        let (tx, _rx) = mpsc::channel(1);
        pending
            .admit(NewConnection {
                tx,
                remote_addr: loopback(),
            })
            .await
            .expect("old authority reconnects after failed transition");
        assert!(fixture.agent_manager.is_online(fixture.server_id.as_str()));
    }

    #[tokio::test]
    async fn preflight_does_not_survive_authority_revocation() {
        let fixture = authority_with_unclaimed_server().await;
        fixture.claim_initial(FIRST_TOKEN).await;
        let pending = fixture
            .authority
            .preflight_connection(PresentedRunToken::parse(FIRST_TOKEN).expect("token"))
            .await
            .expect("preflight");
        fixture
            .authority
            .revoke_authority(RevokeAuthority {
                server_id: fixture.server_id.clone(),
                actor: user_actor(),
                source: api_source(),
            })
            .await
            .expect("revoke authority");
        let (tx, _rx) = mpsc::channel(1);

        let result = pending
            .admit(NewConnection {
                tx,
                remote_addr: loopback(),
            })
            .await;

        assert!(matches!(result, Err(AdmissionError::Rejected)));
        assert!(!fixture.agent_manager.is_online(fixture.server_id.as_str()));
    }

    #[tokio::test]
    async fn concurrent_claims_and_replacements_each_have_exactly_one_winner() {
        let fixture = authority_with_unclaimed_server().await;
        let original = fixture.issue().await;
        let replace_input = ReplaceOffer {
            server_id: fixture.server_id.clone(),
            offer_id: original.id,
            actor: user_actor(),
            source: api_source(),
            ttl: OfferTtl::default(),
        };

        let (first_replace, second_replace) = tokio::join!(
            fixture.authority.replace_offer(replace_input.clone()),
            fixture.authority.replace_offer(replace_input)
        );
        let replacements = [first_replace, second_replace];
        assert_eq!(
            replacements.iter().filter(|result| result.is_ok()).count(),
            1
        );
        let replacement = replacements
            .into_iter()
            .find_map(Result::ok)
            .expect("replacement winner");

        let first_claim = ClaimAgent {
            code: replacement.code.clone(),
            proposed_run_token: ProposedRunToken::parse(FIRST_TOKEN).expect("token"),
            source: agent_source(),
            remote_addr: None,
        };
        let second_claim = ClaimAgent {
            code: replacement.code,
            proposed_run_token: ProposedRunToken::parse(SECOND_TOKEN).expect("token"),
            source: agent_source(),
            remote_addr: None,
        };
        let (first_claim, second_claim) = tokio::join!(
            fixture.authority.claim(first_claim),
            fixture.authority.claim(second_claim)
        );
        assert_eq!(
            [first_claim, second_claim]
                .iter()
                .filter(|result| result.is_ok())
                .count(),
            1
        );
    }

    #[test]
    fn authority_secrets_are_redacted_from_debug() {
        let enrollment = EnrollmentCode::parse("0123456789abcdef").expect("code");
        let proposed = ProposedRunToken::parse("x".repeat(32)).expect("proposed token");
        let presented = PresentedRunToken::parse("abcdefgh-token").expect("presented token");

        assert_eq!(format!("{enrollment:?}"), "EnrollmentCode(<redacted>)");
        assert_eq!(format!("{proposed:?}"), "ProposedRunToken(<redacted>)");
        assert_eq!(format!("{presented:?}"), "PresentedRunToken(<redacted>)");
    }

    #[test]
    fn authority_secrets_reject_non_ascii_prefixes() {
        assert!(EnrollmentCode::parse("密钥密钥密钥密钥密钥密钥").is_err());
        assert!(ProposedRunToken::parse("密".repeat(32)).is_err());
        assert!(PresentedRunToken::parse("密钥密钥密钥").is_err());
    }

    #[tokio::test]
    async fn history_query_survives_server_deletion() {
        let fixture = authority_with_unclaimed_server().await;
        fixture.issue().await;
        let server = server::Entity::find_by_id(fixture.server_id.as_str())
            .one(&fixture.db)
            .await
            .expect("read server")
            .expect("server");
        let tx = fixture.db.begin().await.expect("transaction");
        fixture
            .authority
            .record_server_deleted_tx(&tx, &server, &user_actor(), &api_source())
            .await
            .expect("record deletion");
        server::Entity::delete_by_id(fixture.server_id.as_str())
            .exec(&tx)
            .await
            .expect("delete server");
        tx.commit().await.expect("commit deletion");

        let offers = enrollment_offer::Entity::find()
            .filter(enrollment_offer::Column::TargetServerId.eq(fixture.server_id.as_str()))
            .count(&fixture.db)
            .await
            .expect("count offers");
        let events = agent_authority_event::Entity::find()
            .filter(agent_authority_event::Column::ServerId.eq(fixture.server_id.as_str()))
            .count(&fixture.db)
            .await
            .expect("count events");
        assert_eq!(offers, 0);
        assert_eq!(events, 2);
    }
}
