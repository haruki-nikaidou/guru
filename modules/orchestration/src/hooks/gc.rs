//! The RCU garbage collector.
//!
//! A retired node or edge is deleted only once no retained
//! `orchestration_server_config_revision` row references it, i.e. once every
//! server that ran it has applied a newer config. The master runs this on its
//! cron worker.

use crate::entities::surreal::revision::{CollectRcuGarbage, GcReport};
use crate::services::OrchestrationError;
use kanau::processor::Processor;
use wakuwaku::surreal::SurrealProcessor;

#[derive(Clone)]
pub struct RcuGarbageCollector {
    pub db: SurrealProcessor,
}

pub struct GcTick;

impl Processor<GcTick> for RcuGarbageCollector {
    type Output = GcReport;
    type Error = OrchestrationError;
    #[tracing::instrument(name = "Hook:RcuGarbageCollector", skip_all, err)]
    async fn process(&self, _input: GcTick) -> Result<Self::Output, Self::Error> {
        let report = self.db.process(CollectRcuGarbage {}).await?;
        if report.nodes != 0 || report.edges != 0 {
            tracing::info!(
                nodes = report.nodes,
                edges = report.edges,
                "collected retired rows"
            );
        }
        Ok(report)
    }
}
