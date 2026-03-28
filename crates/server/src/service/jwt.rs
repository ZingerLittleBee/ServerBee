use chrono::Utc;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct AccessTokenClaims {
    pub sub: String,
    pub username: String,
    pub role: String,
    pub exp: i64,
    pub iat: i64,
}

pub struct JwtService {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    access_ttl: i64,
}

impl JwtService {
    pub fn new(secret: &str, access_ttl: i64) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
            access_ttl,
        }
    }

    pub fn create_access_token(
        &self,
        user_id: &str,
        username: &str,
        role: &str,
    ) -> Result<(String, i64), jsonwebtoken::errors::Error> {
        let now = Utc::now().timestamp();
        let exp = now + self.access_ttl;
        let claims = AccessTokenClaims {
            sub: user_id.to_string(),
            username: username.to_string(),
            role: role.to_string(),
            exp,
            iat: now,
        };
        let token = encode(&Header::default(), &claims, &self.encoding_key)?;
        Ok((token, self.access_ttl))
    }

    pub fn validate_access_token(
        &self,
        token: &str,
    ) -> Result<AccessTokenClaims, jsonwebtoken::errors::Error> {
        let data = decode::<AccessTokenClaims>(token, &self.decoding_key, &Validation::default())?;
        Ok(data.claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_validate_access_token() {
        let svc = JwtService::new("test-secret-key-for-unit-tests", 900);
        let (token, ttl) = svc
            .create_access_token("user1", "admin_user", "admin")
            .unwrap();
        assert_eq!(ttl, 900);
        let claims = svc.validate_access_token(&token).unwrap();
        assert_eq!(claims.sub, "user1");
        assert_eq!(claims.username, "admin_user");
        assert_eq!(claims.role, "admin");
    }

    #[test]
    fn test_expired_token_rejected() {
        use jsonwebtoken::{EncodingKey, Header};

        let secret = "test-secret";
        let svc = JwtService::new(secret, 900);

        // Create a token that's already expired (exp in the past)
        let claims = AccessTokenClaims {
            sub: "user1".to_string(),
            username: "u".to_string(),
            role: "admin".to_string(),
            exp: chrono::Utc::now().timestamp() - 120, // 120 seconds in the past (past default 60s leeway)
            iat: chrono::Utc::now().timestamp() - 180,
        };
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap();
        assert!(svc.validate_access_token(&token).is_err());
    }

    #[test]
    fn test_wrong_secret_rejected() {
        let svc1 = JwtService::new("secret-a", 900);
        let svc2 = JwtService::new("secret-b", 900);
        let (token, _) = svc1.create_access_token("user1", "u", "admin").unwrap();
        assert!(svc2.validate_access_token(&token).is_err());
    }
}
