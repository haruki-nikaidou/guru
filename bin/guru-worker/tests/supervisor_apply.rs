#![allow(clippy::unwrap_used, clippy::panic)]

//! `Supervisor::apply` is all-or-nothing: a config that cannot be fully bound must
//! leave the previously running listeners serving.

use guru_worker_config::{
    Config, Forwarding, ForwardingTo, Ipv6Resolve, ListenAs, LogConfig, Remote,
};
use guru_worker::supervisor::Supervisor;
use std::net::SocketAddr;

fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    drop(l);
    port
}

fn forwarding(tag: &str, listen: SocketAddr) -> Forwarding {
    Forwarding {
        tag: tag.to_string(),
        listen,
        receive_proxy_protocol: None,
        listen_as: ListenAs::Raw,
        to: ForwardingTo::Exit {
            destination: Remote::parse("127.0.0.1:1").unwrap(),
            send_proxy_protocol: None,
        },
    }
}

fn config(forwardings: Vec<Forwarding>) -> Config {
    Config {
        ipv6_resolve: Ipv6Resolve::Tolerated,
        log: LogConfig::default(),
        forwardings,
    }
}

#[tokio::test]
async fn failed_apply_keeps_running_listeners() {
    let serving: SocketAddr = format!("127.0.0.1:{}", free_port()).parse().unwrap();
    // A foreign process owns this port; binding it must fail.
    let foreign = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let taken: SocketAddr = foreign.local_addr().unwrap();

    let mut sup = Supervisor::new();
    sup.apply(&config(vec![forwarding("serving", serving)]))
        .unwrap();
    tokio::net::TcpStream::connect(serving)
        .await
        .expect("first listener accepts");

    let err = sup
        .apply(&config(vec![
            forwarding("serving", serving),
            forwarding("collides", taken),
        ]))
        .expect_err("binding a taken port must fail the whole apply");
    assert_eq!(err.tag, "collides");

    // The running listener survived the failed apply...
    tokio::net::TcpStream::connect(serving)
        .await
        .expect("listener still accepts after failed apply");
    // ...and the failed apply released nothing of the foreign listener's port.
    assert!(std::net::TcpListener::bind(taken).is_err());

    // A subsequent good apply still works, and removing a listener stops it.
    let next = format!("127.0.0.1:{}", free_port()).parse().unwrap();
    sup.apply(&config(vec![forwarding("next", next)])).unwrap();
    tokio::net::TcpStream::connect(next)
        .await
        .expect("new listener accepts");
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        tokio::net::TcpStream::connect(serving).await.is_err(),
        "removed listener must stop accepting"
    );

    sup.shutdown_all();
    drop(foreign);
}
