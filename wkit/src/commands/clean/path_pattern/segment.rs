#[derive(Debug, Clone)]
pub(super) enum Segment {
  Literal(String),
  Glob(globset::GlobMatcher),
}
