//! `rustberg-core` — shared domain model, error type, and the service traits
//! that every other crate in the workspace implements or depends on.
//! Nothing in here touches I/O; it is the equivalent of Amoro's
//! `rustberg-core` Java module (core abstractions, no server/runtime code).

pub mod error;
pub mod model;
pub mod traits;

pub use error::{RustbergError, RustbergResult};
