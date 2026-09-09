#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

//! `Supervisor::apply` is all-or-nothing: a config that cannot be fully bound must
//! leave the previously running listeners serving.

use guru_worker::supervisor::Supervisor;
use guru_worker_config::{
    Config, Forwarding, ForwardingTo, Ipv6Resolve, ListenAs, LogConfig, Remote,
};
use std::net::SocketAddr;
use std::time::Duration;

fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    drop(l);
    port
}

fn local(port: u16) -> SocketAddr {
    format!("127.0.0.1:{port}").parse().unwrap()
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
    let serving = local(free_port());
    let moved = local(free_port());
    // A foreign process owns this port; binding it must fail.
    let foreign = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let taken: SocketAddr = foreign.local_addr().unwrap();

    let mut sup = Supervisor::new();
    sup.apply(&config(vec![forwarding("serving", serving)]))
        .unwrap();
    tokio::net::TcpStream::connect(serving)
        .await
        .expect("first listener accepts");

    // The failing revision does not just add a listener: it also moves "serving" to
    // another port. A supervisor that mutates as it goes therefore tears the running
    // listener down (or brings the moved one up) before it discovers that "collides"
    // cannot bind — which is exactly what the assertions below catch.
    let err = sup
        .apply(&config(vec![
            forwarding("serving", moved),
            forwarding("collides", taken),
        ]))
        .expect_err("binding a taken port must fail the whole apply");
    assert_eq!(err.tag, "collides");

    // A cancelled accept loop needs a moment to drop its socket; wait it out so the
    // checks below cannot pass merely because the teardown has not landed yet.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // (i) The listener from the previous revision is still bound and still accepting.
    tokio::net::TcpStream::connect(serving)
        .await
        .expect("the previous revision's listener must still serve after a failed apply");
    // Nothing of the failed revision was half-committed: the socket bound for the
    // moved listener while preparing was released again.
    let probe = std::net::TcpListener::bind(moved)
        .expect("a failed apply must not leave the new listener bound");
    drop(probe);
    // ...and the failed apply released nothing of the foreign listener's port.
    assert!(std::net::TcpListener::bind(taken).is_err());

    // (ii) The supervisor's own state still describes the previous revision: applying
    // that same revision again is a pure retain. Had the supervisor forgotten the
    // listener while its socket stayed alive, this apply would fail to bind.
    sup.apply(&config(vec![forwarding("serving", serving)]))
        .expect("re-applying the running revision must be a no-op retain");
    tokio::net::TcpStream::connect(serving)
        .await
        .expect("the retained listener keeps serving");

    // (iii) A later good revision still converges: the move happens, and the listener
    // the supervisor claims to own really is the one it stops.
    sup.apply(&config(vec![forwarding("serving", moved)]))
        .unwrap();
    tokio::net::TcpStream::connect(moved)
        .await
        .expect("the moved listener accepts");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while tokio::net::TcpStream::connect(serving).await.is_ok() {
        assert!(
            std::time::Instant::now() < deadline,
            "the removed listener must stop accepting"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    sup.shutdown_all();
    drop(foreign);
}
