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

use anyhow::Result;
use clap::Parser;
use futures::executor;
use std::fs;
use sxg_rs::{fetcher::NULL_FETCHER, http_cache::NullCache, CreateSignedExchangeParams, SxgWorker};

// TODO: Make this binary generally useful, by documenting the flags and giving them names.

#[derive(Parser)]
pub struct Opts {
    config_yaml: String,
    cert_pem: String,
    issuer_pem: String,
    out_cert_cbor: String,
    out_sxg: String,
}

pub fn main(opts: Opts) -> Result<()> {
    let worker = SxgWorker::new(
        &fs::read_to_string(opts.config_yaml).unwrap(),
        &fs::read_to_string(opts.cert_pem).unwrap(),
        &fs::read_to_string(opts.issuer_pem).unwrap(),
    )
    .unwrap();
    fs::write(opts.out_cert_cbor, &worker.create_cert_cbor(b"ocsp"))?;
    let payload_headers = worker
        .transform_payload_headers(vec![("content-type".into(), "text/html".into())])
        .unwrap();
    let sxg = worker.create_signed_exchange(CreateSignedExchangeParams {
        fallback_url: "https://test.example/",
        cert_origin: "https://test.example",
        now: std::time::SystemTime::now(),
        payload_body: b"This is a test.",
        payload_headers,
        signer: worker.create_rust_signer().unwrap(),
        status_code: 200,
        subresource_fetcher: NULL_FETCHER,
        header_integrity_cache: NullCache {},
    });
    let sxg = executor::block_on(sxg);
    fs::write(opts.out_sxg, &sxg.unwrap().body)?;
    Ok(())
}
