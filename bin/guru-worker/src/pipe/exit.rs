use crate::BoxError;
use crate::config::{Ipv6Resolve, Remote, TcpProxyProtocol};
use crate::pipe::write_proxy_header;
use std::net::SocketAddr;
use tokio::net::TcpStream;

/// Connects to a real backend, optionally prefixing a PROXY protocol header.
pub async fn connect_exit(
    destination: &Remote,
    ipv6_resolve: Ipv6Resolve,
    send_pp: Option<TcpProxyProtocol>,
    client_addr: SocketAddr,
) -> Result<TcpStream, BoxError> {
    let addr = crate::resolver::resolve(destination, ipv6_resolve).await?;
    let mut s = TcpStream::connect(addr).await?;
    if let Some(v) = send_pp {
        write_proxy_header(&mut s, v, client_addr, addr).await?;
    }
    Ok(s)
}
