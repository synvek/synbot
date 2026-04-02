//! Serde helpers for timestamp fields in persisted JSON.
//!
//! We store internal timestamps as `DateTime<Utc>` but serialize them as RFC3339
//! strings with the machine's **local timezone offset**, so the JSON matches
//! the operator's wall-clock time.

use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Deserializer, Serializer};

pub mod rfc3339_local {
    use super::*;

    pub fn serialize<S>(dt: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let local: DateTime<Local> = DateTime::from(*dt);
        serializer.serialize_str(&local.to_rfc3339())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let parsed = DateTime::parse_from_rfc3339(&s).map_err(serde::de::Error::custom)?;
        Ok(parsed.with_timezone(&Utc))
    }
}

