use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::Rhythm;

#[derive(Serialize, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub timezone: String,
}

#[derive(Serialize, Deserialize)]
pub struct RegisterResponse {
    pub user_id: String,
}

#[derive(Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize, Deserialize)]
pub struct LoginResponse {
    pub token: String,
}

#[derive(Serialize, Deserialize)]
pub struct CreateRhythmRequest {
    pub rhythm: Rhythm,
    pub description: String,
}

#[derive(Serialize, Deserialize)]
pub struct ScheduleItem {
    pub rhythm_id: String,
    pub description: String,
    pub datetime: DateTime<Utc>,
    pub stretch_goal: bool,
}

#[derive(Serialize, Deserialize)]
pub struct DelinquentItem {
    pub rhythm_id: String,
    pub description: String,
}

#[derive(Serialize, Deserialize)]
pub struct MarkDoneRequest {
    pub when: Option<DateTime<Utc>>,
}

#[derive(Serialize, Deserialize)]
pub struct DeferRequest {
    pub when: Option<DateTime<Utc>>,
}

#[derive(Serialize, Deserialize)]
pub struct SetSpoonsRequest {
    pub date: NaiveDate,
    pub value: u8,
}

#[derive(Serialize, Deserialize)]
pub struct UserResponse {
    pub timezone: String,
}

#[derive(Serialize, Deserialize)]
pub struct ConvergenceResponse {
    pub datetime: Option<DateTime<Utc>>,
}

#[derive(Serialize, Deserialize)]
pub struct RhythmResponse {
    pub id: String,
    pub rhythm: Rhythm,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub created_at_tz: String,
    pub modified_at: DateTime<Utc>,
    pub modified_at_tz: String,
}

#[derive(Serialize, Deserialize)]
pub struct ScheduleQuery {
    pub start: NaiveDate,
    pub days: u32,
}

#[derive(Serialize, Deserialize)]
pub struct RhythmWithTimestamp {
    pub id: String,
    pub rhythm: Rhythm,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize)]
pub struct SpoonsQuery {
    pub date: Option<NaiveDate>,
}

#[derive(Serialize, Deserialize)]
pub struct SpoonsResponse {
    pub date: NaiveDate,
    pub value: u8,
}

#[derive(Serialize, Deserialize)]
pub struct UpdateUserRequest {
    pub timezone: String,
}
