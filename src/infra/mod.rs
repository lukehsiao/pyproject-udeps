//! Low-level infrastructure wrappers.
//!
//! Each wrapper abstracts one technology and offers `create()` (real) and,
//! in test builds, `create_null(...)` (embedded stub, no external I/O).
//! Everything above these wrappers is our code and runs for real in tests.

pub mod env;
pub mod fs;
pub mod nullable;
pub mod process;
