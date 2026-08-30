use crate::BoxError;
use crate::config::TcpProxyProtocol;
use crate::pipe::{Prefixed, read_proxy_header};
use crate::prepared::Ingest;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_openssl::SslStream;

pub(crate) enum IngestStream {
    PrefixedTcp(Prefixed<tokio::net::TcpStream>),
    RawTcp(tokio::net::TcpStream),
    TlsPrefixed(SslStream<Prefixed<tokio::net::TcpStream>>),
    PrefixedTls(Prefixed<SslStream<tokio::net::TcpStream>>),
    RawTls(SslStream<tokio::net::TcpStream>),
}

impl AsyncRead for IngestStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        match this {
            IngestStream::PrefixedTcp(stream) => Pin::new(stream).poll_read(cx, buf),
            IngestStream::RawTcp(stream) => Pin::new(stream).poll_read(cx, buf),
            IngestStream::TlsPrefixed(stream) => Pin::new(stream).poll_read(cx, buf),
            IngestStream::PrefixedTls(stream) => Pin::new(stream).poll_read(cx, buf),
            IngestStream::RawTls(stream) => Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for IngestStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        match this {
            IngestStream::PrefixedTcp(stream) => Pin::new(stream).poll_write(cx, buf),
            IngestStream::RawTcp(stream) => Pin::new(stream).poll_write(cx, buf),
            IngestStream::TlsPrefixed(stream) => Pin::new(stream).poll_write(cx, buf),
            IngestStream::PrefixedTls(stream) => Pin::new(stream).poll_write(cx, buf),
            IngestStream::RawTls(stream) => Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        match this {
            IngestStream::PrefixedTcp(stream) => Pin::new(stream).poll_flush(cx),
            IngestStream::RawTcp(stream) => Pin::new(stream).poll_flush(cx),
            IngestStream::TlsPrefixed(stream) => Pin::new(stream).poll_flush(cx),
            IngestStream::PrefixedTls(stream) => Pin::new(stream).poll_flush(cx),
            IngestStream::RawTls(stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        match this {
            IngestStream::PrefixedTcp(stream) => Pin::new(stream).poll_shutdown(cx),
            IngestStream::RawTcp(stream) => Pin::new(stream).poll_shutdown(cx),
            IngestStream::TlsPrefixed(stream) => Pin::new(stream).poll_shutdown(cx),
            IngestStream::PrefixedTls(stream) => Pin::new(stream).poll_shutdown(cx),
            IngestStream::RawTls(stream) => Pin::new(stream).poll_shutdown(cx),
        }
    }
}

/// Turns an accepted TCP-family transport stream into a `(client stream, client addr)` pair,
/// applying receive-proxy-protocol / TLS termination / relay decode as configured.
pub async fn ingest_tcp(
    stream: tokio::net::TcpStream,
    peer: SocketAddr,
    ingest: &Ingest,
    receive_pp: Option<TcpProxyProtocol>,
) -> Result<(IngestStream, SocketAddr), BoxError> {
    match ingest {
        Ingest::Raw => {
            if receive_pp.is_some() {
                let mut s = stream;
                let (src, leftover) = read_proxy_header(&mut s).await?;
                let s = IngestStream::PrefixedTcp(Prefixed::new(leftover, s));
                Ok((s, src))
            } else {
                Ok((IngestStream::RawTcp(stream), peer))
            }
        }
        Ingest::Tls(acceptor) => match receive_pp {
            Some(_) => {
                let mut s = stream;
                let (client_addr, leftover) = read_proxy_header(&mut s).await?;
                let base = Prefixed::new(leftover, s);
                let tls = crate::tls::accept_tls(acceptor, base).await?;
                let s = IngestStream::TlsPrefixed(tls);
                Ok((s, client_addr))
            }
            None => {
                let client_addr = peer;
                let base = stream;
                let tls = crate::tls::accept_tls(acceptor, base).await?;
                let s = IngestStream::RawTls(tls);
                Ok((s, client_addr))
            }
        },
        Ingest::RelayTcp => {
            let mut s = stream;
            let (src, leftover) = read_proxy_header(&mut s).await?;
            let s = IngestStream::PrefixedTcp(Prefixed::new(leftover, s));
            Ok((s, src))
        }
        Ingest::RelayTls(acceptor) => {
            let mut tls = crate::tls::accept_tls(acceptor, stream).await?;
            let (src, leftover) = read_proxy_header(&mut tls).await?;
            let s = IngestStream::PrefixedTls(Prefixed::new(leftover, tls));
            Ok((s, src))
        }
        Ingest::RelayQuic => Err("quic ingest handled by quic listener".into()),
    }
}
