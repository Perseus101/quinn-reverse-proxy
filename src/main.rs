use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use structopt::{self, StructOpt};

use anyhow::Result;

mod error;
mod server;
mod upstream;

use error::ProxyError;

const ALPN_QUIC_HTTP: &[&[u8]] = &[b"hq-24"];

#[derive(StructOpt, Debug)]
#[structopt(name = "server")]
struct Opt {
    /// TLS private key
    #[structopt(
        parse(from_os_str),
        short = "k",
        long = "key",
        requires = "cert",
        default_value = "certs/key.pem"
    )]
    key: PathBuf,
    /// TLS certificate
    #[structopt(
        parse(from_os_str),
        short = "c",
        long = "cert",
        requires = "key",
        default_value = "certs/cert.pem"
    )]
    cert: PathBuf,
    /// Enable stateless retries
    #[structopt(long = "stateless-retry")]
    stateless_retry: bool,
    /// Address to listen on
    #[structopt(long = "listen", default_value = "0.0.0.0:80")]
    listen: SocketAddr,
    /// Address to reverse proxy to
    #[structopt(
        short = "u",
        long = "upstream",
        default_value = "http://localhost:5000"
    )]
    upstream: String,

    /// Logging verbosity
    #[structopt(short, parse(from_occurrences))]
    verbosity: u64,
}

fn main() {
    let opt = Opt::from_args();
    let level = match opt.verbosity {
        0 => tracing::Level::WARN,
        1 => tracing::Level::INFO,
        2 => tracing::Level::DEBUG,
        _ => tracing::Level::TRACE,
    };
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_max_level(level)
            .finish(),
    )
    .expect("tracing failed to set global default");
    let code = {
        if let Err(e) = run(opt) {
            eprintln!("ERROR: {}", e);
            1
        } else {
            0
        }
    };
    ::std::process::exit(code);
}

fn run(options: Opt) -> Result<()> {
    let server_config = quinn::ServerConfig {
        transport: Arc::new(quinn::TransportConfig {
            stream_window_uni: 0,
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut server_config = quinn::ServerConfigBuilder::new(server_config);
    server_config.protocols(ALPN_QUIC_HTTP);

    if options.stateless_retry {
        server_config.use_stateless_retry(true);
    }

    let key_path = options.key;
    let cert_path = options.cert;

    let key = fs::read(&key_path)?;
    let key = if key_path.extension().map_or(false, |x| x == "der") {
        quinn::PrivateKey::from_der(&key).map_err(|_| ProxyError::ConfigurationError)?
    } else {
        quinn::PrivateKey::from_pem(&key).map_err(|_| ProxyError::ConfigurationError)?
    };
    let cert_chain = fs::read(&cert_path)?;
    let cert_chain = if cert_path.extension().map_or(false, |x| x == "der") {
        quinn::CertificateChain::from_certs(quinn::Certificate::from_der(&cert_chain))
    } else {
        quinn::CertificateChain::from_pem(&cert_chain)
            .map_err(|_| ProxyError::ConfigurationError)?
    };
    server_config
        .certificate(cert_chain, key)
        .map_err(|_| ProxyError::ConfigurationError)?;

    server::serve(
        upstream::Upstream::new(options.upstream)?,
        server_config.build(),
        options.listen,
    )?;
    Ok(())
}
