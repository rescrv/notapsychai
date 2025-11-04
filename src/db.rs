use std::collections::HashMap;
use std::str::FromStr;

use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{EventRecord, EventType, Rhythm, RhythmDefinition, RhythmID, RhythmManager};

pub async fn create_user(
    pool: &PgPool,
    username: &str,
    email: &str,
    password_hash: &str,
    timezone: &str,
) -> Result<Uuid, crate::Error> {
    let row = sqlx::query(
        "INSERT INTO users (username, email, password_hash, default_timezone)
         VALUES ($1, $2, $3, $4)
         RETURNING id",
    )
    .bind(username)
    .bind(email)
    .bind(password_hash)
    .bind(timezone)
    .fetch_one(pool)
    .await?;

    Ok(row.get("id"))
}

pub async fn get_user_by_username(
    pool: &PgPool,
    username: &str,
) -> Result<Option<(Uuid, String, String)>, crate::Error> {
    let result =
        sqlx::query("SELECT id, password_hash, default_timezone FROM users WHERE username = $1")
            .bind(username)
            .fetch_optional(pool)
            .await?;

    Ok(result.map(|row| {
        (
            row.get("id"),
            row.get("password_hash"),
            row.get("default_timezone"),
        )
    }))
}

pub async fn get_user_timezone(pool: &PgPool, user_id: Uuid) -> Result<String, crate::Error> {
    let row = sqlx::query("SELECT default_timezone FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(pool)
        .await?;

    Ok(row.get("default_timezone"))
}

pub async fn update_user_timezone(
    pool: &PgPool,
    user_id: Uuid,
    timezone: &str,
) -> Result<(), crate::Error> {
    sqlx::query("UPDATE users SET default_timezone = $1 WHERE id = $2")
        .bind(timezone)
        .bind(user_id)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn load_rhythm_manager(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<RhythmManager, crate::Error> {
    let timezone = get_user_timezone(pool, user_id).await?;

    let rhythms = load_rhythms(pool, user_id).await?;
    let events = load_events(pool, user_id).await?;
    let spoons = load_spoons(pool, user_id).await?;

    RhythmManager::new(user_id, &timezone, rhythms, events, spoons)
}

pub async fn load_rhythms(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<RhythmDefinition>, crate::Error> {
    let rows = sqlx::query(
        "SELECT id, rhythm_type, rhythm_data, description, created_at, created_at_tz, modified_at, modified_at_tz
         FROM rhythms
         WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut rhythms = Vec::new();
    for row in rows {
        let uuid: Uuid = row.get("id");
        let id = uuid.try_into()?;

        let rhythm_json: serde_json::Value = row.get("rhythm_data");
        let rhythm: Rhythm = serde_json::from_value(rhythm_json)?;

        rhythms.push(RhythmDefinition {
            id,
            rhythm,
            description: row.get("description"),
            created_at: row.get("created_at"),
            created_at_tz: row.get("created_at_tz"),
            modified_at: row.get("modified_at"),
            modified_at_tz: row.get("modified_at_tz"),
        });
    }

    Ok(rhythms)
}

pub async fn save_rhythm(
    pool: &PgPool,
    user_id: Uuid,
    rhythm: &RhythmDefinition,
) -> Result<(), crate::Error> {
    let rhythm_json = serde_json::to_value(rhythm.rhythm)?;
    let uuid: Uuid = rhythm.id.try_into()?;

    sqlx::query(
        "INSERT INTO rhythms (id, user_id, rhythm_type, rhythm_data, description, created_at, created_at_tz, modified_at, modified_at_tz)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT (id) DO UPDATE SET
         rhythm_data = EXCLUDED.rhythm_data,
         description = EXCLUDED.description,
         modified_at = EXCLUDED.modified_at,
         modified_at_tz = EXCLUDED.modified_at_tz",
    )
    .bind(uuid)
    .bind(user_id)
    .bind(rhythm.rhythm.type_name())
    .bind(rhythm_json)
    .bind(&rhythm.description)
    .bind(rhythm.created_at)
    .bind(&rhythm.created_at_tz)
    .bind(rhythm.modified_at)
    .bind(&rhythm.modified_at_tz)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn delete_rhythm(
    pool: &PgPool,
    user_id: Uuid,
    rhythm_id: RhythmID,
) -> Result<(), crate::Error> {
    let uuid: Uuid = rhythm_id.try_into()?;

    sqlx::query("DELETE FROM rhythms WHERE id = $1 AND user_id = $2")
        .bind(uuid)
        .bind(user_id)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn load_events(pool: &PgPool, user_id: Uuid) -> Result<Vec<EventRecord>, crate::Error> {
    let rows = sqlx::query(
        "SELECT rhythm_id, event_type, when_utc, when_tz
         FROM events
         WHERE user_id = $1
         ORDER BY when_utc",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut events = Vec::new();
    for row in rows {
        let rhythm_uuid: Uuid = row.get("rhythm_id");
        let rhythm_id = rhythm_uuid.try_into()?;

        let event_type_str: String = row.get("event_type");
        let event_type = EventType::from_str(&event_type_str).map_err(crate::Error::Internal)?;

        events.push(EventRecord {
            rhythm_id,
            event_type,
            when: row.get("when_utc"),
            when_tz: row.get("when_tz"),
        });
    }

    Ok(events)
}

pub async fn save_event(
    pool: &PgPool,
    user_id: Uuid,
    event: &EventRecord,
) -> Result<(), crate::Error> {
    let rhythm_uuid: Uuid = event.rhythm_id.try_into()?;
    let event_type_str = event.event_type.as_str();

    sqlx::query(
        "INSERT INTO events (user_id, rhythm_id, event_type, when_utc, when_tz)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(user_id)
    .bind(rhythm_uuid)
    .bind(event_type_str)
    .bind(event.when)
    .bind(&event.when_tz)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn load_spoons(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<HashMap<NaiveDate, u8>, crate::Error> {
    let rows = sqlx::query("SELECT date, value FROM spoons WHERE user_id = $1")
        .bind(user_id)
        .fetch_all(pool)
        .await?;

    let mut spoons = HashMap::new();
    for row in rows {
        spoons.insert(row.get("date"), row.get::<i16, _>("value") as u8);
    }

    Ok(spoons)
}

pub async fn save_spoons(
    pool: &PgPool,
    user_id: Uuid,
    date: NaiveDate,
    value: u8,
) -> Result<(), crate::Error> {
    sqlx::query(
        "INSERT INTO spoons (user_id, date, value)
         VALUES ($1, $2, $3)
         ON CONFLICT (user_id, date) DO UPDATE SET value = EXCLUDED.value",
    )
    .bind(user_id)
    .bind(date)
    .bind(value as i16)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn create_session(
    pool: &PgPool,
    user_id: Uuid,
    token_hash: &str,
    expires_at: DateTime<Utc>,
) -> Result<(), crate::Error> {
    sqlx::query("INSERT INTO sessions (user_id, token_hash, expires_at) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(token_hash)
        .bind(expires_at)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn get_session(
    pool: &PgPool,
    token_hash: &str,
) -> Result<Option<(Uuid, DateTime<Utc>)>, crate::Error> {
    let result = sqlx::query("SELECT user_id, expires_at FROM sessions WHERE token_hash = $1")
        .bind(token_hash)
        .fetch_optional(pool)
        .await?;

    Ok(result.map(|row| (row.get("user_id"), row.get("expires_at"))))
}

pub async fn delete_session(pool: &PgPool, token_hash: &str) -> Result<(), crate::Error> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
        .bind(token_hash)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn cleanup_expired_sessions(pool: &PgPool) -> Result<(), crate::Error> {
    sqlx::query("DELETE FROM sessions WHERE expires_at < NOW()")
        .execute(pool)
        .await?;

    Ok(())
}
