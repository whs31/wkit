use wkit_core::version::{Flavor, ProjectVersion};

use crate::SlotValue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
  pub span: std::ops::Range<usize>,
  pub value: SlotValue,
}

impl Slot {
  pub fn version(span: std::ops::Range<usize>, flavor: Flavor) -> Self {
    Self {
      span,
      value: SlotValue::Version(flavor),
    }
  }

  pub fn literal(span: std::ops::Range<usize>, literal: &'static str) -> Self {
    Self {
      span,
      value: SlotValue::Literal(literal),
    }
  }

  pub fn flavor(&self) -> Option<Flavor> {
    match self.value {
      SlotValue::Version(flavor) => Some(flavor),
      SlotValue::Literal(_) => None,
    }
  }

  pub fn render(&self, version: &ProjectVersion) -> String {
    match self.value {
      SlotValue::Version(flavor) => version.render(flavor),
      SlotValue::Literal(literal) => literal.to_owned(),
    }
  }
}
