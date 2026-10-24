use wkit_core::version::Flavor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotValue {
  Version(Flavor),
  Literal(&'static str),
}
