mod change;
mod document;
mod error;
mod locator;
mod mismatch;
mod slot;
mod slot_value;
mod version_set;

pub use self::{
  change::Change, document::VersionDocument, error::Error, mismatch::Mismatch, slot::Slot,
  slot_value::SlotValue, version_set::VersionSet,
};

pub type Result<T, E = Error> = std::result::Result<T, E>;
