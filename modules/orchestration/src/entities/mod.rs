//! Persistence layer: data types and the processors that read/write them.
//!
//! Entities live in one backing store:
//!
//! - [`surreal`] — SurrealDB rows and the queries that operate on them.
//!
//! Each query or command is a small input struct with a `Processor`
//! implementation, so persistence logic stays testable and composable.

pub mod surreal;
