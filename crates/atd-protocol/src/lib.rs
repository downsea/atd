//! ATD protocol layer — the spec.
//!
//! Shared between SDK (`atd-sdk`) and runtime (`atd-runtime`); depends on
//! neither. Contains types, wire codec, and sanitization rules that third-
//! party implementations must match byte-for-byte.

pub mod enums;
pub mod error;
pub mod messages;
pub mod result;
pub mod sanitize;
pub mod summary;
pub mod tool;
pub mod wire;

pub use enums::{BindingProtocol, SafetyLevel, ToolTier, ToolVisibility, TrustLevel};
pub use error::AtdError;
pub use messages::{
    ERR_AUDIENCE_MISMATCH, ERR_BROKER_FAILED, ERR_CAPABILITY_DENIED, ERR_DELEGATION_TOO_DEEP,
    ERR_RATE_LIMITED, ERR_UCAN_EXPIRED, ERR_UCAN_INVALID, Request, Response,
};
pub use result::{ToolResult, ToolResultMetadata};
pub use sanitize::{desanitize_tool_name, detect_collisions, sanitize_tool_name};
pub use summary::ToolSummary;
pub use tool::{
    ToolBinding, ToolCapability, ToolDefinition, ToolErrorDef, ToolResources, ToolSafety, ToolTrust,
};
pub use wire::{
    WireError, read_frame, read_frame_with_deadline, write_frame, write_frame_with_deadline,
};
