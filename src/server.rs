use std::net::SocketAddr;
use std::rc::Rc;

use futures::{future::err, Future, Stream};
use tokio::runtime::current_thread::Runtime;

use quinn::ServerConfig;

use slog::Logger;

use crate::error::{ErrorExt, ProxyError, Result};
use crate::upstream::Upstream;

pub(crate) type FutureResponse = Box<dyn Future<Item = Vec<u8>, Error = failure::Error>>;

pub(crate) fn serve(
    upstream: Upstream,
    log: Logger,
    server_config: ServerConfig,
    listen: SocketAddr,
) -> Result<()> {
    let upstream = Rc::new(upstream);
    let mut endpoint = quinn::Endpoint::builder();
    endpoint.logger(log.clone());
    endpoint.listen(server_config);

    let (endpoint_driver, incoming) = {
        let (driver, endpoint, incoming) = endpoint.bind(listen)?;
        info!(log, "listening on {}", endpoint.local_addr()?);
        (driver, incoming)
    };

    let mut runtime = Runtime::new()?;
    runtime.spawn(incoming.for_each(move |conn| {
        handle_connection(upstream.clone(), log.clone(), conn);
        Ok(())
    }));
    runtime.block_on(endpoint_driver)?;

    Ok(())
}

fn handle_connection(
    upstream: Rc<Upstream>,
    log: Logger,
    conn: (
        quinn::ConnectionDriver,
        quinn::Connection,
        quinn::IncomingStreams,
    ),
) {
    let (conn_driver, conn, incoming_streams) = conn;
    info!(log, "got connection";
          "remote_id" => %conn.remote_id(),
          "address" => %conn.remote_address(),
          "protocol" => conn.protocol().map_or_else(|| "<none>".into(), |x| String::from_utf8_lossy(&x).into_owned()));

    // We ignore errors from the driver because they'll be reported by the `incoming` handler anyway.
    tokio_current_thread::spawn(conn_driver.map_err(|_| ()));

    // Each stream initiated by the client constitutes a new request.
    tokio_current_thread::spawn(
        incoming_streams
            .map_err({
                let log = log.clone();
                move |e| info!(log, "connection terminated"; "reason" => %e)
            })
            .for_each(move |stream| {
                handle_request(upstream.clone(), &log, stream);
                Ok(())
            }),
    );
}

fn handle_request(upstream: Rc<Upstream>, log: &Logger, stream: quinn::NewStream) {
    let log = log.clone();
    let local_log = log.clone();
    let (send, recv) = match stream {
        quinn::NewStream::Bi(send, recv) => (send, recv),
        quinn::NewStream::Uni(_) => unreachable!("disabled by endpoint configuration"),
    };

    tokio_current_thread::spawn(
        recv.read_to_end(64 * 1024) // Read the request, which must be at most 64KiB
            .map_err(|e| format_err!("failed reading request: {}", e))
            .and_then(move |(_, req)| {
                info!(log, "got request"; "len" => req.len());
                // Execute the request
                let mut headers = [httparse::EMPTY_HEADER; 16];
                let mut parsed = httparse::Request::new(&mut headers);
                let len = match parsed.parse(&req) {
                    Err(e) => {
                        debug!(log, "parsing request failed"; "error" => format!("{:?}", e));
                        return Box::new(err(ProxyError::InvalidRequest.into())) as FutureResponse;
                    },
                    Ok(httparse::Status::Partial) => {
                        debug!(log, "incomplete request");
                        return Box::new(err(ProxyError::InvalidRequest.into())) as FutureResponse;
                    },
                    Ok(httparse::Status::Complete(len)) => len,
                };
                let body = &req[len..];
                upstream.process_request(parsed, body)
            })
            .and_then(move |resp| {
                // Write the response
                tokio::io::write_all(send, resp)
                    .map_err(|e| format_err!("failed to send response: {}", e))
            })
            // Gracefully terminate the stream
            .and_then(|(send, _)| {
                tokio::io::shutdown(send)
                    .map_err(|e| format_err!("failed to shutdown stream: {}", e))
            })
            .map({
                let log = local_log.clone();
                move |_| info!(log, "request complete")
            })
            .map_err({
                let log = local_log.clone();
                move |e| error!(log, "request failed"; "reason" => %e.pretty())
            }),
    )
}
