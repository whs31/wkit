#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
  Done,
  Fresh,
  Warning,
  Failed,
}
