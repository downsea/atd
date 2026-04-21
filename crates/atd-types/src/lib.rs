//! ATD protocol types — independent reimplementation.
//!
//! This crate must have zero runtime dependency on any `anos-*` crate.

pub mod enums;

pub use enums::{BindingProtocol, SafetyLevel, ToolTier, ToolVisibility, TrustLevel};

pub mod tool;

pub use tool::{
    ToolBinding, ToolCapability, ToolDefinition, ToolResources, ToolSafety, ToolTrust,
};

pub mod summary;

pub use summary::ToolSummary;

pub mod result;

pub use result::{ToolResult, ToolResultMetadata};

pub mod error;

pub use error::AtdError;
