// Copyright 2021 rust-ipfs-api Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.
//

use crate::error::Error;
use async_trait::async_trait;
use bytes::Bytes;
use futures::{FutureExt, StreamExt, TryFutureExt, TryStreamExt};
// use http::{
//     header::{HeaderName, HeaderValue},
//     uri::Scheme,
//     StatusCode, Uri,
// };
use http_req::{request::RequestBuilder, response::StatusCode, tls, uri::Uri};
use ipfs_api_prelude::{ApiRequest, Backend, BoxStream, TryFromUri};
use multipart::client::multipart;

#[derive(Clone)]
pub struct HttpReqBackend<'a> {
    base: Uri<'a>,
}

macro_rules! impl_default {
    ($http_connector:path) => {
        impl_default!();
    };
    ($http_connector:path, $constructor:expr) => {
        impl Default for HttpReqBackend {
            /// Creates an `IpfsClient` connected to the endpoint specified in ~/.ipfs/api.
            /// If not found, tries to connect to `localhost:5001`.
            ///
            fn default() -> Self {
                Self::from_ipfs_config().unwrap_or_else(|| {
                    Self::from_host_and_port(Scheme::HTTP, "localhost", 5001).unwrap()
                })
            }
        }

        impl TryFromUri for HttpReqBackend {
            fn build_with_base_uri(base: Uri) -> Self {
                let client = Builder::default()
                    .pool_max_idle_per_host(0)
                    .build($constructor);

                HttpReqBackend { base, client }
            }
        }
    };
}

// Because the Hyper TLS connector supports both HTTP and HTTPS,
// if TLS is enabled, always use the TLS connector as default.
//
// Otherwise, compile errors will result due to ambiguity:
//
//   * "cannot infer type for struct `IpfsClient<_>`"
//
#[cfg(not(feature = "with-hyper-tls"))]
#[cfg(not(feature = "with-hyper-rustls"))]
impl_default!(HttpConnector);

#[cfg(feature = "with-hyper-tls")]
impl_default!(hyper_tls::HttpsConnector<HttpConnector>);

#[cfg(feature = "with-hyper-rustls")]
impl_default!(
    hyper_rustls::HttpsConnector<HttpConnector>,
    hyper_rustls::HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_or_http()
        .enable_http1()
        .build()
);

#[cfg_attr(feature = "with-send-sync", async_trait)]
#[cfg_attr(not(feature = "with-send-sync"), async_trait(?Send))]
impl<'a> Backend for HttpReqBackend<'a> {
    type HttpRequest = http_req::request::Request<'a>;

    type HttpResponse = http_req::response::Response;

    type Error = Error;

    fn build_base_request<Req>(
        &self,
        req: Req,
        form: Option<multipart::Form<'static>>,
    ) -> Result<Self::HttpRequest, Error>
    where
        Req: ApiRequest,
    {
        let url = req.absolute_url(&self.base)?;

        // TODO(interstellar) cleanup
        // let builder = http::Request::builder();
        // let builder = builder.method(Req::METHOD).uri(url);
        let builder = RequestBuilder::new(&url);
        builder.method(Req::METHOD);

        let req = if let Some(form) = form {
            form.set_body_convert::<&'a [u8], multipart::Body>(builder)
        } else {
            builder.body(&[0u8; 0])
        }?;

        Ok(req)
    }

    fn get_header(res: &Self::HttpResponse, key: String) -> Option<&String> {
        res.headers().get(key)
    }

    async fn request_raw<Req>(
        &self,
        req: Req,
        form: Option<multipart::Form<'static>>,
    ) -> Result<(StatusCode, Bytes), Self::Error>
    where
        Req: ApiRequest,
    {
        let req = self.build_base_request(req, form)?;

        //Container for response's body
        let mut writer = Vec::new();
        let resp = req.send(&mut writer)?;

        let status = resp.status_code();
        let body = writer.try_into().unwrap();

        Ok((status, body))
    }

    fn response_to_byte_stream(res: Self::HttpResponse) -> BoxStream<Bytes, Self::Error> {
        Box::new(res.into_body().err_into())
    }

    fn request_stream<Res, F>(
        &self,
        req: Self::HttpRequest,
        process: F,
    ) -> BoxStream<Res, Self::Error>
    where
        F: 'static + Send + Fn(Self::HttpResponse) -> BoxStream<Res, Self::Error>,
    {
        let stream = self
            .request(req)
            .err_into()
            .map_ok(move |res| {
                match res.status() {
                    StatusCode::OK => process(res).right_stream(),
                    // If the server responded with an error status code, the body
                    // still needs to be read so an error can be built. This block will
                    // read the entire body stream, then immediately return an error.
                    //
                    _ => body::to_bytes(res.into_body())
                        .boxed()
                        .map(|maybe_body| match maybe_body {
                            Ok(body) => Err(Self::process_error_from_body(body)),
                            Err(e) => Err(e.into()),
                        })
                        .into_stream()
                        .left_stream(),
                }
            })
            .try_flatten_stream();

        Box::new(stream)
    }
}
