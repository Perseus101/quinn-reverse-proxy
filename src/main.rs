#[macro_use]
extern crate failure;
#[macro_use]
extern crate slog;

use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use failure::ResultExt;
use slog::{Drain, Logger};
use structopt::{self, StructOpt};

mod error;
mod server;
mod upstream;

use error::{ErrorExt, Result};

#[derive(StructOpt, Debug)]
#[structopt(name = "server")]
struct Opt {
    /// TLS private key
    #[structopt(
        parse(from_os_str),
        short = "k",
        long = "key",
        requires = "cert",
        default_value = "certs/key.der"
    )]
    key: PathBuf,
    /// TLS certificate
    #[structopt(
        parse(from_os_str),
        short = "c",
        long = "cert",
        requires = "key",
        default_value = "certs/cert.der"
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
}

fn main() {
    let opt = Opt::from_args();
    let code = {
        let decorator = slog_term::PlainSyncDecorator::new(std::io::stderr());
        let drain = slog_term::FullFormat::new(decorator)
            .use_original_order()
            .build()
            .fuse();
        if let Err(e) = run(Logger::root(drain, o!()), opt) {
            eprintln!("ERROR: {}", e.pretty());
            1
        } else {
            0
        }
    };
    ::std::process::exit(code);
}

fn run(log: Logger, options: Opt) -> Result<()> {
    let server_config = quinn::ServerConfig {
        transport: Arc::new(quinn::TransportConfig {
            stream_window_uni: 0,
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut server_config = quinn::ServerConfigBuilder::new(server_config);
    server_config.protocols(&[quinn::ALPN_QUIC_HTTP]);

    if options.stateless_retry {
        server_config.use_stateless_retry(true);
    }

    let key_path = options.key;
    let cert_path = options.cert;

    let key = fs::read(&key_path).context("failed to read private key")?;
    let key = if key_path.extension().map_or(false, |x| x == "der") {
        quinn::PrivateKey::from_der(&key)?
    } else {
        quinn::PrivateKey::from_pem(&key)?
    };
    let cert_chain = fs::read(&cert_path).context("failed to read certificate chain")?;
    let cert_chain = if cert_path.extension().map_or(false, |x| x == "der") {
        quinn::CertificateChain::from_certs(quinn::Certificate::from_der(&cert_chain))
    } else {
        quinn::CertificateChain::from_pem(&cert_chain)?
    };
    server_config.certificate(cert_chain, key)?;
    server::serve(
        upstream::Upstream::new(options.upstream)?,
        log.clone(),
        server_config.build(),
        options.listen,
    )
}
