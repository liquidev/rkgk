use std::{fmt::Display, marker::PhantomData, str::FromStr};

use serde::de::{Error, Visitor};

pub struct DeserializeFromStr<T> {
    expecting: &'static str,
    _phantom: PhantomData<T>,
}

impl<T> DeserializeFromStr<T> {
    pub fn new(expecting: &'static str) -> Self {
        Self {
            expecting,
            _phantom: PhantomData,
        }
    }
}

impl<'de, T> Visitor<'de> for DeserializeFromStr<T>
where
    T: FromStr,
    T::Err: Display,
{
    type Value = T;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str(self.expecting)
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        T::from_str(v).map_err(|e| Error::custom(e))
    }
}
