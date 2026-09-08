//! The transport edge: gRPC services and their middleware.
//!
//! Two services on two ports: [`orchestration_service::OrchestrationGrpc`] for
//! operators (authenticated by `auth`'s session/API-key middleware) and
//! [`agent_service::WorkerAgentGrpc`] for workers (registration with an operator
//! API key, then a dynamic refresh key handled by [`agent_middleware`]).

pub mod agent_middleware;
pub mod agent_service;
pub mod orchestration_service;

pub use agent_service::WorkerAgentGrpc;
pub use orchestration_service::OrchestrationGrpc;
