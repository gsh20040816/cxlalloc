pub mod allocator;
pub mod index;
pub mod worker;

use core::marker::PhantomData;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

pub use allocator::Allocator;
pub use index::Index;

use bon::Builder;
use serde::de::DeserializeOwned;
use serde::de::IntoDeserializer as _;
use serde::Deserialize;
use serde::Serialize;

#[derive(Builder, Clone, Debug, Deserialize, Serialize)]
#[builder(state_mod(name = "config", vis = "pub"), derive(Clone, Debug))]
pub struct Config {
    #[builder(default = date())]
    date: u128,
    pub global: allocator_bench::config::Global,
    allocator: allocator_bench::allocator::Config<serde_json::Value>,
    benchmark: allocator_bench::benchmark::Config,
}

impl Config {
    pub fn with_process_id(&self, process_id: usize) -> worker::Config {
        worker::Config {
            date: self.date,
            process: self.global.with_process_id(process_id),
            allocator: self.allocator.clone(),
            benchmark: self.benchmark.clone(),
        }
    }
}

fn date() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

// TOML doesn't have a native null value
#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
#[serde(transparent)]
pub struct TomlOption<T: DeserializeOwned>(
    #[serde(deserialize_with = "empty_string_as_none")] pub Option<T>,
);

// https://github.com/serde-rs/serde/issues/1425#issuecomment-462282398
fn empty_string_as_none<'de, D, T>(de: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    de.deserialize_any(Visitor::<T>(PhantomData))
}

struct Visitor<T>(PhantomData<T>);

impl<'de, T: serde::Deserialize<'de>> serde::de::Visitor<'de> for Visitor<T> {
    type Value = Option<T>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "empty string or {}", std::any::type_name::<T>())
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        match v {
            "" => Ok(None),
            _ => T::deserialize(v.into_deserializer()).map(Some),
        }
    }

    fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        T::deserialize(serde::de::value::MapAccessDeserializer::new(map)).map(Some)
    }
}
