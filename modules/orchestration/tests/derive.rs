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
use orchestration::services::derive::{DeriveError, derive_server_config};
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

/// Compares against `tests/golden/<name>.toml` and re-parses the emitted text.
fn assert_golden(name: &str, toml: &str) {
    let path = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden"))
        .join(format!("{name}.toml"));
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, toml).unwrap();
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    assert_eq!(toml, expected, "derived config for {name} changed");
    guru_worker_config::Config::from_toml_str(toml)
        .unwrap_or_else(|e| panic!("emitted config for {name} does not parse: {e}"));
}

fn derived(builder: &Builder, server: &ServerId) -> String {
    derive_server_config(&builder.build(), server)
        .unwrap_or_else(|e| panic!("derive failed: {e}"))
        .toml
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
    assert_golden("single_pod", &result.toml);
    assert_eq!(
        result.nodes.len(),
        3,
        "pod, entry and exit are retained for this revision"
    );
    assert_eq!(result.edges.len(), 2);
}

#[test]
fn load_balance_members_follow_port_position() {
    let mut b = Builder::new("prod");
    let s = b.server("tokyo");
    let ip = b.ip("ip1", &s, "203.0.113.10");
    b.named_node("pod", "edge", pod(&ip, 443), pod_ports());
    b.node("entry", entry(Some(ProxyProtocolVersion::V2)), entry_ports());
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
            guru_worker_config::ForwardingTo::Exit { destination, .. } => format!("{destination:?}"),
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
    b.named_node("relay_osaka", "to-osaka", relay(RelayProtocol::TcpRaw), relay_ports());
    // Osaka's pod terminates the relay and hands off to the next relay.
    b.named_node("pod_osaka", "osaka-hop", pod(&ip_osaka, 9443), pod_ports());
    b.named_node("relay_sg", "to-singapore", relay(RelayProtocol::TcpRaw), relay_ports());
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

    let err = derive_server_config(&b.build(), &s).expect_err("tls is a later stage");
    assert!(
        matches!(err, DeriveError::TlsNotYetSupported { .. }),
        "{err:?}"
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

    let err = derive_server_config(&b.build(), &s).expect_err("quic relays need the internal CA");
    assert!(
        matches!(err, DeriveError::RelayProtocolNotYetSupported { .. }),
        "{err:?}"
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
    assert!(result.nodes.is_empty());
}
