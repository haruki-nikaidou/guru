//! Topology-related query processors spanning several entity kinds.

use crate::entities::surreal::canvas::{CanvasContents, CanvasEntity, CanvasId};
use crate::entities::surreal::connection::EdgeConnectionEntity;
use crate::entities::surreal::node::{NodeEntity, NodeWithPorts};
use crate::entities::surreal::port::PortEntity;
use crate::entities::surreal::server::{
    ServerEntity, ServerId, ServerIpRecordEntity, ServerWithIp,
};
use kanau::processor::Processor;
use std::collections::HashMap;
use wakuwaku::surreal::SurrealProcessor;

/// A consistent read of everything the topology checker and the config deriver need.
/// Only live rows: retired nodes and edges take part in no derivation.
#[derive(Debug, Clone)]
pub struct CanvasTopology {
    pub canvas: CanvasId,
    pub servers: Vec<ServerEntity>,
    pub ips: Vec<ServerIpRecordEntity>,
    pub nodes: Vec<NodeWithPorts>,
    pub edges: Vec<EdgeConnectionEntity>,
}

pub struct LoadCanvasTopology {
    pub canvas: CanvasId,
}

impl Processor<LoadCanvasTopology> for SurrealProcessor {
    type Output = CanvasTopology;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:LoadCanvasTopology", skip_all, err)]
    async fn process(&self, input: LoadCanvasTopology) -> Result<Self::Output, Self::Error> {
        let (servers, ips, nodes, edges) = load_canvas(self, &input.canvas, true).await?;
        Ok(CanvasTopology {
            canvas: input.canvas,
            servers,
            ips,
            nodes,
            edges,
        })
    }
}

/// Live **and** retiring rows, for rendering a rollout in flight.
pub struct LoadCanvasContents {
    pub canvas: CanvasId,
}

impl Processor<LoadCanvasContents> for SurrealProcessor {
    type Output = Option<CanvasContents>;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:LoadCanvasContents", skip_all, err)]
    async fn process(&self, input: LoadCanvasContents) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query("SELECT * FROM $canvas")
            .bind(("canvas", input.canvas.clone()))
            .await?;
        let Some(canvas) = resp.take::<Option<CanvasEntity>>(0)? else {
            return Ok(None);
        };
        let (servers, ips, nodes, edges) = load_canvas(self, &input.canvas, false).await?;
        let mut by_server: HashMap<String, Vec<ServerIpRecordEntity>> = HashMap::new();
        for ip in ips {
            by_server
                .entry(crate::utils::ids::record_key(&ip.server.0))
                .or_default()
                .push(ip);
        }
        let servers = servers
            .into_iter()
            .map(|server| {
                let ips = by_server
                    .remove(crate::utils::ids::record_key(&server.id.0).as_str())
                    .unwrap_or_default();
                ServerWithIp { server, ips }
            })
            .collect();
        Ok(Some(CanvasContents {
            canvas,
            servers,
            nodes,
            edges,
        }))
    }
}

pub struct FindCanvasOfServer {
    pub server: ServerId,
}

impl Processor<FindCanvasOfServer> for SurrealProcessor {
    type Output = Option<CanvasId>;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:FindCanvasOfServer", skip_all, err)]
    async fn process(&self, input: FindCanvasOfServer) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query("SELECT VALUE canvas FROM $server")
            .bind(("server", input.server))
            .await?;
        resp.take::<Option<CanvasId>>(0)
    }
}

type CanvasRows = (
    Vec<ServerEntity>,
    Vec<ServerIpRecordEntity>,
    Vec<NodeWithPorts>,
    Vec<EdgeConnectionEntity>,
);

/// Reads the servers, ip records, nodes (with their ports) and edges of one canvas.
/// `live_only` filters out rows that are retired but not yet garbage-collected.
async fn load_canvas(
    sp: &SurrealProcessor,
    canvas: &CanvasId,
    live_only: bool,
) -> Result<CanvasRows, surrealdb::Error> {
    let sql = if live_only {
        include_str!("../../../sql/topology/load_canvas_live.surql")
    } else {
        include_str!("../../../sql/topology/load_canvas_all.surql")
    };
    let mut resp = sp.db().query(sql).bind(("canvas", canvas.clone())).await?;
    let servers = resp.take::<Vec<ServerEntity>>(0)?;
    let ips = resp.take::<Vec<ServerIpRecordEntity>>(1)?;
    let node_rows = resp.take::<Vec<NodeEntity>>(2)?;
    let port_rows = resp.take::<Vec<PortEntity>>(3)?;
    let edges = resp.take::<Vec<EdgeConnectionEntity>>(4)?;

    let mut ports_by_node: HashMap<String, Vec<PortEntity>> = HashMap::new();
    for port in port_rows {
        ports_by_node
            .entry(crate::utils::ids::record_key(&port.owner.0))
            .or_default()
            .push(port);
    }
    let nodes = node_rows
        .into_iter()
        .map(|node| {
            let ports = ports_by_node
                .remove(crate::utils::ids::record_key(&node.id.0).as_str())
                .unwrap_or_default();
            NodeWithPorts { node, ports }
        })
        .collect();
    Ok((servers, ips, nodes, edges))
}
