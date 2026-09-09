//! Topology rules: one scenario per reported problem kind.

#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

#[path = "common/mem.rs"]
mod mem;

use mem::*;
use orchestration::entities::surreal::node::{
    CanvasImportConfig, EntryConfig, ExitConfig, LoadBalanceAggregateConfig,
    LoadBalanceDistributeConfig, LoadBalanceMode, NodeSpec, PodConfig, ProxyProtocolVersion,
    RelayConfig, RelayProtocol,
};
use orchestration::entities::surreal::port::{PortDirection, PortKind};
use orchestration::entities::surreal::server::ServerIpRecordId;
use orchestration::services::topology::{
    ProblemKind, ProblemSeverity, TopologyProblem, analyze, ensure_valid,
};
use orchestration::utils::ids;

fn kinds(problems: &[TopologyProblem], severity: ProblemSeverity) -> Vec<ProblemKind> {
    problems
        .iter()
        .filter(|p| p.severity == severity)
        .map(|p| p.kind)
        .collect()
}

fn errors(problems: &[TopologyProblem]) -> Vec<ProblemKind> {
    kinds(problems, ProblemSeverity::Error)
}

fn warnings(problems: &[TopologyProblem]) -> Vec<ProblemKind> {
    kinds(problems, ProblemSeverity::Warning)
}

fn exit(dest: &str) -> NodeSpec {
    NodeSpec::Exit(ExitConfig {
        destination: dest.to_string(),
        pass_proxy_protocol: None,
    })
}

fn entry(pp: Option<ProxyProtocolVersion>) -> NodeSpec {
    NodeSpec::Entry(EntryConfig {
        receive_proxy_protocol: pp,
        tls: None,
    })
}

fn pod(ip: &ServerIpRecordId, port: u16) -> NodeSpec {
    NodeSpec::Pod(PodConfig {
        ip: ip.clone(),
        port,
    })
}

fn relay(protocol: RelayProtocol) -> NodeSpec {
    NodeSpec::Relay(RelayConfig {
        protocol,
        override_ip_address: None,
        override_port: None,
    })
}

/// pod -> entry, exit -> pod: the smallest valid canvas.
fn valid_builder() -> Builder {
    let mut b = Builder::new("prod");
    let s = b.server("tokyo");
    let ip = b.ip("ip1", &s, "203.0.113.10");
    b.node("pod", pod(&ip, 443), pod_ports());
    b.node("entry", entry(None), entry_ports());
    b.node("exit", exit("10.0.0.5:8080"), exit_ports());
    b.connect("pod-listen", "entry-listen");
    b.connect("exit-destination", "pod-destination");
    b
}

#[test]
fn a_valid_canvas_has_no_problems() {
    let topology = valid_builder().build();
    let problems = analyze(&topology);
    assert!(problems.is_empty(), "{problems:?}");
    assert!(ensure_valid(&topology).is_ok());
}

#[test]
fn edge_direction_must_be_output_to_input() {
    let mut b = Builder::new("prod");
    let s = b.server("tokyo");
    let ip = b.ip("ip1", &s, "203.0.113.10");
    b.node("pod", pod(&ip, 443), pod_ports());
    b.node("entry", entry(None), entry_ports());
    b.node("exit", exit("10.0.0.5:8080"), exit_ports());
    // Both endpoints are inputs.
    b.connect_raw(port("entry", "listen"), port("pod", "destination"));
    let problems = analyze(&b.build());
    assert!(errors(&problems).contains(&ProblemKind::EdgeDirectionInvalid));
}

#[test]
fn edges_must_join_ports_of_the_same_kind() {
    let mut b = Builder::new("prod");
    let s = b.server("tokyo");
    let ip = b.ip("ip1", &s, "203.0.113.10");
    b.node("pod", pod(&ip, 443), pod_ports());
    b.node("pod2", pod(&ip, 8443), pod_ports());
    // listen output -> destination input
    b.connect("pod-listen", "pod2-destination");
    let problems = analyze(&b.build());
    assert!(errors(&problems).contains(&ProblemKind::PortKindMismatch));
}

#[test]
fn a_node_cannot_connect_to_itself() {
    let mut b = Builder::new("prod");
    b.node(
        "lb",
        NodeSpec::LoadBalanceDistribute(LoadBalanceDistributeConfig {
            mode: LoadBalanceMode::RoundRobin,
        }),
        distribute_ports(2),
    );
    b.connect("lb-destination", "lb-member_0");
    let problems = analyze(&b.build());
    assert!(errors(&problems).contains(&ProblemKind::EdgeSelfNode));
}

#[test]
fn edges_may_not_cross_canvases() {
    let b = valid_builder();
    let mut topology = b.build();
    topology.nodes[2].node.canvas = ids::canvas_id("other");
    let problems = analyze(&topology);
    assert!(errors(&problems).contains(&ProblemKind::EdgeCrossCanvas));
}

#[test]
fn a_port_carries_at_most_one_edge() {
    let mut b = valid_builder();
    b.node("exit2", exit("10.0.0.6:8080"), exit_ports());
    b.connect("exit2-destination", "pod-destination");
    let problems = analyze(&b.build());
    assert!(errors(&problems).contains(&ProblemKind::PortOversubscribed));
}

#[test]
fn a_node_must_have_the_ports_its_spec_requires() {
    let mut b = Builder::new("prod");
    let s = b.server("tokyo");
    let ip = b.ip("ip1", &s, "203.0.113.10");
    // A pod without its destination input.
    b.node(
        "pod",
        pod(&ip, 443),
        vec![(
            "listen".to_string(),
            PortKind::DeriveListen,
            PortDirection::Output,
            0,
        )],
    );
    let problems = analyze(&b.build());
    assert!(errors(&problems).contains(&ProblemKind::PortShapeInvalid));
}

#[test]
fn canvas_import_and_export_are_rejected() {
    let mut b = Builder::new("prod");
    b.node(
        "import",
        NodeSpec::CanvasImport(CanvasImportConfig {}),
        vec![],
    );
    let problems = analyze(&b.build());
    assert_eq!(errors(&problems), vec![ProblemKind::UnsupportedSpec]);
}

#[test]
fn a_pod_must_reference_an_ip_of_this_canvas() {
    let mut b = Builder::new("prod");
    let s = b.server("tokyo");
    b.ip("ip1", &s, "203.0.113.10");
    b.node("pod", pod(&ids::server_ip_id("foreign"), 443), pod_ports());
    b.node("entry", entry(None), entry_ports());
    b.node("exit", exit("10.0.0.5:8080"), exit_ports());
    b.connect("pod-listen", "entry-listen");
    b.connect("exit-destination", "pod-destination");
    let problems = analyze(&b.build());
    assert_eq!(errors(&problems), vec![ProblemKind::PodIpForeign]);
}

#[test]
fn an_exit_destination_must_be_host_port() {
    let mut b = Builder::new("prod");
    let s = b.server("tokyo");
    let ip = b.ip("ip1", &s, "203.0.113.10");
    b.node("pod", pod(&ip, 443), pod_ports());
    b.node("entry", entry(None), entry_ports());
    b.node("exit", exit("no-port-here"), exit_ports());
    b.connect("pod-listen", "entry-listen");
    b.connect("exit-destination", "pod-destination");
    let problems = analyze(&b.build());
    assert_eq!(errors(&problems), vec![ProblemKind::ExitDestinationInvalid]);
}

#[test]
fn two_pods_may_not_share_a_listen_address() {
    let mut b = Builder::new("prod");
    let s = b.server("tokyo");
    let ip = b.ip("ip1", &s, "203.0.113.10");
    for tag in ["pod_a", "pod_b"] {
        b.node(tag, pod(&ip, 443), pod_ports());
        b.named_node(
            &format!("{tag}_entry"),
            &format!("{tag}_entry"),
            entry(None),
            entry_ports(),
        );
        b.named_node(
            &format!("{tag}_exit"),
            &format!("{tag}_exit"),
            exit("10.0.0.5:8080"),
            exit_ports(),
        );
        b.connect(&format!("{tag}-listen"), &format!("{tag}_entry-listen"));
        b.connect(
            &format!("{tag}_exit-destination"),
            &format!("{tag}-destination"),
        );
    }
    let problems = analyze(&b.build());
    assert_eq!(errors(&problems), vec![ProblemKind::DuplicateListen]);
}

#[test]
fn a_relay_loop_is_a_cycle() {
    let mut b = Builder::new("prod");
    let s = b.server("tokyo");
    let ip = b.ip("ip1", &s, "203.0.113.10");
    b.node("pod", pod(&ip, 443), pod_ports());
    b.node("relay", relay(RelayProtocol::TcpRaw), relay_ports());
    // The relay dials the very pod it feeds.
    b.connect("pod-listen", "relay-listen");
    b.connect("relay-destination", "pod-destination");
    let problems = analyze(&b.build());
    assert!(
        errors(&problems).contains(&ProblemKind::Cycle),
        "{problems:?}"
    );
}

#[test]
fn ip_hash_needs_the_client_address() {
    let mut b = Builder::new("prod");
    let s = b.server("tokyo");
    let ip = b.ip("ip1", &s, "203.0.113.10");
    b.node("pod", pod(&ip, 443), pod_ports());
    b.node("entry", entry(None), entry_ports());
    b.node(
        "lb",
        NodeSpec::LoadBalanceDistribute(LoadBalanceDistributeConfig {
            mode: LoadBalanceMode::IpHash,
        }),
        distribute_ports(2),
    );
    b.node("exit_a", exit("10.0.0.5:8080"), exit_ports());
    b.node("exit_b", exit("10.0.0.6:8080"), exit_ports());
    b.connect("pod-listen", "entry-listen");
    b.connect("lb-destination", "pod-destination");
    b.connect("exit_a-destination", "lb-member_0");
    b.connect("exit_b-destination", "lb-member_1");
    let problems = analyze(&b.build());
    assert_eq!(errors(&problems), vec![ProblemKind::IpHashWithoutClientIp]);

    // With PROXY protocol on the entry the client address is known again.
    let mut ok = Builder::new("prod");
    let s = ok.server("tokyo");
    let ip = ok.ip("ip1", &s, "203.0.113.10");
    ok.node("pod", pod(&ip, 443), pod_ports());
    ok.node(
        "entry",
        entry(Some(ProxyProtocolVersion::V2)),
        entry_ports(),
    );
    ok.node(
        "lb",
        NodeSpec::LoadBalanceDistribute(LoadBalanceDistributeConfig {
            mode: LoadBalanceMode::IpHash,
        }),
        distribute_ports(2),
    );
    ok.node("exit_a", exit("10.0.0.5:8080"), exit_ports());
    ok.node("exit_b", exit("10.0.0.6:8080"), exit_ports());
    ok.connect("pod-listen", "entry-listen");
    ok.connect("lb-destination", "pod-destination");
    ok.connect("exit_a-destination", "lb-member_0");
    ok.connect("exit_b-destination", "lb-member_1");
    assert!(errors(&analyze(&ok.build())).is_empty());
}

#[test]
fn ip_hash_behind_an_aggregate_is_still_reachable() {
    let mut b = Builder::new("prod");
    let s = b.server("tokyo");
    let ip = b.ip("ip1", &s, "203.0.113.10");
    b.node("pod", pod(&ip, 443), pod_ports());
    b.node("entry", entry(None), entry_ports());
    b.node(
        "agg",
        NodeSpec::LoadBalanceAggregate(LoadBalanceAggregateConfig {}),
        aggregate_ports(2),
    );
    b.node(
        "lb",
        NodeSpec::LoadBalanceDistribute(LoadBalanceDistributeConfig {
            mode: LoadBalanceMode::IpHash,
        }),
        distribute_ports(2),
    );
    b.node("exit_a", exit("10.0.0.5:8080"), exit_ports());
    b.node("exit_b", exit("10.0.0.6:8080"), exit_ports());
    b.connect("pod-listen", "entry-listen");
    b.connect("agg-copy_0", "pod-destination");
    b.connect("lb-destination", "agg-source");
    b.connect("exit_a-destination", "lb-member_0");
    b.connect("exit_b-destination", "lb-member_1");
    let problems = analyze(&b.build());
    assert!(errors(&problems).contains(&ProblemKind::IpHashWithoutClientIp));
}

#[test]
fn an_unconnected_pod_port_is_a_warning() {
    let mut b = Builder::new("prod");
    let s = b.server("tokyo");
    let ip = b.ip("ip1", &s, "203.0.113.10");
    b.node("pod", pod(&ip, 443), pod_ports());
    let topology = b.build();
    let problems = analyze(&topology);
    assert!(errors(&problems).is_empty(), "{problems:?}");
    assert_eq!(warnings(&problems), vec![ProblemKind::PodPortUnconnected]);
    assert!(
        ensure_valid(&topology).is_ok(),
        "warnings never block a write"
    );
}

#[test]
fn a_relay_hopping_within_one_server_is_a_warning() {
    let mut b = Builder::new("prod");
    let s = b.server("tokyo");
    let ip = b.ip("ip1", &s, "203.0.113.10");
    b.node("pod_in", pod(&ip, 443), pod_ports());
    b.node("pod_out", pod(&ip, 8443), pod_ports());
    b.node("entry", entry(None), entry_ports());
    b.node("relay", relay(RelayProtocol::TcpRaw), relay_ports());
    b.node("exit", exit("10.0.0.5:8080"), exit_ports());
    b.connect("pod_in-listen", "relay-listen");
    b.connect("relay-destination", "pod_out-destination");
    b.connect("pod_out-listen", "entry-listen");
    b.connect("exit-destination", "pod_in-destination");
    let problems = analyze(&b.build());
    assert!(errors(&problems).is_empty(), "{problems:?}");
    assert!(warnings(&problems).contains(&ProblemKind::RelaySameServer));
}

#[test]
fn a_single_member_load_balancer_is_a_warning() {
    let mut b = Builder::new("prod");
    let s = b.server("tokyo");
    let ip = b.ip("ip1", &s, "203.0.113.10");
    b.node("pod", pod(&ip, 443), pod_ports());
    b.node("entry", entry(None), entry_ports());
    b.node(
        "lb",
        NodeSpec::LoadBalanceDistribute(LoadBalanceDistributeConfig {
            mode: LoadBalanceMode::RoundRobin,
        }),
        distribute_ports(2),
    );
    b.node("exit_a", exit("10.0.0.5:8080"), exit_ports());
    b.connect("pod-listen", "entry-listen");
    b.connect("lb-destination", "pod-destination");
    b.connect("exit_a-destination", "lb-member_0");
    let problems = analyze(&b.build());
    assert!(errors(&problems).is_empty(), "{problems:?}");
    assert!(warnings(&problems).contains(&ProblemKind::DistributeSingleMember));
}

#[test]
fn projection_validates_a_change_before_it_is_written() {
    use orchestration::services::topology::TopologyEdit;
    let b = valid_builder();
    let topology = b.build();

    // Retiring the exit leaves the pod's destination unconnected: a warning, not an error.
    let exit_id = topology.nodes[2].node.id.clone();
    let projected = topology.project(&[TopologyEdit::RetireNode { node: exit_id }]);
    assert_eq!(projected.nodes.len(), 2);
    assert_eq!(projected.edges.len(), 1, "the exit's edge went with it");
    assert!(ensure_valid(&projected).is_ok());

    // Adding a second edge into an occupied port is rejected before writing.
    let extra = orchestration::entities::surreal::connection::EdgeConnectionEntity {
        id: ids::edge_id("pending-0"),
        source: port("exit", "destination"),
        target: port("pod", "destination"),
    };
    let projected = topology.project(&[TopologyEdit::AddEdge { edge: extra }]);
    let err = ensure_valid(&projected).expect_err("oversubscribed port must be rejected");
    assert_eq!(err.first.kind, ProblemKind::PortOversubscribed);
}
