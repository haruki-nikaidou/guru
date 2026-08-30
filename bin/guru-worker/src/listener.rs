use crate::prepared::PreparedForwarding;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

/// Accept loop for a TCP-family listener. Reads the latest hot-swapped config per accept.
/// Cancelling the token stops accepting; already-spawned connection tasks keep running.
pub async fn run_tcp(
    listener: tokio::net::TcpListener,
    cfg_rx: watch::Receiver<Arc<PreparedForwarding>>,
    token: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = token.cancelled() => break,
            accept = listener.accept() => match accept {
                Ok((stream, peer)) => {
                    let cfg = cfg_rx.borrow().clone();
                    tokio::spawn(crate::pipe::handle_tcp_connection(stream, peer, cfg));
                }
                Err(e) => {
                    tracing::warn!(error = %e, "accept error");
                }
            }
        }
    }
    // listener dropped here -> socket closed; already-spawned connection tasks keep running.
}

/// Accept loop for a QUIC listener. Pushes new server configs on reload and drains
/// existing connections on cancellation.
pub async fn run_quic(
    endpoint: quinn::Endpoint,
    mut cfg_rx: watch::Receiver<Arc<PreparedForwarding>>,
    token: CancellationToken,
) {
    let mut current = cfg_rx.borrow().clone();
    let mut conns = JoinSet::new();
    loop {
        tokio::select! {
            _ = token.cancelled() => break,
            changed = cfg_rx.changed() => {
                if changed.is_err() {
                    break;
                }
                current = cfg_rx.borrow_and_update().clone();
                endpoint.set_server_config(current.quic_server.clone());
            }
            incoming = endpoint.accept() => match incoming {
                Some(inc) => {
                    conns.spawn(handle_quic_connection(inc, current.clone()));
                }
                None => break, // endpoint closed
            }
        }
    }
    while conns.join_next().await.is_some() {}
    endpoint.wait_idle().await;
}

async fn handle_quic_connection(incoming: quinn::Incoming, cfg: Arc<PreparedForwarding>) {
    let conn = match incoming.await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "quic handshake failed");
            return;
        }
    };
    let remote = conn.remote_address();
    while let Ok((send, recv)) = conn.accept_bi().await {
        let joined = Box::new(tokio::io::join(recv, send));
        tokio::spawn(crate::pipe::handle_relay_quic_stream_logged(
            joined,
            remote,
            cfg.clone(),
        ));
    }
}

/// Binds a TCP listener with `SO_REUSEADDR` so a restart does not trip over `TIME_WAIT`.
pub fn bind_tcp(addr: SocketAddr) -> Result<tokio::net::TcpListener, crate::BoxError> {
    let socket = if addr.is_ipv4() {
        tokio::net::TcpSocket::new_v4()?
    } else {
        tokio::net::TcpSocket::new_v6()?
    };
    socket.set_reuseaddr(true)?;
    socket.bind(addr)?;
    Ok(socket.listen(1024)?)
}

/// Binds a QUIC (UDP) endpoint serving the given config.
pub fn bind_quic(
    addr: SocketAddr,
    server_cfg: quinn::ServerConfig,
) -> Result<quinn::Endpoint, crate::BoxError> {
    Ok(quinn::Endpoint::server(server_cfg, addr)?)
}
