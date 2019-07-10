use futures::{future::err, Future, Stream};
use httparse::Request;
use hyper::client::connect::HttpConnector;
use hyper::{Body, Client, Request as HyperRequest, Uri};
use hyper_tls::HttpsConnector;

use crate::error::{ProxyError, Result};
use crate::server::FutureResponse;

pub struct Upstream {
    client: Client<HttpsConnector<HttpConnector>>,
    uri: String,
}

impl Upstream {
    pub fn new(uri: String) -> Result<Self> {
        let https = HttpsConnector::new(4).expect("TLS initialization failed");
        let client = Client::builder().build::<_, hyper::Body>(https);
        // Check that uri is valid
        uri.parse::<Uri>()?;
        Ok(Upstream { client, uri })
    }

    pub fn process_request(&self, request: Request) -> FutureResponse {
        let outgoing = match self.build_upstream_request(request) {
            Ok(outgoing) => outgoing,
            Err(e) => return Box::new(err(e)),
        };
        let future = self
            .client
            .request(outgoing)
            .and_then(|resp| {
                // Once the request to the upstream server is complete
                // convert the body into bytes to send as a response
                resp.map(|body| body.concat2().map(|chunk| chunk.to_vec()))
                    .into_body()
            })
            .map_err(|e| {
                println!("Body parsing error: {}", e);
                ProxyError::RequestFailure.into()
            });
        Box::new(future)
    }

    /// Builds a request to the upstream server based on the incoming request
    fn build_upstream_request(&self, request: Request) -> Result<HyperRequest<hyper::Body>> {
        let path = request.path.ok_or(ProxyError::InvalidRequest)?;
        let uri = format!("{}{}", self.uri, path);
        let method = String::from(request.method.ok_or(ProxyError::InvalidRequest)?);

        HyperRequest::builder()
            .method(method.as_bytes())
            .uri(uri)
            .body(Body::empty())
            .map_err(|err| err.into())
    }
}
