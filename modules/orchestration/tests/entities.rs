//! Entity-layer queries against an in-memory SurrealDB with the real schema.

#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

mod common;

use common::*;
use kanau::processor::Processor;
use orchestration::entities::surreal::canvas::{
    DeleteCanvasRow, FindCanvasById, ListCanvases, UpdateCanvasMeta,
};
use orchestration::entities::surreal::connection::{
    ConnectPorts, FindEdgeById, FindLiveEdgeByPort, ForceDeleteEdgeRow, ListEdgesByCanvas,
    ListLiveEdgesByCanvas, RetireEdgeRow,
};
use orchestration::entities::surreal::node::{
    CarryEdge, ExitConfig, FindNodeById, FindNodeWithPorts, ForceDeleteNodeRow, NodeSpec,
    ReplaceNodeRow, RetireNodeRow, UpdateNodeMetaRow,
};
use orchestration::entities::surreal::revision::{
    CollectRcuGarbage, FindServerConfigRevision, ListRetainedRevisions, NextRevision,
    PruneServerRevisionsBelow, RecordServerConfigRevision,
};
use orchestration::entities::surreal::server::{
    DeleteServerIpRow, DeleteServerRow, FindServerById, FindServerByRefreshKeyDigest,
    FindServerIpById, ListServerIpsByCanvas, ListServerWatchState, ListServersByCanvas,
    MarkServerApplied, MoveServerPosition, RotateServerRefreshKey, SetServerApplyError,
    SetServerDesiredRevision, ServerIpv6Resolve, UpdateServerSettings,
};
use orchestration::entities::surreal::topology::{
    FindCanvasOfServer, LoadCanvasContents, LoadCanvasTopology,
};

fn exit_spec(dest: &str) -> NodeSpec {
    NodeSpec::Exit(ExitConfig {
        destination: dest.to_string(),
        pass_proxy_protocol: None,
    })
}

#[tokio::test]
async fn canvas_crud_round_trip() -> TestResult {
    let sp = setup().await?;
    let c = canvas(&sp, "prod").await?;
    assert_eq!(sp.process(ListCanvases {}).await?.len(), 1);

    let updated = sp
        .process(UpdateCanvasMeta {
            id: c.id.clone(),
            name: "prod2".to_string(),
            description: "desc".to_string(),
        })
        .await?;
    assert_eq!(updated.name, "prod2");
    assert_eq!(
        sp.process(FindCanvasById { id: c.id.clone() })
            .await?
            .unwrap()
            .description,
        "desc"
    );

    let s = server(&sp, &c, "tokyo").await?;
    server_ip(&sp, &s, "203.0.113.10").await?;
    sp.process(DeleteCanvasRow { id: c.id.clone() }).await?;
    assert!(sp.process(FindCanvasById { id: c.id }).await?.is_none());
    assert!(sp.process(FindServerById { id: s.id }).await?.is_none());
    Ok(())
}

#[tokio::test]
async fn server_lifecycle_and_rollout_columns() -> TestResult {
    let sp = setup().await?;
    let c = canvas(&sp, "prod").await?;
    let s = server(&sp, &c, "tokyo").await?;
    assert_eq!(s.desired_revision, 0);
    assert_eq!(s.applied_revision, 0);
    assert_eq!(s.refresh_key_generation, 0);
    assert!(s.last_seen_at.is_none());

    let updated = sp
        .process(UpdateServerSettings {
            id: s.id.clone(),
            name: "tokyo-1".to_string(),
            icon: "jp".to_string(),
            comment: "primary".to_string(),
            ipv6_resolve: ServerIpv6Resolve::Preferred,
            log_level: "debug".to_string(),
        })
        .await?;
    assert_eq!(updated.ipv6_resolve, ServerIpv6Resolve::Preferred);
    assert_eq!(updated.log_level, "debug");

    sp.process(MoveServerPosition {
        id: s.id.clone(),
        position: pos(7, 9),
    })
    .await?;
    assert_eq!(
        sp.process(FindServerById { id: s.id.clone() })
            .await?
            .unwrap()
            .position,
        pos(7, 9)
    );

    // Rollout bookkeeping.
    sp.process(SetServerDesiredRevision {
        server: s.id.clone(),
        revision: 4,
    })
    .await?;
    sp.process(SetServerApplyError {
        server: s.id.clone(),
        error: "boom".to_string(),
    })
    .await?;
    let row = sp
        .process(FindServerById { id: s.id.clone() })
        .await?
        .unwrap();
    assert_eq!(row.desired_revision, 4);
    assert_eq!(row.last_apply_error.as_deref(), Some("boom"));
    sp.process(MarkServerApplied {
        server: s.id.clone(),
        revision: 4,
    })
    .await?;
    let row = sp
        .process(FindServerById { id: s.id.clone() })
        .await?
        .unwrap();
    assert_eq!(row.applied_revision, 4);
    assert!(row.last_apply_error.is_none());

    // Refresh keys rotate, and lookup is by digest.
    let generation = sp
        .process(RotateServerRefreshKey {
            server: s.id.clone(),
            digest: "digest-1".to_string(),
            now: chrono::Utc::now(),
        })
        .await?;
    assert_eq!(generation, 1);
    let found = sp
        .process(FindServerByRefreshKeyDigest {
            digest: "digest-1".to_string(),
        })
        .await?
        .unwrap();
    assert_eq!(found.id.0, s.id.0);
    assert!(found.last_seen_at.is_some());
    let generation = sp
        .process(RotateServerRefreshKey {
            server: s.id.clone(),
            digest: "digest-2".to_string(),
            now: chrono::Utc::now(),
        })
        .await?;
    assert_eq!(generation, 2);
    assert!(
        sp.process(FindServerByRefreshKeyDigest {
            digest: "digest-1".to_string()
        })
        .await?
        .is_none(),
        "the superseded digest must no longer resolve"
    );

    let watch = sp
        .process(ListServerWatchState {
            servers: vec![s.id.clone()],
        })
        .await?;
    assert_eq!(watch.len(), 1);
    assert_eq!(watch[0].desired_revision, 4);
    assert_eq!(watch[0].refresh_key_generation, 2);

    assert_eq!(
        sp.process(ListServersByCanvas {
            canvas: c.id.clone()
        })
        .await?
        .len(),
        1
    );
    assert_eq!(
        sp.process(FindCanvasOfServer {
            server: s.id.clone()
        })
        .await?
        .unwrap()
        .0,
        c.id.0
    );
    Ok(())
}

#[tokio::test]
async fn server_ip_records_are_scoped_to_their_canvas() -> TestResult {
    let sp = setup().await?;
    let c = canvas(&sp, "prod").await?;
    let other = canvas(&sp, "staging").await?;
    let s = server(&sp, &c, "tokyo").await?;
    let s2 = server(&sp, &other, "osaka").await?;
    let ip = server_ip(&sp, &s, "203.0.113.10").await?;
    server_ip(&sp, &s2, "198.51.100.10").await?;

    let ips = sp
        .process(ListServerIpsByCanvas {
            canvas: c.id.clone(),
        })
        .await?;
    assert_eq!(ips.len(), 1);
    assert_eq!(ips[0].ip, "203.0.113.10");
    assert!(
        sp.process(FindServerIpById { id: ip.id.clone() })
            .await?
            .is_some()
    );

    sp.process(DeleteServerIpRow { id: ip.id.clone() }).await?;
    assert!(sp.process(FindServerIpById { id: ip.id }).await?.is_none());

    sp.process(DeleteServerRow { id: s2.id.clone() }).await?;
    assert!(sp.process(FindServerById { id: s2.id }).await?.is_none());
    assert!(
        sp.process(ListServerIpsByCanvas { canvas: other.id })
            .await?
            .is_empty(),
        "deleting a server deletes its ip records"
    );
    Ok(())
}

#[tokio::test]
async fn next_revision_is_strictly_increasing() -> TestResult {
    let sp = setup().await?;
    let a = sp.process(NextRevision {}).await?;
    let b = sp.process(NextRevision {}).await?;
    let c = sp.process(NextRevision {}).await?;
    assert_eq!(a, 1, "revision 0 is the reserved never-stamped sentinel");
    assert!(a < b && b < c, "{a} < {b} < {c}");
    Ok(())
}

#[tokio::test]
async fn create_node_writes_node_and_ports_together() -> TestResult {
    let sp = setup().await?;
    let c = canvas(&sp, "prod").await?;
    let s = server(&sp, &c, "tokyo").await?;
    let ip = server_ip(&sp, &s, "203.0.113.10").await?;

    let pod = node(&sp, &c, "pod", pod_spec(&ip, 443), pod_ports(), 1).await?;
    assert_eq!(pod.ports.len(), 2);
    assert_eq!(pod.node.created_rev, 1);
    assert!(pod.node.retired_rev.is_none());

    let loaded = sp
        .process(FindNodeWithPorts {
            id: pod.node.id.clone(),
        })
        .await?
        .unwrap();
    assert_eq!(loaded.ports.len(), 2);
    match loaded.node.spec {
        NodeSpec::Pod(cfg) => {
            assert_eq!(cfg.port, 443);
            assert_eq!(cfg.ip.0, ip.id.0);
        }
        other => panic!("expected pod spec, got {other:?}"),
    }

    let meta = sp
        .process(UpdateNodeMetaRow {
            id: pod.node.id.clone(),
            name: "edge".to_string(),
            comment: "renamed".to_string(),
            position: pos(3, 4),
        })
        .await?;
    assert_eq!(meta.name, "edge");
    assert_eq!(meta.position, pos(3, 4));
    Ok(())
}

#[tokio::test]
async fn retire_node_retires_its_live_edges() -> TestResult {
    let sp = setup().await?;
    let c = canvas(&sp, "prod").await?;
    let s = server(&sp, &c, "tokyo").await?;
    let ip = server_ip(&sp, &s, "203.0.113.10").await?;
    let pod = node(&sp, &c, "pod", pod_spec(&ip, 443), pod_ports(), 1).await?;
    let exit = node(
        &sp,
        &c,
        "exit",
        exit_spec("10.0.0.5:8080"),
        exit_ports(),
        1,
    )
    .await?;
    let edge = sp
        .process(ConnectPorts {
            source: port_of(&exit, "destination"),
            target: port_of(&pod, "destination"),
            revision: 1,
        })
        .await?;
    assert!(edge.retired_rev.is_none());
    assert_eq!(
        sp.process(ListLiveEdgesByCanvas {
            canvas: c.id.clone()
        })
        .await?
        .len(),
        1
    );
    assert!(
        sp.process(FindLiveEdgeByPort {
            port: port_of(&pod, "destination")
        })
        .await?
        .is_some()
    );

    sp.process(RetireNodeRow {
        id: pod.node.id.clone(),
        revision: 7,
    })
    .await?;
    assert_eq!(
        sp.process(FindNodeById {
            id: pod.node.id.clone()
        })
        .await?
        .unwrap()
        .retired_rev,
        Some(7)
    );
    assert_eq!(
        sp.process(FindEdgeById { id: edge.id.clone() })
            .await?
            .unwrap()
            .retired_rev,
        Some(7),
        "retiring a node retires every live edge touching its ports"
    );
    assert!(
        sp.process(ListLiveEdgesByCanvas {
            canvas: c.id.clone()
        })
        .await?
        .is_empty()
    );
    assert_eq!(
        sp.process(ListEdgesByCanvas {
            canvas: c.id.clone()
        })
        .await?
        .len(),
        1,
        "the retiring edge is still visible to the dashboard"
    );
    Ok(())
}

#[tokio::test]
async fn force_delete_node_leaves_no_ports_or_edges() -> TestResult {
    let sp = setup().await?;
    let c = canvas(&sp, "prod").await?;
    let s = server(&sp, &c, "tokyo").await?;
    let ip = server_ip(&sp, &s, "203.0.113.10").await?;
    let pod = node(&sp, &c, "pod", pod_spec(&ip, 443), pod_ports(), 1).await?;
    let exit = node(
        &sp,
        &c,
        "exit",
        exit_spec("10.0.0.5:8080"),
        exit_ports(),
        1,
    )
    .await?;
    let edge = sp
        .process(ConnectPorts {
            source: port_of(&exit, "destination"),
            target: port_of(&pod, "destination"),
            revision: 1,
        })
        .await?;

    sp.process(ForceDeleteNodeRow {
        id: pod.node.id.clone(),
    })
    .await?;
    assert!(sp.process(FindNodeById { id: pod.node.id }).await?.is_none());
    assert!(sp.process(FindEdgeById { id: edge.id }).await?.is_none());
    let topology = sp
        .process(LoadCanvasTopology {
            canvas: c.id.clone(),
        })
        .await?;
    assert_eq!(topology.nodes.len(), 1);
    assert!(topology.edges.is_empty());
    assert_eq!(
        topology.nodes[0].ports.len(),
        1,
        "the deleted node's ports are gone"
    );
    Ok(())
}

#[tokio::test]
async fn replace_node_carries_edges_to_the_replacement() -> TestResult {
    let sp = setup().await?;
    let c = canvas(&sp, "prod").await?;
    let s = server(&sp, &c, "tokyo").await?;
    let ip = server_ip(&sp, &s, "203.0.113.10").await?;
    let pod = node(&sp, &c, "pod", pod_spec(&ip, 443), pod_ports(), 1).await?;
    let exit = node(
        &sp,
        &c,
        "exit",
        exit_spec("10.0.0.5:8080"),
        exit_ports(),
        1,
    )
    .await?;
    let edge = sp
        .process(ConnectPorts {
            source: port_of(&exit, "destination"),
            target: port_of(&pod, "destination"),
            revision: 1,
        })
        .await?;

    let replacement = sp
        .process(ReplaceNodeRow {
            old: pod.node.id.clone(),
            canvas: c.id.clone(),
            name: "pod".to_string(),
            comment: String::new(),
            spec: pod_spec(&ip, 8443),
            position: pos(0, 0),
            revision: 5,
            ports: pod_ports(),
            carry: vec![CarryEdge {
                old_edge: edge.id.clone(),
                new_port_key: "destination".to_string(),
                other_port: port_of(&exit, "destination"),
                new_port_is_source: false,
            }],
        })
        .await?;

    assert_eq!(
        replacement.node.replaces.as_ref().map(|r| r.0.clone()),
        Some(pod.node.id.0.clone())
    );
    assert_eq!(
        sp.process(FindNodeById {
            id: pod.node.id.clone()
        })
        .await?
        .unwrap()
        .retired_rev,
        Some(5)
    );
    assert_eq!(
        sp.process(FindEdgeById { id: edge.id }).await?.unwrap().retired_rev,
        Some(5)
    );

    let live = sp
        .process(ListLiveEdgesByCanvas {
            canvas: c.id.clone(),
        })
        .await?;
    assert_eq!(live.len(), 1, "exactly one carried edge is live");
    assert_eq!(live[0].created_rev, 5);
    assert_eq!(live[0].source.0, port_of(&exit, "destination").0);
    assert_eq!(live[0].target.0, port_of(&replacement, "destination").0);

    let topology = sp.process(LoadCanvasTopology { canvas: c.id }).await?;
    assert_eq!(topology.nodes.len(), 2, "only live nodes are in a topology");
    Ok(())
}

#[tokio::test]
async fn edges_can_be_retired_and_force_deleted() -> TestResult {
    let sp = setup().await?;
    let c = canvas(&sp, "prod").await?;
    let s = server(&sp, &c, "tokyo").await?;
    let ip = server_ip(&sp, &s, "203.0.113.10").await?;
    let pod = node(&sp, &c, "pod", pod_spec(&ip, 443), pod_ports(), 1).await?;
    let exit = node(
        &sp,
        &c,
        "exit",
        exit_spec("10.0.0.5:8080"),
        exit_ports(),
        1,
    )
    .await?;
    let edge = sp
        .process(ConnectPorts {
            source: port_of(&exit, "destination"),
            target: port_of(&pod, "destination"),
            revision: 1,
        })
        .await?;

    sp.process(RetireEdgeRow {
        id: edge.id.clone(),
        revision: 3,
    })
    .await?;
    assert_eq!(
        sp.process(FindEdgeById { id: edge.id.clone() })
            .await?
            .unwrap()
            .retired_rev,
        Some(3)
    );
    assert!(
        sp.process(FindLiveEdgeByPort {
            port: port_of(&pod, "destination")
        })
        .await?
        .is_none()
    );

    sp.process(ForceDeleteEdgeRow { id: edge.id.clone() })
        .await?;
    assert!(sp.process(FindEdgeById { id: edge.id }).await?.is_none());
    Ok(())
}

#[tokio::test]
async fn revision_rows_prune_below_the_applied_revision() -> TestResult {
    let sp = setup().await?;
    let c = canvas(&sp, "prod").await?;
    let s = server(&sp, &c, "tokyo").await?;

    for revision in [1, 2, 3] {
        sp.process(RecordServerConfigRevision {
            server: s.id.clone(),
            revision,
            nodes: vec![],
            edges: vec![],
            toml: format!("# revision {revision}"),
        })
        .await?;
    }
    assert_eq!(
        sp.process(ListRetainedRevisions {
            server: s.id.clone()
        })
        .await?
        .len(),
        3
    );

    sp.process(PruneServerRevisionsBelow {
        server: s.id.clone(),
        revision: 2,
    })
    .await?;
    let retained = sp
        .process(ListRetainedRevisions {
            server: s.id.clone(),
        })
        .await?;
    assert_eq!(
        retained.iter().map(|r| r.revision).collect::<Vec<_>>(),
        vec![2, 3],
        "the applied revision itself is kept"
    );
    assert_eq!(
        sp.process(FindServerConfigRevision {
            server: s.id.clone(),
            revision: 2
        })
        .await?
        .unwrap()
        .toml,
        "# revision 2"
    );
    assert!(
        sp.process(FindServerConfigRevision {
            server: s.id,
            revision: 1
        })
        .await?
        .is_none()
    );
    Ok(())
}

#[tokio::test]
async fn gc_keeps_retired_rows_a_retained_revision_still_references() -> TestResult {
    let sp = setup().await?;
    let c = canvas(&sp, "prod").await?;
    let s = server(&sp, &c, "tokyo").await?;
    let ip = server_ip(&sp, &s, "203.0.113.10").await?;
    let pod = node(&sp, &c, "pod", pod_spec(&ip, 443), pod_ports(), 1).await?;
    let exit = node(
        &sp,
        &c,
        "exit",
        exit_spec("10.0.0.5:8080"),
        exit_ports(),
        1,
    )
    .await?;
    let edge = sp
        .process(ConnectPorts {
            source: port_of(&exit, "destination"),
            target: port_of(&pod, "destination"),
            revision: 1,
        })
        .await?;
    sp.process(RecordServerConfigRevision {
        server: s.id.clone(),
        revision: 1,
        nodes: vec![pod.node.id.clone(), exit.node.id.clone()],
        edges: vec![edge.id.clone()],
        toml: "# running".to_string(),
    })
    .await?;

    sp.process(RetireNodeRow {
        id: pod.node.id.clone(),
        revision: 2,
    })
    .await?;

    let report = sp.process(CollectRcuGarbage {}).await?;
    assert_eq!(
        (report.nodes, report.edges),
        (0, 0),
        "a revision the server still runs pins the retired rows"
    );
    assert!(
        sp.process(FindNodeById {
            id: pod.node.id.clone()
        })
        .await?
        .is_some()
    );

    // The server moves on: its old revision row is pruned, so nothing pins the rows.
    sp.process(PruneServerRevisionsBelow {
        server: s.id.clone(),
        revision: 2,
    })
    .await?;
    let report = sp.process(CollectRcuGarbage {}).await?;
    assert_eq!((report.nodes, report.edges), (1, 1));
    assert!(sp.process(FindNodeById { id: pod.node.id }).await?.is_none());
    assert!(sp.process(FindEdgeById { id: edge.id }).await?.is_none());
    assert!(
        sp.process(FindNodeById {
            id: exit.node.id.clone()
        })
        .await?
        .is_some(),
        "live nodes are never collected"
    );
    Ok(())
}

#[tokio::test]
async fn canvas_contents_include_retiring_rows() -> TestResult {
    let sp = setup().await?;
    let c = canvas(&sp, "prod").await?;
    let s = server(&sp, &c, "tokyo").await?;
    let ip = server_ip(&sp, &s, "203.0.113.10").await?;
    let pod = node(&sp, &c, "pod", pod_spec(&ip, 443), pod_ports(), 1).await?;
    sp.process(RetireNodeRow {
        id: pod.node.id,
        revision: 2,
    })
    .await?;

    let contents = sp
        .process(LoadCanvasContents {
            canvas: c.id.clone(),
        })
        .await?
        .unwrap();
    assert_eq!(contents.canvas.name, "prod");
    assert_eq!(contents.servers.len(), 1);
    assert_eq!(contents.servers[0].ips.len(), 1);
    assert_eq!(contents.nodes.len(), 1);
    assert_eq!(contents.nodes[0].node.retired_rev, Some(2));

    let topology = sp.process(LoadCanvasTopology { canvas: c.id }).await?;
    assert!(
        topology.nodes.is_empty(),
        "a topology only ever sees live rows"
    );
    assert_eq!(topology.ips.len(), 1);
    assert_eq!(topology.servers.len(), 1);
    Ok(())
}
