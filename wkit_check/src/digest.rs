#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Digest(pub u64, pub u64);

impl Digest {
  pub fn of(bytes: &[u8]) -> Self {
    let hash = |seed: u8| {
      let mut hasher = std::hash::DefaultHasher::new();
      std::hash::Hasher::write_u8(&mut hasher, seed);
      std::hash::Hasher::write(&mut hasher, bytes);
      std::hash::Hasher::finish(&hasher)
    };
    Self(hash(0), hash(1))
  }

  pub fn of_file(path: &std::path::Path) -> Option<Self> {
    std::fs::read(path).ok().map(|bytes| Self::of(&bytes))
  }
}

impl std::fmt::Display for Digest {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{:016x}{:016x}", self.0, self.1)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn digests_are_stable_and_distinct() {
    assert_eq!(Digest::of(b"a"), Digest::of(b"a"));
    assert_ne!(Digest::of(b"a"), Digest::of(b"b"));
    assert_eq!(Digest::of(b"a").to_string().len(), 32);
  }
}
