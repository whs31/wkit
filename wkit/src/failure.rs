#[derive(Debug, thiserror::Error)]
#[error("{}", message.as_deref().unwrap_or_default())]
pub struct Failure {
  pub message: Option<String>,
  pub code: i32,
}

impl Failure {
  pub const INTERNAL: i32 = 101;

  pub fn new(message: impl Into<String>, code: i32) -> Self {
    Self {
      message: Some(message.into()),
      code,
    }
  }

  pub fn silent(code: i32) -> Self {
    Self {
      message: None,
      code,
    }
  }

  pub fn exit_code(error: &anyhow::Error) -> i32 {
    error
      .chain()
      .find_map(|cause| {
        cause
          .downcast_ref::<Self>()
          .map(|f| Some(f.code))
          .or_else(|| {
            cause
              .downcast_ref::<wkit_core::Error>()
              .map(wkit_core::Error::exit_code)
          })
          .or_else(|| {
            cause
              .downcast_ref::<wkit_version::Error>()
              .map(wkit_version::Error::exit_code)
          })
          .or_else(|| {
            cause
              .downcast_ref::<wkit_build::Error>()
              .map(wkit_build::Error::exit_code)
          })
          .or_else(|| {
            cause
              .downcast_ref::<wkit_check::Error>()
              .map(wkit_check::Error::exit_code)
          })
          .or_else(|| {
            cause
              .downcast_ref::<wkit_test::Error>()
              .map(wkit_test::Error::exit_code)
          })
          .or_else(|| {
            cause
              .downcast_ref::<wkit_package::Error>()
              .map(wkit_package::Error::exit_code)
          })
          .or_else(|| {
            cause
              .downcast_ref::<wkit_ws::Error>()
              .map(wkit_ws::Error::exit_code)
          })
          .flatten()
      })
      .unwrap_or(Self::INTERNAL)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn exit_codes_come_from_failures_and_children() {
    assert_eq!(Failure::exit_code(&Failure::silent(3).into()), 3);
    assert_eq!(
      Failure::exit_code(&anyhow::Error::from(Failure::new("x", 1)).context("outer")),
      1
    );
    assert_eq!(
      Failure::exit_code(&anyhow::anyhow!("internal")),
      Failure::INTERNAL
    );
    let missing = wkit_core::Error::ProgramNotFound {
      program: "x".into(),
    };
    assert_eq!(Failure::exit_code(&missing.into()), Failure::INTERNAL);
  }
}
