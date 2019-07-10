#[macro_use]
extern crate failure;
#[macro_use]
extern crate slog;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::fs;

use failure::ResultExt;
use slog::{Drain, Logger};
use structopt::{self, StructOpt};

use httparse::Request;
use hyper::{Body, Request as HyperRequest};
use futures::{Future, Stream, future::err};

mod error;
mod server;

use server::FutureResponse;

use error::{ErrorExt, Result, ProxyError};

#[derive(StructOpt, Debug)]
#[structopt(name = "server")]
struct Opt {
    /// TLS private key in PEM format
    #[structopt(parse(from_os_str), short = "k", long = "key", requires = "cert", default_value = "/opt/configs/key")]
    key: PathBuf,
    /// TLS certificate in PEM format
    #[structopt(parse(from_os_str), short = "c", long = "cert", requires = "key", default_value = "/opt/configs/cert")]
    cert: PathBuf,
    /// Enable stateless retries
    #[structopt(long = "stateless-retry")]
    stateless_retry: bool,
    /// Address to listen on
    #[structopt(long = "listen", default_value = "127.0.0.1:5001")]
    listen: SocketAddr,

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
    server::serve(&process_request_wrapper, log.clone(), server_config.build(), options.listen)
}

fn process_request_wrapper(request: Request) -> FutureResponse {
    let outgoing = match process_response(request) {
        Ok(outgoing) => outgoing,
        Err(e) => return Box::new(err(e)),
    };
    let client = hyper::Client::new();
    let future = client.request(outgoing)
        .and_then(|resp| {
            // Once the request to the upstream server is complete
            // convert the body into bytes to send as a response
            resp.map(|body| {
                body.concat2()
                    .map(|chunk| {
                        chunk.to_vec()
                    })
            }).into_body()
        })
        .map_err(|e| {
            println!("Body parsing error: {}", e);
            ProxyError::RequestFailure.into()
        });
    Box::new(future)
}

fn process_response(request: Request) -> Result<HyperRequest<hyper::Body>> {
    let path = request.path.ok_or(ProxyError::InvalidRequest)?;
    let url = format!("http://localhost:5000{}", path);
    let method = String::from(request.method.ok_or(ProxyError::InvalidRequest)?);


    HyperRequest::builder()
        .method(method.as_bytes())
        .uri(url)
        .body(Body::from("Test body"))
        .map_err(|err| err.into())
}