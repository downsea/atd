//! `atd-cli` library side — exposes subcommand modules so integration tests can
//! drive them with captured output buffers.

pub mod call;
pub mod cli;
pub mod connect;
pub mod list;
pub mod schema;
