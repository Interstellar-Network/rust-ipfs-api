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
use http::{
    header::{HeaderName, HeaderValue},
    uri::Scheme,
    StatusCode as HttpStatusCode, Uri as HttpUri,
};
use http_req::{
    request::Method as HttpReqMethod, request::RequestBuilder,
    response::StatusCode as HttpReqStatusCode, tls, uri::Uri as HttpReqUri,
};
use ipfs_api_prelude::{ApiRequest, Backend, BoxStream, TryFromUri};
use multipart::client::multipart;

#[derive(Clone)]
pub struct HttpReqBackend {
    base: HttpUri,
    // phantom: PhantomData<&'a T>,
}

impl Default for HttpReqBackend {
    /// Creates an `IpfsClient` connected to the endpoint specified in ~/.ipfs/api.
    /// If not found, tries to connect to `localhost:5001`.
    ///
    fn default() -> Self {
        Self::from_ipfs_config()
            .unwrap_or_else(|| Self::from_host_and_port(Scheme::HTTP, "localhost", 5001).unwrap())
    }
}

impl TryFromUri for HttpReqBackend {
    fn build_with_base_uri(base: HttpUri) -> Self {
        HttpReqBackend { base }
    }
}

#[cfg_attr(feature = "with-send-sync", async_trait)]
#[cfg_attr(not(feature = "with-send-sync"), async_trait(?Send))]
impl Backend for HttpReqBackend {
    type HttpRequest = http_req::request::Request<'static>;

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
        // TODO(interstellar) uri!
        // let url: Uri = req.absolute_url(&self.base.to_string())?;
        let url = HttpReqUri::try_from("TODO").unwrap();

        // TODO(interstellar) cleanup
        // let builder = http::Request::builder();
        // let builder = builder.method(Req::METHOD).uri(url);
        let builder = RequestBuilder::new(&url);
        // builder.method(Req::);

        let req = if let Some(form) = form {
            form.set_body_convert::<&[u8], multipart::Body>(builder)
        } else {
            builder.body(&[0u8; 0])
        }?;

        Ok(req)
    }

    fn get_header(res: &Self::HttpResponse, key: HeaderName) -> Option<&HeaderValue> {
        let header_str = match res.headers().get(&key) {
            Some(header) => header,
            None => return None,
        };

        match HeaderValue::from_str(header_str) {
            Ok(header_value) => Some(&header_value),
            _ => None,
        }
    }

    async fn request_raw<Req>(
        &self,
        req: Req,
        form: Option<multipart::Form<'static>>,
    ) -> Result<(HttpStatusCode, Bytes), Self::Error>
    where
        Req: ApiRequest,
    {
        let req = self.build_base_request(req, form)?;

        //Container for response's body
        let mut writer = Vec::new();
        let resp = req.send(&mut writer)?;

        let status_http_req: HttpReqStatusCode = resp.status_code();
        let body = writer.try_into().unwrap();

        let status_http = HttpStatusCode::from_u16(status_http_req.try_into().unwrap()).unwrap();

        Ok((status_http, body))
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
        //Container for response's body
        let mut writer = Vec::new();

        let stream = req
            .send(&mut writer)
            .err_into()
            .map_ok(move |res| {
                match res.status_code() {
                    HttpReqStatusCode::OK => process(res).right_stream(),
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
