//! Canvas CRUD and validation.

use crate::entities::surreal::canvas::{
    CanvasContents, CanvasEntity, CanvasId, CreateCanvas as CreateCanvasRow, DeleteCanvasRow,
    FindCanvasById, ListCanvases as ListCanvasesRow, UpdateCanvasMeta,
};
use crate::entities::surreal::topology::{LoadCanvasContents, LoadCanvasTopology};
use crate::services::OrchestrationError;
use crate::services::rollout::DirtyNotifier;
use crate::services::topology::{TopologyProblem, analyze};
use auth::services::identity::Identity;
use auth::utils::rbac::Permission;
use kanau::processor::Processor;
use wakuwaku::surreal::SurrealProcessor;

#[derive(Clone)]
pub struct CanvasService {
    pub db: SurrealProcessor,
    pub notifier: DirtyNotifier,
}

pub struct CreateCanvas {
    pub actor: Identity,
    pub name: String,
    pub description: String,
}

impl Processor<CreateCanvas> for CanvasService {
    type Output = CanvasEntity;
    type Error = OrchestrationError;
    #[tracing::instrument(name = "Service:CreateCanvas", skip_all, err)]
    async fn process(&self, input: CreateCanvas) -> Result<Self::Output, Self::Error> {
        input.actor.ensure(Permission::EditWorkspace)?;
        if input.name.trim().is_empty() {
            return Err(OrchestrationError::Invalid("name must not be empty".into()));
        }
        Ok(self
            .db
            .process(CreateCanvasRow {
                name: input.name,
                description: input.description,
            })
            .await?)
    }
}

pub struct ListCanvases {
    pub actor: Identity,
}

impl Processor<ListCanvases> for CanvasService {
    type Output = Vec<CanvasEntity>;
    type Error = OrchestrationError;
    #[tracing::instrument(name = "Service:ListCanvases", skip_all, err)]
    async fn process(&self, input: ListCanvases) -> Result<Self::Output, Self::Error> {
        input.actor.ensure(Permission::ViewWorkspace)?;
        Ok(self.db.process(ListCanvasesRow {}).await?)
    }
}

pub struct GetCanvas {
    pub actor: Identity,
    pub canvas: CanvasId,
}

impl Processor<GetCanvas> for CanvasService {
    type Output = CanvasContents;
    type Error = OrchestrationError;
    #[tracing::instrument(name = "Service:GetCanvas", skip_all, err)]
    async fn process(&self, input: GetCanvas) -> Result<Self::Output, Self::Error> {
        input.actor.ensure(Permission::ViewWorkspace)?;
        self.db
            .process(LoadCanvasContents {
                canvas: input.canvas,
            })
            .await?
            .ok_or(OrchestrationError::NotFound)
    }
}

pub struct UpdateCanvas {
    pub actor: Identity,
    pub canvas: CanvasId,
    pub name: String,
    pub description: String,
}

impl Processor<UpdateCanvas> for CanvasService {
    type Output = CanvasEntity;
    type Error = OrchestrationError;
    #[tracing::instrument(name = "Service:UpdateCanvas", skip_all, err)]
    async fn process(&self, input: UpdateCanvas) -> Result<Self::Output, Self::Error> {
        input.actor.ensure(Permission::EditWorkspace)?;
        if input.name.trim().is_empty() {
            return Err(OrchestrationError::Invalid("name must not be empty".into()));
        }
        self.db
            .process(FindCanvasById {
                id: input.canvas.clone(),
            })
            .await?
            .ok_or(OrchestrationError::NotFound)?;
        // Metadata only: no revision, no re-derivation.
        Ok(self
            .db
            .process(UpdateCanvasMeta {
                id: input.canvas,
                name: input.name,
                description: input.description,
            })
            .await?)
    }
}

pub struct DeleteCanvas {
    pub actor: Identity,
    pub canvas: CanvasId,
}

impl Processor<DeleteCanvas> for CanvasService {
    type Output = ();
    type Error = OrchestrationError;
    /// Deletes the canvas and everything under it in one transaction.
    ///
    /// Workers of the deleted servers keep running their last config: there is no
    /// canvas left to derive an empty one from, and no view row to send it through.
    /// This is the same behaviour as deleting a single server.
    #[tracing::instrument(name = "Service:DeleteCanvas", skip_all, err)]
    async fn process(&self, input: DeleteCanvas) -> Result<Self::Output, Self::Error> {
        input.actor.ensure(Permission::EditWorkspace)?;
        self.db
            .process(FindCanvasById {
                id: input.canvas.clone(),
            })
            .await?
            .ok_or(OrchestrationError::NotFound)?;
        self.db
            .process(DeleteCanvasRow { id: input.canvas })
            .await?;
        Ok(())
    }
}

pub struct ValidateCanvas {
    pub actor: Identity,
    pub canvas: CanvasId,
}

impl Processor<ValidateCanvas> for CanvasService {
    type Output = Vec<TopologyProblem>;
    type Error = OrchestrationError;
    #[tracing::instrument(name = "Service:ValidateCanvas", skip_all, err)]
    async fn process(&self, input: ValidateCanvas) -> Result<Self::Output, Self::Error> {
        input.actor.ensure(Permission::ViewWorkspace)?;
        self.db
            .process(FindCanvasById {
                id: input.canvas.clone(),
            })
            .await?
            .ok_or(OrchestrationError::NotFound)?;
        let topology = self
            .db
            .process(LoadCanvasTopology {
                canvas: input.canvas,
            })
            .await?;
        Ok(analyze(&topology))
    }
}
