//! Versioned AES-256-GCM envelope. No identity or notification text leaves in plaintext.
use crate::error::AppError;
use base64::{Engine, engine::general_purpose::STANDARD};
use ring::{
    aead,
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAX_CONTENT_BYTES: usize = 2048;

#[derive(Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PushEnvelope {
    pub version: u8,
    pub key_id: String,
    pub identity: String,
    pub nonce: String,
    /// Ciphertext followed by the 16-byte GCM tag (standard base64).
    pub ciphertext: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PushContent {
    pub kind: String,
    pub deployment_id: String,
    pub user_id: String,
    pub installation_id: String,
    pub event_id: String,
    pub created_at: i64,
    pub expires_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alert: Option<AlertPushTarget>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlertPushTarget {
    pub alert_key: String,
    pub status: String,
    pub rule_name: String,
    pub server_name: String,
}

pub fn identity(deployment: &str, user: &str, installation: &str) -> Result<String, AppError> {
    let canonical = serde_json::to_vec(&[deployment, user, installation])
        .map_err(|_| AppError::Internal("Push identity encoding failed".into()))?;
    Ok(format!("{:x}", Sha256::digest(canonical)))
}

pub fn encrypt(
    key_id: &str,
    secret: &[u8],
    content: &PushContent,
) -> Result<PushEnvelope, AppError> {
    let mut nonce = [0_u8; 12];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| AppError::Internal("Push nonce generation failed".into()))?;
    seal(key_id, secret, content, nonce)
}

fn seal(
    key_id: &str,
    secret: &[u8],
    content: &PushContent,
    nonce: [u8; 12],
) -> Result<PushEnvelope, AppError> {
    let mut bytes = serde_json::to_vec(content)
        .map_err(|_| AppError::Internal("Push encoding failed".into()))?;
    if bytes.len() > MAX_CONTENT_BYTES {
        return Err(AppError::Validation("Push content is too large".into()));
    }
    let identity = identity(
        &content.deployment_id,
        &content.user_id,
        &content.installation_id,
    )?;
    let aad = format!("ServerBee.Push.v1|{key_id}|{identity}");
    let key = aead::UnboundKey::new(&aead::AES_256_GCM, secret)
        .map_err(|_| AppError::Validation("Invalid push content key".into()))?;
    aead::LessSafeKey::new(key)
        .seal_in_place_append_tag(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(aad.as_bytes()),
            &mut bytes,
        )
        .map_err(|_| AppError::Internal("Push encryption failed".into()))?;
    Ok(PushEnvelope {
        version: 1,
        key_id: key_id.into(),
        identity,
        nonce: STANDARD.encode(nonce),
        ciphertext: STANDARD.encode(bytes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rust_matches_shared_swift_vector() {
        for source in [
            include_str!("../../../../tests/fixtures/push-envelope-v1.json"),
            include_str!("../../../../tests/fixtures/push-alert-envelope-v1.json"),
        ] {
            let vector: serde_json::Value = serde_json::from_str(source).expect("vector");
            let content: PushContent =
                serde_json::from_value(vector["content"].clone()).expect("content");
            let key = STANDARD
                .decode(vector["key"].as_str().expect("key"))
                .expect("key bytes");
            let nonce: [u8; 12] = STANDARD
                .decode(vector["envelope"]["nonce"].as_str().expect("nonce"))
                .expect("nonce bytes")
                .try_into()
                .expect("12 bytes");
            let sealed = seal(
                vector["envelope"]["key_id"].as_str().expect("key id"),
                &key,
                &content,
                nonce,
            )
            .expect("encrypt");
            assert_eq!(
                serde_json::to_value(sealed).expect("encode"),
                vector["envelope"]
            );
            let a = encrypt("key", &key, &content).expect("fresh a");
            let b = encrypt("key", &key, &content).expect("fresh b");
            assert_ne!(a.nonce, b.nonce);
            assert_ne!(a.ciphertext, b.ciphertext);
        }
    }
}
