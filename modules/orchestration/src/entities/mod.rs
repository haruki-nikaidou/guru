//! Persistence layer: data types and the processors that read/write them.
//!
//! Entities are split by backing store:
//!
//! - [`surreal`] — SurrealDB rows and the queries that operate on them.
//! - [`redis`] — Redis key/value types used for caching and ephemeral state.
//!
//! Each query or command is a small input struct with a `Processor`
//! implementation, so persistence logic stays testable and composable.

pub mod redis;
pub mod surreal;
