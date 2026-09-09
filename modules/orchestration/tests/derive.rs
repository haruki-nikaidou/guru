//! Config derivation: golden TOML per topology shape.
//!
//! Set `UPDATE_GOLDEN=1` to rewrite the files under `tests/golden/`.

#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

#[path = "common/mem.rs"]
mod mem;

use mem::*;
use orchestration::entities::surreal::node::{
    EntryConfig, ExitConfig, LoadBalanceAggregateConfig, LoadBalanceDistributeConfig,
    LoadBalanceMode, NodeSpec, PodConfig, ProxyProtocolVersion, RelayConfig, RelayProtocol,
    TlsConfig,
};
use orchestration::entities::surreal::server::{ServerId, ServerIpRecordId};
use orchestration::services::derive::derive_server_config;
use orchestration::utils::ids;

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

/// True only for an explicit `UPDATE_GOLDEN=1` / `UPDATE_GOLDEN=true`, so a stray
/// `UPDATE_GOLDEN=0` in the environment cannot turn the suite into a self-comparison.
fn regenerating() -> bool {
    matches!(
        std::env::var("UPDATE_GOLDEN").as_deref(),
        Ok("1") | Ok("true")
    )
}

/// Compares against `tests/golden/<name>.toml` and re-parses the emitted text.
fn assert_golden(name: &str, toml: &str) {
    let path = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden"))
        .join(format!("{name}.toml"));
    if regenerating() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, toml).unwrap();
    } else {
        let expected = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        assert_eq!(toml, expected, "derived config for {name} changed");
    }
    guru_worker_config::Config::from_toml_str(toml)
        .unwrap_or_else(|e| panic!("emitted config for {name} does not parse: {e}"));
}

/// Derives twice from independently built snapshots: `hooks::derive` only skips a
/// server whose config did not change, so the same topology must render identical
/// bytes every time.
fn derived(builder: &Builder, server: &ServerId) -> String {
    let once = render(builder, server);
    let twice = render(builder, server);
    assert_eq!(once, twice, "derivation is not byte-stable");
    once
}

fn render(builder: &Builder, server: &ServerId) -> String {
    derive_server_config(&builder.build(), server)
        .unwrap_or_else(|e| panic!("derive failed: {e}"))
        .config
        .to_toml_string()
        .unwrap_or_else(|e| panic!("rendering failed: {e}"))
}

#[test]
fn single_pod_to_entry_and_exit() {
    let mut b = Builder::new("prod");
    let s = b.server("tokyo");
    let ip = b.ip("ip1", &s, "203.0.113.10");
    b.named_node("pod", "edge", pod(&ip, 443), pod_ports());
    b.node("entry", entry(None), entry_ports());
    b.node("exit", exit("10.0.0.5:8080"), exit_ports());
    b.connect("pod-listen", "entry-listen");
    b.connect("exit-destination", "pod-destination");

    let result = derive_server_config(&b.build(), &s).unwrap();
    assert_golden("single_pod", &result.config.to_toml_string().unwrap());
    assert_eq!(
        result.forwardings.len(),
        1,
        "one pod, one forwarding, one listener capability"
    );
    assert_eq!(result.forwardings[0].serves.port, 443);
    assert!(
        result.forwardings[0].points_at.is_empty(),
        "an exit destination is not a listener this fabric serves"
    );
}

#[test]
fn load_balance_members_follow_port_position() {
    let mut b = Builder::new("prod");
    let s = b.server("tokyo");
    let ip = b.ip("ip1", &s, "203.0.113.10");
    b.named_node("pod", "edge", pod(&ip, 443), pod_ports());
    b.node(
        "entry",
        entry(Some(ProxyProtocolVersion::V2)),
        entry_ports(),
    );
    b.node(
        "lb",
        NodeSpec::LoadBalanceDistribute(LoadBalanceDistributeConfig {
            mode: LoadBalanceMode::Fallback,
        }),
        distribute_ports(3),
    );
    // Deliberately connected out of order: emission must follow the port position.
    b.node("exit_c", exit("10.0.0.7:8080"), exit_ports());
    b.node("exit_a", exit("10.0.0.5:8080"), exit_ports());
    b.node("exit_b", exit("backend.internal:9000"), exit_ports());
    b.connect("pod-listen", "entry-listen");
    b.connect("lb-destination", "pod-destination");
    b.connect("exit_c-destination", "lb-member_2");
    b.connect("exit_a-destination", "lb-member_0");
    b.connect("exit_b-destination", "lb-member_1");

    let toml = derived(&b, &s);
    assert_golden("load_balance_fallback", &toml);
    let config = guru_worker_config::Config::from_toml_str(&toml).unwrap();
    let members = match &config.forwardings[0].to {
        guru_worker_config::ForwardingTo::LoadBalance(g) => g.members.clone(),
        other => panic!("expected a load balance group, got {other:?}"),
    };
    let destinations: Vec<String> = members
        .iter()
        .map(|m| match m {
            guru_worker_config::ForwardingTo::Exit { destination, .. } => {
                format!("{destination:?}")
            }
            other => panic!("expected exits, got {other:?}"),
        })
        .collect();
    assert!(destinations[0].contains("10.0.0.5"));
    assert!(destinations[1].contains("backend.internal"));
    assert!(destinations[2].contains("10.0.0.7"));
}

#[test]
fn an_aggregate_feeds_the_same_subtree_to_two_pods() {
    let mut b = Builder::new("prod");
    let s = b.server("tokyo");
    let ip = b.ip("ip1", &s, "203.0.113.10");
    b.named_node("pod_a", "alpha", pod(&ip, 443), pod_ports());
    b.named_node("pod_b", "beta", pod(&ip, 8443), pod_ports());
    b.node("entry_a", entry(None), entry_ports());
    b.node("entry_b", entry(None), entry_ports());
    b.node(
        "agg",
        NodeSpec::LoadBalanceAggregate(LoadBalanceAggregateConfig {}),
        aggregate_ports(2),
    );
    b.node("exit", exit("10.0.0.5:8080"), exit_ports());
    b.connect("pod_a-listen", "entry_a-listen");
    b.connect("pod_b-listen", "entry_b-listen");
    b.connect("agg-copy_0", "pod_a-destination");
    b.connect("agg-copy_1", "pod_b-destination");
    b.connect("exit-destination", "agg-source");

    assert_golden("aggregate_two_pods", &derived(&b, &s));
}

/// tokyo -> osaka -> singapore, each hop a raw TCP relay.
fn relay_chain() -> (Builder, ServerId, ServerId, ServerId) {
    let mut b = Builder::new("prod");
    let tokyo = b.server("tokyo");
    let osaka = b.server("osaka");
    let singapore = b.server("singapore");
    let ip_tokyo = b.ip("ip_tokyo", &tokyo, "203.0.113.10");
    let ip_osaka = b.ip("ip_osaka", &osaka, "198.51.100.10");
    let ip_sg = b.ip("ip_sg", &singapore, "192.0.2.10");

    // Tokyo takes client traffic and relays it to Osaka.
    b.named_node("pod_tokyo", "ingress", pod(&ip_tokyo, 443), pod_ports());
    b.node("entry", entry(None), entry_ports());
    b.named_node(
        "relay_osaka",
        "to-osaka",
        relay(RelayProtocol::TcpRaw),
        relay_ports(),
    );
    // Osaka's pod terminates the relay and hands off to the next relay.
    b.named_node("pod_osaka", "osaka-hop", pod(&ip_osaka, 9443), pod_ports());
    b.named_node(
        "relay_sg",
        "to-singapore",
        relay(RelayProtocol::TcpRaw),
        relay_ports(),
    );
    // Singapore's pod exits to the origin.
    b.named_node("pod_sg", "singapore-hop", pod(&ip_sg, 9443), pod_ports());
    b.node("exit", exit("10.0.0.5:8080"), exit_ports());

    b.connect("pod_tokyo-listen", "entry-listen");
    b.connect("relay_osaka-destination", "pod_tokyo-destination");
    b.connect("pod_osaka-listen", "relay_osaka-listen");
    b.connect("relay_sg-destination", "pod_osaka-destination");
    b.connect("pod_sg-listen", "relay_sg-listen");
    b.connect("exit-destination", "pod_sg-destination");
    (b, tokyo, osaka, singapore)
}

#[test]
fn a_two_hop_relay_chain_derives_each_server() {
    let (b, tokyo, osaka, singapore) = relay_chain();
    assert_golden("relay_chain_tokyo", &derived(&b, &tokyo));
    assert_golden("relay_chain_osaka", &derived(&b, &osaka));
    assert_golden("relay_chain_singapore", &derived(&b, &singapore));
}

#[test]
fn tls_entries_are_not_supported_yet() {
    let mut b = Builder::new("prod");
    let s = b.server("tokyo");
    let ip = b.ip("ip1", &s, "203.0.113.10");
    b.node("pod", pod(&ip, 443), pod_ports());
    b.node(
        "entry",
        NodeSpec::Entry(EntryConfig {
            receive_proxy_protocol: None,
            tls: Some(TlsConfig {
                sni: "example.com".to_string(),
                dns_provider: ids::dns_provider_id("cf"),
                domain_id: "zone".to_string(),
                acme_directory: "https://acme.example/directory".to_string(),
            }),
        }),
        entry_ports(),
    );
    b.node("exit", exit("10.0.0.5:8080"), exit_ports());
    b.connect("pod-listen", "entry-listen");
    b.connect("exit-destination", "pod-destination");

    let result = derive_server_config(&b.build(), &s).expect("the server still derives");
    assert!(
        result.config.forwardings.is_empty(),
        "the only pod is invalid, so nothing is served: {:?}",
        result.config.forwardings
    );
    let [invalid] = result.invalid.as_slice() else {
        panic!("expected exactly one invalid pod, got {:?}", result.invalid);
    };
    assert_eq!(invalid.pod, "pod");
    assert_eq!(invalid.listen, "203.0.113.10:443");
    assert!(
        invalid.error.contains("TLS"),
        "the stored reason must name the cause: {}",
        invalid.error
    );
}

#[test]
fn quic_relays_are_not_supported_yet() {
    let mut b = Builder::new("prod");
    let s = b.server("tokyo");
    let ip = b.ip("ip1", &s, "203.0.113.10");
    b.node("pod", pod(&ip, 443), pod_ports());
    b.node("relay", relay(RelayProtocol::Quic), relay_ports());
    b.node("pod2", pod(&ip, 9443), pod_ports());
    b.node("entry", entry(None), entry_ports());
    b.node("exit", exit("10.0.0.5:8080"), exit_ports());
    b.connect("pod-listen", "relay-listen");
    b.connect("relay-destination", "pod2-destination");
    b.connect("pod2-listen", "entry-listen");
    b.connect("exit-destination", "pod-destination");

    // Both pods sit behind the unsupported relay: the quic hop is on pod's
    // destination side and pod2 is the relay's listening side.
    let result = derive_server_config(&b.build(), &s).expect("the server still derives");
    let mut invalid: Vec<&str> = result.invalid.iter().map(|p| p.pod.as_str()).collect();
    invalid.sort_unstable();
    assert_eq!(invalid, ["pod", "pod2"]);
    assert!(
        result.invalid.iter().all(|p| p.error.contains("quic")),
        "{:?}",
        result.invalid
    );
}

#[test]
fn a_pod_with_an_unconnected_port_is_skipped() {
    let mut b = Builder::new("prod");
    let s = b.server("tokyo");
    let ip = b.ip("ip1", &s, "203.0.113.10");
    b.node("pod", pod(&ip, 443), pod_ports());
    let result = derive_server_config(&b.build(), &s).unwrap();
    assert!(result.config.forwardings.is_empty());
    assert!(result.forwardings.is_empty());
}

/// The point of per-pod isolation: a half-drawn relay must not cost the server
/// its other pods.
#[test]
fn a_broken_pod_leaves_its_neighbour_deriving() {
    let mut b = Builder::new("prod");
    let s = b.server("tokyo");
    let ip = b.ip("ip1", &s, "203.0.113.10");

    // Healthy: entry -> good -> exit.
    b.node("good", pod(&ip, 443), pod_ports());
    b.node("in", entry(None), entry_ports());
    b.node("out", exit("10.0.0.5:8080"), exit_ports());
    b.connect("good-listen", "in-listen");
    b.connect("out-destination", "good-destination");

    // Broken: dials a relay whose own listen side nothing feeds yet.
    b.node("half", pod(&ip, 8443), pod_ports());
    b.node("in2", entry(None), entry_ports());
    b.node("hop", relay(RelayProtocol::TcpRaw), relay_ports());
    b.connect("half-listen", "in2-listen");
    b.connect("hop-destination", "half-destination");

    let result = derive_server_config(&b.build(), &s).expect("the healthy pod still derives");
    let tags: Vec<&str> = result
        .config
        .forwardings
        .iter()
        .map(|f| f.tag.as_str())
        .collect();
    assert_eq!(tags, ["good"], "the healthy pod is served on its own");
    assert_eq!(result.forwardings.len(), 1);
    let [invalid] = result.invalid.as_slice() else {
        panic!("expected exactly one invalid pod, got {:?}", result.invalid);
    };
    assert_eq!(invalid.pod, "half");
    assert_eq!(invalid.listen, "203.0.113.10:8443");
}

/// A load-balance group with no connected members used to reach the worker as an
/// empty group and fail the whole server at validation.
#[test]
fn an_empty_load_balance_group_invalidates_only_its_pod() {
    let mut b = Builder::new("prod");
    let s = b.server("tokyo");
    let ip = b.ip("ip1", &s, "203.0.113.10");

    b.node("good", pod(&ip, 443), pod_ports());
    b.node("in", entry(None), entry_ports());
    b.node("out", exit("10.0.0.5:8080"), exit_ports());
    b.connect("good-listen", "in-listen");
    b.connect("out-destination", "good-destination");

    b.node("lb-pod", pod(&ip, 8443), pod_ports());
    b.node("in2", entry(None), entry_ports());
    b.node(
        "lb",
        NodeSpec::LoadBalanceDistribute(LoadBalanceDistributeConfig {
            mode: LoadBalanceMode::RoundRobin,
        }),
        distribute_ports(2),
    );
    b.connect("lb-pod-listen", "in2-listen");
    b.connect("lb-destination", "lb-pod-destination");

    let result = derive_server_config(&b.build(), &s).expect("the healthy pod still derives");
    let tags: Vec<&str> = result
        .config
        .forwardings
        .iter()
        .map(|f| f.tag.as_str())
        .collect();
    assert_eq!(tags, ["good"]);
    let [invalid] = result.invalid.as_slice() else {
        panic!("expected exactly one invalid pod, got {:?}", result.invalid);
    };
    assert_eq!(invalid.pod, "lb-pod");
}
