//! # `orchestration` — canvases, servers, nodes and worker rollout
//!
//! This module owns the control plane of the proxy fabric: operators build a
//! **canvas** of servers and nodes, the module validates that topology, derives one
//! `guru-worker` config per server from it, and streams every new revision to the
//! workers that registered for it.
//!
//! ## Module layout
//!
//! - [`entities`] — persistence layer. SurrealDB row types plus one `Processor` per
//!   query in [`entities::surreal`], and Redis key/value types in
//!   [`entities::redis`].
//! - [`services`] — business logic: canvas/server/node/edge CRUD, the topology
//!   checker, the config deriver, rollout stamping and the worker agent.
//! - [`rpc`] — the transport edge: the operator `Orchestration` service and the
//!   `WorkerAgent` service workers talk to, plus their middleware.
//! - [`events`] — AMQP payloads this module publishes or consumes.
//! - [`hooks`] — background reactors, notably the RCU garbage collector.
//! - [`config`] — typed module configuration.
//! - [`utils`] — record-id conversion helpers shared by the edge.
//!
//! ## RCU
//!
//! Nodes and edges are never mutated in place: a spec change writes a replacement
//! row and stamps the old one with the global revision that retired it. A retired
//! row is deleted only once no retained `orchestration_server_config_revision`
//! references it, i.e. once every server that ran it has moved on.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![warn(clippy::arithmetic_side_effects)]

pub mod config;
pub mod entities;
pub mod events;
pub mod hooks;
pub mod rpc;
pub mod services;
pub mod utils;
