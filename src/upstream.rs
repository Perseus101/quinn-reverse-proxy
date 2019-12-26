use httparse::Request;
use hyper::client::connect::HttpConnector;
use hyper::{body::HttpBody as _, Body, Client, Request as HyperRequest, Uri};
use hyper_tls::HttpsConnector;
use bytebuffer::ByteBuffer;
use async_trait::async_trait;

use crate::error::{ProxyError, Result};

#[async_trait]
pub trait UpstreamTrait {
    async fn process_request<'a, 'b: 'a>(&self, request: Request<'a, 'b>, body: &'b [u8]) -> Result<ByteBuffer>;
}

pub struct Upstream {
    client: Client<HttpsConnector<HttpConnector>>,
    uri: String,
}

impl Upstream {
    pub fn new(uri: String) -> Result<Self> {
        let https = HttpsConnector::new();
        let client = Client::builder().build::<_, hyper::Body>(https);
        // Check that uri is valid
        uri.parse::<Uri>()
            .map_err(|_| ProxyError::ConfigurationError)?;
        Ok(Upstream { client, uri })
    }

    /// Builds a request to the upstream server based on the incoming request
    fn build_upstream_request<'a>(&self, request: Request, body: &'a [u8]) -> Result<HyperRequest<hyper::Body>> {
        let path = request.path.ok_or(ProxyError::InvalidRequest)?;
        let uri = format!("{}{}", self.uri, path);
        let method = String::from(request.method.ok_or(ProxyError::InvalidRequest)?);

        HyperRequest::builder()
            .method(method.as_bytes())
            .uri(uri)
            .body(Body::from(Vec::from(body)))
            .map_err(From::from)
    }
}

#[async_trait]
impl UpstreamTrait for Upstream {
    async fn process_request<'a, 'b: 'a>(&self, request: Request<'a, 'b>, body: &'b [u8]) -> Result<ByteBuffer> {
        let outgoing = self.build_upstream_request(request, body)?;
        let mut resp = self
            .client
            .request(outgoing)
            .await?;

        let mut buf = ByteBuffer::new();

        while let Some(next) = resp.data().await {
            let chunk = next?;
            buf.write_bytes(&chunk);
        }
        Ok(buf)
    }
}
