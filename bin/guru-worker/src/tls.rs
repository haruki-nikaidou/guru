use crate::config::TlsHostConfig;
use crate::pipe::AsyncRw;
use openssl::ssl::{Ssl, SslAcceptor, SslConnector, SslFiletype, SslMethod};

/// Builds a TLS server acceptor from a cert chain + key on disk (parsed once).
pub fn server_acceptor(c: &TlsHostConfig) -> Result<SslAcceptor, crate::BoxError> {
    let mut b = SslAcceptor::mozilla_intermediate_v5(SslMethod::tls())?;
    b.set_private_key_file(&c.key, SslFiletype::PEM)?;
    b.set_certificate_chain_file(&c.full_chain)?;
    b.check_private_key()?;
    Ok(b.build())
}

/// Cached system-roots TLS connector for dialing relay hops over TLS-over-TCP.
fn client_connector() -> Result<&'static SslConnector, crate::BoxError> {
    static C: std::sync::OnceLock<SslConnector> = std::sync::OnceLock::new();
    if let Some(c) = C.get() {
        return Ok(c);
    }
    let b = SslConnector::builder(SslMethod::tls())?; // default verify + system CA paths
    let c = b.build();
    Ok(C.get_or_init(|| c))
}

/// Terminates TLS on an accepted stream.
pub async fn accept_tls<S: AsyncRw>(
    acceptor: &SslAcceptor,
    stream: S,
) -> Result<tokio_openssl::SslStream<S>, crate::BoxError> {
    let ssl = Ssl::new(acceptor.context())?;
    let mut s = tokio_openssl::SslStream::new(ssl, stream)?;
    std::pin::Pin::new(&mut s).accept().await?;
    Ok(s)
}

/// Establishes a client TLS session over an existing TCP connection, verifying the
/// server against the system root store using `sni`.
pub async fn connect_tls(
    sni: &str,
    stream: tokio::net::TcpStream,
) -> Result<tokio_openssl::SslStream<tokio::net::TcpStream>, crate::BoxError> {
    let ssl = client_connector()?.configure()?.into_ssl(sni)?;
    let mut s = tokio_openssl::SslStream::new(ssl, stream)?;
    std::pin::Pin::new(&mut s).connect().await?;
    Ok(s)
}

/// Builds a quinn server config from a cert chain + key on disk.
pub fn quic_server_config(c: &TlsHostConfig) -> Result<quinn::ServerConfig, crate::BoxError> {
    use quinn::rustls::pki_types::{CertificateDer, PrivateKeyDer};
    let chain: Vec<CertificateDer<'static>> = {
        let f = std::fs::File::open(&c.full_chain)?;
        let mut r = std::io::BufReader::new(f);
        rustls_pemfile::certs(&mut r).collect::<Result<Vec<_>, _>>()?
    };
    let key: PrivateKeyDer<'static> = {
        let f = std::fs::File::open(&c.key)?;
        let mut r = std::io::BufReader::new(f);
        rustls_pemfile::private_key(&mut r)?.ok_or("no private key in key file")?
    };
    Ok(quinn::ServerConfig::with_single_cert(chain, key)?)
}

/// Shared quinn client endpoint for dialing relay hops over QUIC (system roots).
pub fn quic_client_endpoint() -> Result<quinn::Endpoint, crate::BoxError> {
    static E: std::sync::OnceLock<quinn::Endpoint> = std::sync::OnceLock::new();
    if let Some(e) = E.get() {
        return Ok(e.clone());
    }
    let mut roots = quinn::rustls::RootCertStore::empty();
    for cert in rustls_native_certs::load_native_certs().certs {
        let _ = roots.add(cert);
    }
    let client_cfg = quinn::ClientConfig::with_root_certificates(std::sync::Arc::new(roots))?;
    let addr: std::net::SocketAddr = "[::]:0".parse()?;
    let mut ep = quinn::Endpoint::client(addr)?;
    ep.set_default_client_config(client_cfg);
    Ok(E.get_or_init(|| ep).clone())
}
