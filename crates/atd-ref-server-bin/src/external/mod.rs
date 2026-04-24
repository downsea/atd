//! External-subprocess tools. Each module here demonstrates a `CliBinding`
//! rather than an in-process `Tool::call` implementation. SP-12 ships one
//! (`uname`) so the binding machinery has at least one real user; future
//! SPs can add more.

pub mod uname;
