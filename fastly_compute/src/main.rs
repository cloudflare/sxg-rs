// Copyright 2021 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

mod fetcher;

use anyhow::{Error, Result};
use fastly::{
    http::{StatusCode, Url},
    mime::Mime,
    Request, Response,
};
use fetcher::FastlyFetcher;
use futures::executor::block_on;
use once_cell::sync::Lazy;
use std::convert::TryInto;
use sxg_rs::{
    headers::{AcceptFilter, Headers},
    http::HeaderFields,
    PresetContent,
};

pub static WORKER: Lazy<::sxg_rs::SxgWorker> = Lazy::new(|| {
    ::sxg_rs::SxgWorker::new(
        include_str!("../config.yaml"),
        include_str!("../../credentials/cert.pem"),
        include_str!("../../credentials/issuer.pem"),
    )
    .unwrap()
});

fn binary_response(status_code: StatusCode, content_type: Mime, body: &[u8]) -> Response {
    let mut response = Response::new();
    response.set_status(status_code);
    response.set_content_type(content_type);
    response.set_body(body);
    response
}

fn text_response(body: &str) -> Response {
    binary_response(StatusCode::OK, fastly::mime::TEXT_PLAIN, body.as_bytes())
}

fn get_req_header_fields(req: &Request, accept_filter: AcceptFilter) -> Result<HeaderFields> {
    let mut fields: Vec<(String, String)> = vec![];
    for name in req.get_header_names() {
        for value in req.get_header_all(name) {
            let value = value.to_str().map_err(|_| {
                Error::msg(format!(r#"Header "{}" contains non-ASCII value."#, name))
            })?;
            fields.push((name.as_str().to_string(), value.to_string()))
        }
    }
    WORKER.transform_request_headers(fields, accept_filter)
}

fn get_rsp_header_fields(rsp: &Response) -> Result<Headers> {
    let mut fields: Vec<(String, String)> = vec![];
    for name in rsp.get_header_names() {
        for value in rsp.get_header_all(name) {
            let value = value.to_str().map_err(|_| {
                Error::msg(format!(r#"Header "{}" contains non-ASCII value."#, name))
            })?;
            fields.push((name.as_str().to_string(), value.to_string()))
        }
    }
    WORKER.transform_payload_headers(fields)
}

pub fn sxg_rs_response_to_fastly_response(
    rsp: sxg_rs::http::HttpResponse,
) -> anyhow::Result<fastly::Response> {
    let rsp: ::http::response::Response<Vec<u8>> = rsp.try_into()?;
    let rsp: ::http::response::Response<fastly::Body> = rsp.map(From::<Vec<u8>>::from);
    Ok(rsp.into())
}

fn fetch_from_html_server(url: &Url, req_headers: Vec<(String, String)>) -> Result<Response> {
    let mut req = Request::new("GET", url);
    for (name, value) in req_headers {
        req.append_header(name, value);
    }
    req.send("Origin HTML server")
        .map_err(|err| Error::msg(format!(r#"Fetching "{}" leads to error "{}""#, url, err)))
}

fn generate_sxg_response(fallback_url: &Url, payload: Response) -> Result<Response> {
    let signer = WORKER.create_rust_signer()?;
    let payload_headers = get_rsp_header_fields(&payload)?;
    let payload_body = payload.into_body_bytes();
    let cert_origin = fallback_url.origin().ascii_serialization();
    let subresource_fetcher = FastlyFetcher::new("subresources");
    let sxg = WORKER.create_signed_exchange(sxg_rs::CreateSignedExchangeParams {
        now: std::time::SystemTime::now(),
        payload_body: &payload_body,
        payload_headers,
        signer,
        status_code: 200,
        fallback_url: fallback_url.as_str(),
        cert_origin: &cert_origin,
        subresource_fetcher,
        // The fastly crate provides only read access to dictionaries, so
        // header integrities cannot be cached. However, I believe the
        // subresource_fetcher will go through the cache.
        header_integrity_cache: sxg_rs::http_cache::NullCache {},
    });
    let sxg = block_on(sxg)?;
    sxg_rs_response_to_fastly_response(sxg)
}

fn handle_request(req: Request) -> Result<Response> {
    let fetcher = FastlyFetcher::new("OCSP server");
    // TODO: store OCSP in database
    let ocsp_der = WORKER.fetch_ocsp_from_ca(fetcher);
    let ocsp_der = block_on(ocsp_der);
    let fallback_url: Url;
    let sxg_payload;
    match WORKER.serve_preset_content(req.get_url_str(), &ocsp_der) {
        Some(PresetContent::Direct(response)) => {
            return sxg_rs_response_to_fastly_response(response)
        }
        Some(PresetContent::ToBeSigned { url, payload, .. }) => {
            fallback_url = Url::parse(&url).map_err(Error::new)?;
            sxg_payload = sxg_rs_response_to_fastly_response(payload)?;
            get_req_header_fields(&req, AcceptFilter::AcceptsSxg)?;
        }
        None => {
            fallback_url = WORKER.get_fallback_url(req.get_url())?;
            let req_headers = get_req_header_fields(&req, AcceptFilter::PrefersSxg)?;
            sxg_payload = fetch_from_html_server(&fallback_url, req_headers)?;
        }
    };
    generate_sxg_response(&fallback_url, sxg_payload)
}

#[fastly::main]
fn main(req: Request) -> Result<Response, std::convert::Infallible> {
    let response = handle_request(req).unwrap_or_else(|msg| {
        text_response(&format!("A message is gracefully thrown.\n{:?}", msg))
    });
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn it_works() {
        WORKER.create_rust_signer().unwrap();
    }
}
