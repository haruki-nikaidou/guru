//! Business logic: canvas CRUD, topology rules, config derivation and rollout.
//!
//! Every config-affecting operation follows the same pipeline:
//!
//! 1. authorize the actor,
//! 2. resolve the canvas it targets,
//! 3. load the current [`CanvasTopology`](crate::entities::surreal::topology::CanvasTopology),
//! 4. validate the topology the change *would* produce and reject it before writing,
//! 5. allocate a global revision,
//! 6. write the rows,
//! 7. re-derive every server of the canvas ([`rollout::stamp_canvas`]).

pub mod agent;
pub mod canvas;
pub mod derive;
pub mod edge;
pub mod node;
pub mod rollout;
pub mod server;
pub mod topology;
pub mod watch;

use crate::services::derive::DeriveError;
use crate::services::topology::TopologyError;

#[derive(Debug, thiserror::Error)]
pub enum OrchestrationError {
    #[error(transparent)]
    Core(#[from] wakuwaku::Error),
    #[error(transparent)]
    Db(#[from] surrealdb::Error),
    #[error("topology: {0}")]
    Topology(#[from] Box<TopologyError>),
    #[error("derive: {0}")]
    Derive(#[from] DeriveError),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Conflict(String),
    #[error("not found")]
    NotFound,
    #[error("permission denied")]
    PermissionDenied,
}

impl From<OrchestrationError> for tonic::Status {
    fn from(error: OrchestrationError) -> Self {
        match error {
            OrchestrationError::Core(e) => tonic::Status::from(e),
            OrchestrationError::Db(e) => {
                tracing::error!(error = %e, "database error");
                tonic::Status::internal("Database error")
            }
            OrchestrationError::Topology(e) => tonic::Status::failed_precondition(e.to_string()),
            OrchestrationError::Derive(e) => tonic::Status::failed_precondition(e.to_string()),
            OrchestrationError::Invalid(message) => tonic::Status::invalid_argument(message),
            OrchestrationError::Conflict(message) => tonic::Status::failed_precondition(message),
            OrchestrationError::NotFound => tonic::Status::not_found("Not found"),
            OrchestrationError::PermissionDenied => {
                tonic::Status::permission_denied("Permission denied")
            }
        }
    }
}
