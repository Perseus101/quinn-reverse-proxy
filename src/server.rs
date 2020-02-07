use std::{clone::Clone, net::SocketAddr, sync::Arc};

use quinn::ServerConfig;

use futures::{StreamExt, TryFutureExt};
use tokio::runtime::Builder;
use tracing::{error, info, info_span};
use tracing_futures::Instrument as _;

use crate::error::{ProxyError, Result};
use crate::upstream::{Upstream, UpstreamTrait};

pub(crate) fn serve(
    upstream: Upstream,
    server_config: ServerConfig,
    listen: SocketAddr,
) -> Result<()> {
    let mut runtime = Builder::new()
        .basic_scheduler()
        .core_threads(4)
        .enable_all()
        .build()?;

    let mut endpoint = quinn::Endpoint::builder();
    endpoint.listen(server_config);

    let upstream = Arc::new(upstream);

    let (endpoint_driver, mut incoming) = {
        let (driver, endpoint, incoming) = runtime.enter(|| endpoint.bind(&listen))?;
        info!("listening on {}", endpoint.local_addr()?);
        (driver, incoming)
    };

    runtime.spawn(async move {
        while let Some(conn) = incoming.next().await {
            info!("connection incoming");
            tokio::spawn(
                handle_connection(upstream.clone(), conn).unwrap_or_else(move |e| {
                    error!("connection failed: {reason}", reason = e.to_string())
                }),
            );
        }
    });
    runtime.block_on(endpoint_driver)?;

    Ok(())
}

async fn handle_connection(upstream: Arc<Upstream>, conn: quinn::Connecting) -> Result<()> {
    let quinn::NewConnection {
        driver,
        connection,
        mut bi_streams,
        ..
    } = conn.await?;
    let span = info_span!(
        "connection",
        remote = %connection.remote_address(),
        protocol = %connection.protocol().map_or_else(|| "<none>".into(), |x| String::from_utf8_lossy(&x).into_owned())
    );
    tokio::spawn(driver.unwrap_or_else(|_| ()).instrument(span.clone()));
    async {
        info!("established");

        // We ignore errors from the driver because they'll be reported by the `streams` handler anyway.

        // Each stream initiated by the client constitutes a new request.
        while let Some(stream) = bi_streams.next().await {
            let stream = match stream {
                Err(quinn::ConnectionError::ApplicationClosed { reason }) => {
                    info!("connection closed: {}", reason);
                    return Ok(());
                }
                Err(e) => {
                    return Err(e);
                }
                Ok(s) => s,
            };
            tokio::spawn(
                handle_request(upstream.clone(), stream)
                    .unwrap_or_else(move |e| error!("failed: {:?}", e))
                    .instrument(info_span!("request")),
            );
        }
        Ok(())
    }
    .instrument(span)
    .await?;
    Ok(())
}

async fn handle_request(
    upstream: Arc<Upstream>,
    (mut send, recv): (quinn::SendStream, quinn::RecvStream),
) -> Result<()> {
    let req = recv.read_to_end(1024 * 1024 * 1024).await?; // Read request, maximum size of 1GB
    let mut headers = [httparse::EMPTY_HEADER; 16];
    let mut parsed = httparse::Request::new(&mut headers);

    let len = match parsed.parse(&req) {
        Err(e) => {
            info!("parsing request failed: {:?}", e);
            Err(ProxyError::InvalidRequest)
        }
        Ok(httparse::Status::Partial) => {
            info!("incomplete request");
            Err(ProxyError::InvalidRequest)
        }
        Ok(httparse::Status::Complete(len)) => Ok(len),
    }?;
    let body = &req[len..];
    let resp = upstream.process_request(parsed, body).await?;
    // Write the response
    send.write_all(&resp.to_bytes()).await?;
    // Gracefully terminate the stream
    send.finish().await?;
    Ok(())
}
