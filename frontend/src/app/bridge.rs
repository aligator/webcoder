//! JS bridge to `assets/api.js` plus the JSON decode helpers shared by every
//! async call into it.

use serde::Deserialize;
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::*;
use web_sys::File;

#[wasm_bindgen(module = "/assets/api.js")]
extern "C" {
    #[wasm_bindgen(catch, js_name = getEncoders)]
    pub(crate) async fn get_encoders() -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_name = probeMedia)]
    pub(crate) async fn probe_media(file: File) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_name = pickNativeFiles)]
    pub(crate) async fn pick_native_files() -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_name = probeNativePath)]
    pub(crate) async fn probe_native_path(path: String) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_name = nativeApp)]
    pub(crate) fn native_app() -> bool;

    #[wasm_bindgen(js_name = listenNativeDrop)]
    pub(crate) fn listen_native_drop(callback: &JsValue);

    #[wasm_bindgen(catch, js_name = runEncode)]
    pub(crate) async fn run_encode(
        job_id: String,
        settings_json: String,
        tracks_json: String,
        output_dir: String,
        overwrite: bool,
    ) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_name = withApiKey)]
    pub(crate) fn with_api_key(url: &str) -> String;

    #[wasm_bindgen(catch, js_name = pickOutputDir)]
    pub(crate) async fn pick_output_dir() -> Result<JsValue, JsValue>;

    #[wasm_bindgen(js_name = listenEncodeProgress)]
    pub(crate) fn listen_encode_progress(callback: &JsValue);
}

/// Decode a JSON string returned by the api.js bridge into `T`.
pub(crate) fn parse_json<T: for<'de> Deserialize<'de>>(value: JsValue) -> Result<T, String> {
    let text = value
        .as_string()
        .ok_or("Expected a JSON string from bridge.")?;
    serde_json::from_str(&text).map_err(|e| format!("Bad response: {e}"))
}

pub(crate) fn js_error_text(error: JsValue) -> String {
    error
        .as_string()
        .or_else(|| {
            js_sys::Reflect::get(&error, &JsValue::from_str("message"))
                .ok()
                .and_then(|m| m.as_string())
        })
        .unwrap_or_else(|| "unknown error".into())
}
