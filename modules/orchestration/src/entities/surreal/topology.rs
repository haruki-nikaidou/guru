//! Topology-related query processors across multiple kind of entities.

use crate::entities::surreal::connection::EdgeConnectionEntity;
use crate::entities::surreal::node::NodeEntity;
use crate::entities::surreal::server::{ServerId, ServerWithIp};

pub struct ShowEverythingDerivesServerConfig {
    pub server: ServerId,
}

pub struct ServerConfigDeriveDep {
    pub servers: Vec<ServerWithIp>,
    pub pods_on_this_server: Vec<NodeEntity>,
    pub nodes_on_path: Vec<NodeEntity>,
    pub edge_connection: Vec<EdgeConnectionEntity>,
}
