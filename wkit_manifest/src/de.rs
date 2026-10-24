#[derive(Debug)]
pub(crate) enum Shorthand<T> {
  Str(String),
  Seq(Vec<String>),
  Map(T),
}

struct ShorthandVisitor<T>(std::marker::PhantomData<T>);

impl<'de, T: serde::Deserialize<'de>> serde::de::Visitor<'de> for ShorthandVisitor<T> {
  type Value = Shorthand<T>;

  fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str("a string, an array of strings or a table")
  }

  fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
    Ok(Shorthand::Str(v.to_owned()))
  }

  fn visit_seq<A: serde::de::SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
    <Vec<String> as serde::Deserialize>::deserialize(serde::de::value::SeqAccessDeserializer::new(
      seq,
    ))
    .map(Shorthand::Seq)
  }

  fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
    T::deserialize(serde::de::value::MapAccessDeserializer::new(map)).map(Shorthand::Map)
  }
}

impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for Shorthand<T> {
  fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
    deserializer.deserialize_any(ShorthandVisitor(std::marker::PhantomData))
  }
}
