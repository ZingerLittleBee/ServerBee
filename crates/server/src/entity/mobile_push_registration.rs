use sea_orm::entity::prelude::*;

// APNs tokens are variable-length bytes encoded as lowercase hex. This is a
// resource bound, not an assumed provider token size.
const MAX_DEVICE_TOKEN_CHARS: usize = 1024;

/// Encrypted push registrations are separate from legacy direct-APNs tokens.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "mobile_push_registrations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub installation_id: String,
    pub user_id: String,
    pub mobile_session_id: String,
    pub revision: i64,
    pub enabled: bool,
    pub alerts: bool,
    pub security: bool,
    pub task_failure: bool,
    pub task_success: bool,
    pub device_token: Option<String>,
    pub environment: Option<String>,
    pub content_key_id: Option<String>,
    pub content_key: Option<String>,
    pub deployment_id: Option<String>,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}

/// The same complete-registration check protects setup status and queue admission.
impl Model {
    pub fn is_registered(&self) -> bool {
        self.enabled
            && match (
                self.device_token.as_deref(),
                self.environment.as_deref(),
                self.content_key_id.as_deref(),
                self.content_key.as_deref(),
                self.deployment_id.as_deref(),
            ) {
                (Some(token), Some(environment), Some(key_id), Some(key), Some(deployment)) => {
                    valid_registration(token, environment, key_id, key, deployment)
                }
                _ => false,
            }
    }
}

pub fn valid_registration(
    token: &str,
    environment: &str,
    content_key_id: &str,
    content_key: &str,
    deployment_id: &str,
) -> bool {
    use base64::{Engine, engine::general_purpose::STANDARD};
    (2..=MAX_DEVICE_TOKEN_CHARS).contains(&token.len())
        && token.len().is_multiple_of(2)
        && token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && matches!(environment, "sandbox" | "production")
        && uuid::Uuid::parse_str(content_key_id).is_ok_and(|id| id.to_string() == content_key_id)
        && STANDARD
            .decode(content_key)
            .is_ok_and(|key| key.len() == 32)
        && deployment_id.len() <= 512
        && url::Url::parse(deployment_id).is_ok_and(|url| {
            url.host_str().is_some()
                && (url.scheme() == "https"
                    || (url.scheme() == "http"
                        && url.host_str() == Some("127.0.0.1")
                        && url.port().is_some()))
        })
}
