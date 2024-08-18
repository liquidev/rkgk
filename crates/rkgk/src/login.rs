use std::{
    error::Error,
    fmt::{self},
    str::FromStr,
};

use rand::RngCore;

pub mod database;

pub use database::Database;
use serde::{Deserialize, Serialize};

use crate::{id, serialization::DeserializeFromStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserId(pub [u8; 32]);

impl UserId {
    pub fn new(rng: &mut dyn RngCore) -> Self {
        let mut bytes = [0; 32];
        rng.fill_bytes(&mut bytes[..]);
        Self(bytes)
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        id::serialize(f, "user_", &self.0)
    }
}

impl FromStr for UserId {
    type Err = InvalidUserId;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        id::deserialize(s, "user_")
            .map(Self)
            .map_err(|_| InvalidUserId)
    }
}

impl Serialize for UserId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for UserId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(DeserializeFromStr::new("user ID"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidUserId;

impl fmt::Display for InvalidUserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid user ID")
    }
}

impl Error for InvalidUserId {}
