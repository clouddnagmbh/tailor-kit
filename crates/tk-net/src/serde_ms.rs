// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

//! Serializes `Duration` as fractional milliseconds, so JSON output reads `"ttfb": 12.5`.

use std::time::Duration;

use serde::{Deserialize, Deserializer, Serializer};

pub fn serialize<S: Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_f64(d.as_secs_f64() * 1000.0)
}

pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
    let ms = f64::deserialize(d)?;
    Duration::try_from_secs_f64(ms / 1000.0).map_err(serde::de::Error::custom)
}

pub mod option {
    use super::*;

    pub fn serialize<S: Serializer>(d: &Option<Duration>, s: S) -> Result<S::Ok, S::Error> {
        match d {
            Some(d) => super::serialize(d, s),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Duration>, D::Error> {
        match Option::<f64>::deserialize(d)? {
            Some(ms) => Duration::try_from_secs_f64(ms / 1000.0)
                .map(Some)
                .map_err(serde::de::Error::custom),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct T {
        #[serde(with = "crate::serde_ms")]
        a: Duration,
        #[serde(with = "crate::serde_ms::option")]
        b: Option<Duration>,
    }

    #[test]
    fn round_trips_as_milliseconds() {
        let t = T {
            a: Duration::from_micros(12_500),
            b: None,
        };
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, r#"{"a":12.5,"b":null}"#);
        assert_eq!(serde_json::from_str::<T>(&json).unwrap(), t);
    }
}
