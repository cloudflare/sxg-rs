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

// TODO(antiphoton) No longer allow unused_unit when a new version wasm_bindgen is released with
// https://github.com/rustwasm/wasm-bindgen/pull/2778
#![allow(clippy::unused_unit)]

use crate::headers::AcceptFilter;
use crate::http::HttpResponse;
use crate::utils::to_js_error;
use crate::SxgWorker;
use anyhow::Result;
use js_sys::Function as JsFunction;
use js_sys::Promise as JsPromise;
use js_sys::Uint8Array;
use std::sync::Arc;
use wasm_bindgen::prelude::{wasm_bindgen, JsValue};
use wasm_bindgen_futures::future_to_promise;

#[wasm_bindgen]
pub struct WasmWorker(Arc<SxgWorker>);

#[wasm_bindgen]
impl WasmWorker {
    #[wasm_bindgen(constructor)]
    pub fn new(config_yaml: &str, cert_pem: &str, issuer_pem: &str) -> Result<WasmWorker, JsValue> {
        let sxg_worker = SxgWorker::new(config_yaml, cert_pem, issuer_pem).map_err(to_js_error)?;
        Ok(WasmWorker(Arc::new(sxg_worker)))
    }
    #[wasm_bindgen(js_name=fetchOcspFromCa)]
    pub fn fetch_ocsp_from_ca(&self, fetcher: JsFunction) -> JsPromise {
        let worker = self.0.clone();
        future_to_promise(async move {
            let fetcher = crate::fetcher::js_fetcher::JsFetcher::new(fetcher);
            let response = worker.fetch_ocsp_from_ca(fetcher).await;
            let ocsp = Uint8Array::from(response.as_slice());
            Ok(JsValue::from(ocsp))
        })
    }
    #[wasm_bindgen(js_name=servePresetContent)]
    pub fn serve_preset_content(
        &self,
        req_url: &str,
        ocsp_base64: &str,
    ) -> Result<JsValue, JsValue> {
        let ocsp_der = ::base64::decode(ocsp_base64).unwrap();
        Ok(self
            .0
            .serve_preset_content(req_url, &ocsp_der)
            .map_or(JsValue::UNDEFINED, |preset_content| {
                JsValue::from_serde(&preset_content).unwrap()
            }))
    }
    #[wasm_bindgen(js_name=shouldRespondDebugInfo)]
    pub fn should_respond_debug_info(&self) -> Result<bool, JsValue> {
        Ok(self.0.should_respond_debug_info())
    }

    #[wasm_bindgen(js_name=createRequestHeaders)]
    pub fn create_request_headers(
        &self,
        accept_filter: JsValue,
        requestor_headers: JsValue,
    ) -> Result<JsValue, JsValue> {
        let fields = requestor_headers.into_serde().unwrap();
        let accept_filter: AcceptFilter = accept_filter.into_serde().unwrap();
        let result = self.0.transform_request_headers(fields, accept_filter);
        match result {
            Ok(fields) => Ok(JsValue::from_serde(&fields).unwrap()),
            Err(err) => Err(to_js_error(err)),
        }
    }
    #[wasm_bindgen(js_name=validatePayloadHeaders)]
    pub fn validate_payload_headers(&self, fields: JsValue) -> Result<(), JsValue> {
        let fields: Vec<(String, String)> = fields.into_serde().map_err(to_js_error)?;
        self.0
            .transform_payload_headers(fields)
            .map_err(to_js_error)?;
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    #[wasm_bindgen(js_name=createSignedExchange)]
    pub fn create_signed_exchange(
        &self,
        fallback_url: String,
        cert_origin: String,
        status_code: u16,
        payload_headers: JsValue,
        payload_body: Vec<u8>,
        now_in_seconds: u32,
        signer: JsFunction,
        subresource_fetcher: JsFunction,
        header_integrity_get: JsFunction,
        header_integrity_put: JsFunction,
    ) -> JsPromise {
        let worker = self.0.clone();
        future_to_promise(async move {
            let payload_headers: Vec<(String, String)> =
                payload_headers.into_serde().map_err(to_js_error)?;
            let payload_headers = worker
                .transform_payload_headers(payload_headers)
                .map_err(to_js_error)?;
            let signer = crate::signature::js_signer::JsSigner::from_raw_signer(signer);
            let subresource_fetcher =
                crate::fetcher::js_fetcher::JsFetcher::new(subresource_fetcher);
            let header_integrity_cache = crate::http_cache::js_http_cache::JsHttpCache {
                get: header_integrity_get,
                put: header_integrity_put,
            };
            let sxg: HttpResponse = worker
                .create_signed_exchange(crate::CreateSignedExchangeParams {
                    fallback_url: &fallback_url,
                    cert_origin: &cert_origin,
                    now: std::time::UNIX_EPOCH
                        + std::time::Duration::from_secs(now_in_seconds as u64),
                    payload_body: &payload_body,
                    payload_headers,
                    signer,
                    status_code,
                    subresource_fetcher,
                    header_integrity_cache,
                })
                .await
                .map_err(to_js_error)?;
            Ok(JsValue::from_serde(&sxg).unwrap())
        })
    }
}
