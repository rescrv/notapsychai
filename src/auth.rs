use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
}

pub fn hash_password(password: &str) -> Result<String, crate::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| crate::Error::Internal(format!("Failed to hash password: {e}")))?;
    Ok(password_hash.to_string())
}

pub fn verify_password(password: &str, password_hash: &str) -> Result<bool, crate::Error> {
    let parsed_hash = PasswordHash::new(password_hash)
        .map_err(|e| crate::Error::Internal(format!("Failed to parse password hash: {e}")))?;
    let argon2 = Argon2::default();
    Ok(argon2
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

pub fn generate_jwt(
    user_id: Uuid,
    secret: &str,
    duration_days: i64,
) -> Result<String, crate::Error> {
    let expiration = Utc::now()
        .checked_add_signed(Duration::days(duration_days))
        .ok_or_else(|| crate::Error::Internal("Failed to calculate expiration".to_string()))?;

    let claims = Claims {
        sub: user_id.to_string(),
        exp: expiration.timestamp() as usize,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| crate::Error::Internal(format!("Failed to generate JWT: {e}")))
}

pub fn validate_jwt(token: &str, secret: &str) -> Result<Claims, crate::Error> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|e| crate::Error::Internal(format!("Failed to validate JWT: {e}")))?;

    Ok(token_data.claims)
}

pub fn token_expiration(duration_days: i64) -> DateTime<Utc> {
    Utc::now()
        .checked_add_signed(Duration::days(duration_days))
        .unwrap_or_else(Utc::now)
}

pub fn hash_token_for_storage(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}
