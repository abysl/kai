use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn kai_autoplay(json: String) -> Result<(), JsValue> {
    crate::autoplay::install(&json, false).map_err(|reason| JsValue::from_str(&reason))
}

#[wasm_bindgen]
pub fn kai_events() -> js_sys::Array {
    crate::autoplay::drain_events()
        .into_iter()
        .map(|line| JsValue::from(js_sys::JsString::from(line)))
        .collect()
}

#[wasm_bindgen]
pub fn kai_node_id() -> Option<String> {
    crate::autoplay::node_id()
}

#[wasm_bindgen]
pub fn kai_host_block(enforced: bool) -> Option<String> {
    crate::net::host_block(&crate::net::TableChoice {
        game: crate::net::TableGame::Riftbound,
        enforced,
        options: None,
    })
}
