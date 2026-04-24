//! Filesystem tools: read, write, edit, glob, grep.
//!
//! Byte-exact semantics with sanitize rules in atd-protocol; tree-walk
//! uses `ignore` for gitignore-aware traversal and `grep-*` for content
//! search.

pub mod edit;
pub mod glob;
pub mod grep;
pub mod read;
pub mod shared;
pub mod write;

pub use edit::FsEditTool;
pub use glob::FsGlobTool;
pub use grep::FsGrepTool;
pub use read::FsReadTool;
pub use write::FsWriteTool;
