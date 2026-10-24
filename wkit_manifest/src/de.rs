use std::{fmt, marker::PhantomData};

use serde::{
  Deserialize, Deserializer,
  de::{
    self, MapAccess, SeqAccess, Visitor,
    value::{MapAccessDeserializer, SeqAccessDeserializer},
  },
};

#[derive(Debug)]
pub(crate) enum Shorthand<T> {
  Str(String),
  Seq(Vec<String>),
  Map(T),
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Shorthand<T> {
  fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
    struct ShorthandVisitor<T>(PhantomData<T>);

    impl<'de, T: Deserialize<'de>> Visitor<'de> for ShorthandVisitor<T> {
      type Value = Shorthand<T>;

      fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a string, an array of strings or a table")
      }

      fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        Ok(Shorthand::Str(v.to_owned()))
      }

      fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
        Vec::deserialize(SeqAccessDeserializer::new(seq)).map(Shorthand::Seq)
      }

      fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
        T::deserialize(MapAccessDeserializer::new(map)).map(Shorthand::Map)
      }
    }

    deserializer.deserialize_any(ShorthandVisitor(PhantomData))
  }
}

pub(crate) fn version_req<'de, D: Deserializer<'de>>(
  deserializer: D,
) -> Result<Option<semver::VersionReq>, D::Error> {
  let Some(raw) = Option::<String>::deserialize(deserializer)? else {
    return Ok(None);
  };
  semver::VersionReq::parse(&raw)
    .map(Some)
    .map_err(|e| de::Error::custom(format!("invalid version requirement `{raw}`: {e}")))
}
