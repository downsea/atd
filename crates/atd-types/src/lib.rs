//! ATD protocol types — independent reimplementation.
//!
//! This crate must have zero runtime dependency on any `anos-*` crate.

pub mod enums;

pub use enums::{BindingProtocol, SafetyLevel, ToolTier, ToolVisibility, TrustLevel};
