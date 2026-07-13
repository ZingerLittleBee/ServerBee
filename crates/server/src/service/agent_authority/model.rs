use std::fmt;
use std::net::SocketAddr;

use chrono::{DateTime, Utc};
use serverbee_common::protocol::ServerMessage;
use tokio::sync::mpsc;

use crate::error::AppError;

macro_rules! identifier {
    ($name:ident, $label:literal) => {
        #[derive(Clone, Debug, Eq, Hash, PartialEq)]
        pub struct $name(String);

        impl $name {
            pub fn parse(value: impl Into<String>) -> Result<Self, String> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(concat!($label, " must not be empty").to_string());
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn into_inner(self) -> String {
                self.0
            }
        }
    };
}

identifier!(ServerId, "server id");
identifier!(OfferId, "offer id");

#[derive(Clone, Eq, PartialEq)]
pub struct EnrollmentCode(String);

impl EnrollmentCode {
    pub fn parse(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.len() < 16 || value.len() > 512 || !value.is_ascii() {
            return Err("enrollment code must be 16-512 ASCII characters".to_string());
        }
        Ok(Self(value))
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for EnrollmentCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("EnrollmentCode(<redacted>)")
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct ProposedRunToken(String);

impl ProposedRunToken {
    pub fn parse(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.len() < 32
            || value.len() > 512
            || !value.is_ascii()
            || value.chars().any(char::is_whitespace)
        {
            return Err(
                "proposed run token must be 32-512 non-whitespace ASCII characters".to_string(),
            );
        }
        Ok(Self(value))
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ProposedRunToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ProposedRunToken(<redacted>)")
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct PresentedRunToken(String);

impl PresentedRunToken {
    pub fn parse(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.len() < 8 || value.len() > 512 || !value.is_ascii() {
            return Err("presented run token must be 8-512 ASCII characters".to_string());
        }
        Ok(Self(value))
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for PresentedRunToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PresentedRunToken(<redacted>)")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Actor {
    User { id: String },
    System,
    Agent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActorKind {
    User,
    System,
    Agent,
}

impl ActorKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::System => "system",
            Self::Agent => "agent",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "user" => Some(Self::User),
            "system" => Some(Self::System),
            "agent" => Some(Self::Agent),
            _ => None,
        }
    }
}

impl Actor {
    pub(crate) fn kind(&self) -> ActorKind {
        match self {
            Self::User { .. } => ActorKind::User,
            Self::System => ActorKind::System,
            Self::Agent => ActorKind::Agent,
        }
    }

    pub(crate) fn id(&self) -> Option<&str> {
        match self {
            Self::User { id } => Some(id),
            Self::System | Self::Agent => None,
        }
    }

    pub(crate) fn offer_creator(&self) -> String {
        self.id()
            .unwrap_or_else(|| self.kind().as_str())
            .to_string()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestSource(String);

impl RequestSource {
    pub fn parse(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() || value.len() > 256 {
            return Err("request source must be 1-256 characters".to_string());
        }
        Ok(Self(value))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OfferTtl(i64);

impl OfferTtl {
    pub const DEFAULT_SECONDS: i64 = 600;

    pub fn seconds(value: i64) -> Result<Self, String> {
        if !(1..=86_400).contains(&value) {
            return Err("offer ttl must be between 1 and 86400 seconds".to_string());
        }
        Ok(Self(value))
    }

    pub fn default_ttl() -> Self {
        Self(Self::DEFAULT_SECONDS)
    }

    pub(crate) fn value(self) -> i64 {
        self.0
    }
}

impl Default for OfferTtl {
    fn default() -> Self {
        Self::default_ttl()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthorityStatus {
    Claimed,
    Unclaimed,
}

impl AuthorityStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Claimed => "claimed",
            Self::Unclaimed => "unclaimed",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OfferOutcome {
    Consumed,
    Revoked,
    Replaced,
    Expired,
}

impl OfferOutcome {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Consumed => "consumed",
            Self::Revoked => "revoked",
            Self::Replaced => "replaced",
            Self::Expired => "expired",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "consumed" => Some(Self::Consumed),
            "revoked" => Some(Self::Revoked),
            "replaced" => Some(Self::Replaced),
            "expired" => Some(Self::Expired),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReenrollmentMode {
    Graceful,
    Emergency,
}

impl ReenrollmentMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Graceful => "graceful",
            Self::Emergency => "emergency",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "graceful" => Some(Self::Graceful),
            "emergency" => Some(Self::Emergency),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthorityTransition {
    InitialOfferIssued,
    OfferIssued,
    ReenrollmentStarted,
    OfferConsumed,
    OfferRevoked,
    OfferReplaced,
    OfferExpired,
    AuthorityRevoked,
    ServerDeleted,
}

impl AuthorityTransition {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::InitialOfferIssued => "initial_offer_issued",
            Self::OfferIssued => "offer_issued",
            Self::ReenrollmentStarted => "reenrollment_started",
            Self::OfferConsumed => "offer_consumed",
            Self::OfferRevoked => "offer_revoked",
            Self::OfferReplaced => "offer_replaced",
            Self::OfferExpired => "offer_expired",
            Self::AuthorityRevoked => "authority_revoked",
            Self::ServerDeleted => "server_deleted",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "initial_offer_issued" => Some(Self::InitialOfferIssued),
            "offer_issued" => Some(Self::OfferIssued),
            "reenrollment_started" => Some(Self::ReenrollmentStarted),
            "offer_consumed" => Some(Self::OfferConsumed),
            "offer_revoked" => Some(Self::OfferRevoked),
            "offer_replaced" => Some(Self::OfferReplaced),
            "offer_expired" => Some(Self::OfferExpired),
            "authority_revoked" => Some(Self::AuthorityRevoked),
            "server_deleted" => Some(Self::ServerDeleted),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutstandingOffer {
    pub id: OfferId,
    pub code_prefix: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssuedOffer {
    pub id: OfferId,
    pub code: EnrollmentCode,
    pub code_prefix: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorityState {
    pub server_id: ServerId,
    pub authority: AuthorityStatus,
    pub outstanding_offer: Option<OutstandingOffer>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimAgent {
    pub code: EnrollmentCode,
    pub proposed_run_token: ProposedRunToken,
    pub source: RequestSource,
    pub remote_addr: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClaimReceipt {
    pub server_id: ServerId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssueOfferForUnclaimed {
    pub server_id: ServerId,
    pub actor: Actor,
    pub source: RequestSource,
    pub ttl: OfferTtl,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BeginReenrollment {
    pub server_id: ServerId,
    pub mode: ReenrollmentMode,
    pub actor: Actor,
    pub source: RequestSource,
    pub ttl: OfferTtl,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplaceOffer {
    pub server_id: ServerId,
    pub offer_id: OfferId,
    pub actor: Actor,
    pub source: RequestSource,
    pub ttl: OfferTtl,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevokeOffer {
    pub server_id: ServerId,
    pub offer_id: OfferId,
    pub actor: Actor,
    pub source: RequestSource,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevokedOffer {
    pub offer_id: OfferId,
    pub already_revoked: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevokeAuthority {
    pub server_id: ServerId,
    pub actor: Actor,
    pub source: RequestSource,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevocationReceipt {
    pub server_id: ServerId,
    pub changed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryQuery {
    pub server_id: ServerId,
    pub limit: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorityEvent {
    pub id: String,
    pub server_id: ServerId,
    pub server_name: String,
    pub actor_kind: ActorKind,
    pub actor_id: Option<String>,
    pub request_source: String,
    pub offer_id: Option<OfferId>,
    pub transition: AuthorityTransition,
    pub mode: Option<ReenrollmentMode>,
    pub offer_outcome: Option<OfferOutcome>,
    pub authority_before: AuthorityStatus,
    pub authority_after: AuthorityStatus,
    pub created_at: DateTime<Utc>,
}

pub struct NewConnection {
    pub tx: mpsc::Sender<ServerMessage>,
    pub remote_addr: SocketAddr,
}

impl fmt::Debug for NewConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NewConnection")
            .field("tx", &"<channel>")
            .field("remote_addr", &self.remote_addr)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmittedConnection {
    pub server_id: ServerId,
    pub server_name: String,
    pub server_capabilities: i32,
    pub connection_id: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ClaimError {
    #[error("enrollment claim rejected")]
    Rejected,
    #[error("agent authority store failed: {0}")]
    Store(#[from] AppError),
}

#[derive(Debug, thiserror::Error)]
pub enum IssueOfferError {
    #[error("server not found")]
    NotFound,
    #[error("server is already claimed")]
    AlreadyClaimed,
    #[error("an Outstanding enrollment offer already exists")]
    OutstandingExists(OutstandingOffer),
    #[error("agent authority store failed: {0}")]
    Store(#[from] AppError),
}

#[derive(Debug, thiserror::Error)]
pub enum ReenrollmentError {
    #[error("server not found")]
    NotFound,
    #[error("server is Unclaimed")]
    Unclaimed,
    #[error("an Outstanding enrollment offer already exists")]
    OutstandingExists(OutstandingOffer),
    #[error("agent authority store failed: {0}")]
    Store(#[from] AppError),
}

#[derive(Debug, thiserror::Error)]
pub enum ReplaceOfferError {
    #[error("server not found")]
    ServerNotFound,
    #[error("enrollment offer not found")]
    OfferNotFound,
    #[error("enrollment offer is no longer Outstanding: {outcome:?}")]
    NotOutstanding {
        outcome: OfferOutcome,
        current: Option<OutstandingOffer>,
    },
    #[error("the exact offer is not the current Outstanding offer")]
    Stale { current: Option<OutstandingOffer> },
    #[error("agent authority store failed: {0}")]
    Store(#[from] AppError),
}

#[derive(Debug, thiserror::Error)]
pub enum RevokeOfferError {
    #[error("server not found")]
    ServerNotFound,
    #[error("enrollment offer not found")]
    OfferNotFound,
    #[error("enrollment offer is terminal and cannot be revoked: {0:?}")]
    Terminal(OfferOutcome),
    #[error("agent authority store failed: {0}")]
    Store(#[from] AppError),
}

#[derive(Debug, thiserror::Error)]
pub enum RevokeAuthorityError {
    #[error("server not found")]
    NotFound,
    #[error("agent authority store failed: {0}")]
    Store(#[from] AppError),
}

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("server not found")]
    NotFound,
    #[error("agent authority store failed: {0}")]
    Store(#[from] AppError),
}

#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("agent authority store failed: {0}")]
    Store(#[from] AppError),
}

#[derive(Debug, thiserror::Error)]
pub enum AdmissionError {
    #[error("agent run token rejected")]
    Rejected,
    #[error("agent authority store failed: {0}")]
    Store(#[from] AppError),
}
