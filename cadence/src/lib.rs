use std::time::Duration;

use chrono::{DateTime, NaiveDate, NaiveTime, TimeZone, Utc};

pub mod api_types;
pub mod auth;
pub mod db;
mod rhythm;

pub use rhythm::{
    EventRecord, EventType, Rhythm, RhythmDefinition, RhythmID, RhythmManager, Slider,
};

///////////////////////////////////////////// Constants ////////////////////////////////////////////

const ONE_DAY: Duration = Duration::from_secs(86_400);

/////////////////////////////////////////////// Error //////////////////////////////////////////////

#[derive(Debug)]
pub enum Error {
    Internal(String),
    IO(std::io::Error),
    Json(serde_json::Error),
    FromUtf8Error(std::string::FromUtf8Error),
    Sqlx(sqlx::Error),
}

impl Error {
    #[allow(dead_code)]
    fn internal(s: impl Into<String>) -> Self {
        Self::Internal(s.into())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{self:#?}")
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::IO(err)
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}

impl From<std::string::FromUtf8Error> for Error {
    fn from(err: std::string::FromUtf8Error) -> Self {
        Self::FromUtf8Error(err)
    }
}

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        Self::Sqlx(err)
    }
}

////////////////////////////////////////// time utilities //////////////////////////////////////////

// Return the _earliest possible_ interpretation of `dt` as if its time component were lopped off
// and replaced with `t`.
pub fn change_time<TZ: TimeZone>(dt: DateTime<TZ>, t: NaiveTime) -> DateTime<TZ> {
    dt.with_time(t)
        .earliest()
        .unwrap_or_else(|| Utc::now().with_timezone(&dt.timezone()))
}

// Assign the _earliest possible_ interpretation of `d` as if it were at local time `t` in `Tz`.
pub fn assign_time<TZ: TimeZone>(d: NaiveDate, t: NaiveTime, tz: TZ) -> DateTime<TZ> {
    change_time(
        d.and_hms_opt(0, 0, 0)
            .expect("naive date should always have midnight")
            .and_local_timezone(tz)
            .earliest()
            .expect("should always have an earliest time"),
        t,
    )
}

///////////////////////////////////////// generate_id_serde ////////////////////////////////////////

#[macro_export]
macro_rules! generate_id_serde {
    ($name:ident, $visitor:ident) => {
        impl serde::Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                let s = self.to_string();
                serializer.serialize_str(&s)
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<$name, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                deserializer.deserialize_str($visitor)
            }
        }

        struct $visitor;

        impl<'de> serde::de::Visitor<'de> for $visitor {
            type Value = $name;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an ID")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                $name::from_human_readable(value).ok_or_else(|| E::custom("not a valid tx:UUID"))
            }
        }
    };
}
