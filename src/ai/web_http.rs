use super::provider::{http_error, Provider};
use serde_json::Value;
use wasm_bindgen::{closure::Closure, JsCast};
use wasm_bindgen_futures::JsFuture;

struct Deadline {
    controller: web_sys::AbortController,
    timer: i32,
    _callback: Closure<dyn FnMut()>,
}

impl Deadline {
    fn new(ms: i32) -> Result<Self, String> {
        let controller =
            web_sys::AbortController::new().map_err(|_| "Cannot create request cancellation")?;
        let abort = controller.clone();
        let callback = Closure::new(move || abort.abort());
        let timer = web_sys::window()
            .ok_or("Browser window unavailable")?
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                callback.as_ref().unchecked_ref(),
                ms,
            )
            .map_err(|_| "Cannot schedule request timeout")?;
        Ok(Self {
            controller,
            timer,
            _callback: callback,
        })
    }
}

impl Drop for Deadline {
    fn drop(&mut self) {
        if let Some(window) = web_sys::window() {
            window.clear_timeout_with_handle(self.timer);
        }
        self.controller.abort();
    }
}

pub async fn request(
    provider: Provider,
    url: &str,
    key: Option<&str>,
    body: Option<&str>,
    timeout: i32,
) -> Result<Value, String> {
    let deadline = Deadline::new(timeout)?;
    let options = web_sys::RequestInit::new();
    options.set_method(if body.is_some() { "POST" } else { "GET" });
    options.set_signal(Some(&deadline.controller.signal()));
    if let Some(body) = body {
        options.set_body(&wasm_bindgen::JsValue::from_str(body));
    }
    let request = web_sys::Request::new_with_str_and_init(url, &options)
        .map_err(|_| "Cannot create provider request")?;
    if let Some(key) = key {
        request
            .headers()
            .set("Authorization", &format!("Bearer {}", key.trim()))
            .map_err(|_| "Invalid API key")?;
    }
    if body.is_some() {
        request
            .headers()
            .set("Content-Type", "application/json")
            .map_err(|_| "Cannot set request type")?;
    }
    let window = web_sys::window().ok_or("Browser window unavailable")?;
    let response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|_| {
            format!(
                "{} request failed or timed out; check your connection and browser access",
                provider.label()
            )
        })?
        .dyn_into::<web_sys::Response>()
        .map_err(|_| "Invalid provider response")?;
    if !response.ok() {
        return Err(http_error(provider, response.status()));
    }
    let text = JsFuture::from(
        response
            .text()
            .map_err(|_| "Cannot read provider response")?,
    )
    .await
    .map_err(|_| "Provider response could not be read")?
    .as_string()
    .ok_or("Invalid provider response")?;
    serde_json::from_str(&text).map_err(|_| "Provider returned invalid JSON".into())
}

pub async fn pause(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
        }
    });
    let _ = JsFuture::from(promise).await;
}
