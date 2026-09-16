use std::cell::RefCell;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

thread_local! {
    static WANTED: RefCell<bool> = const { RefCell::new(false) };
    static SENTINEL: RefCell<Option<JsValue>> = const { RefCell::new(None) };
}

pub fn show_panics() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let text = format!("kai stopped: {info}");
        web_sys::console::error_1(&JsValue::from_str(&text));
        if let Some(document) = web_sys::window().and_then(|window| window.document()) {
            if let Ok(banner) = document.create_element("pre") {
                banner.set_id("panic");
                banner.set_text_content(Some(&text));
                let _ = banner.set_attribute(
                    "style",
                    "position:fixed;left:0;right:0;top:0;z-index:99;margin:0;padding:12px;background:#3a0d0d;color:#ffd7d7;font:13px monospace;white-space:pre-wrap",
                );
                if let Some(body) = document.body() {
                    let _ = body.append_child(&banner);
                }
            }
        }
        previous(info);
    }));
}

pub fn boot() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let unload = Closure::<dyn Fn()>::new(crate::net::withdraw_table);
    let _ = window.add_event_listener_with_callback("pagehide", unload.as_ref().unchecked_ref());
    unload.forget();
    let Some(document) = window.document() else {
        return;
    };
    let shown = Closure::<dyn Fn()>::new(|| {
        if wanted() && document_visible() {
            request();
        }
    });
    let _ = document
        .add_event_listener_with_callback("visibilitychange", shown.as_ref().unchecked_ref());
    shown.forget();
}

pub fn keep_awake(on: bool) {
    WANTED.with(|slot| *slot.borrow_mut() = on);
    if on {
        request();
    } else {
        release();
    }
}

fn wanted() -> bool {
    WANTED.with(|slot| *slot.borrow())
}

fn document_visible() -> bool {
    web_sys::window()
        .and_then(|window| window.document())
        .is_some_and(|document| document.visibility_state() == web_sys::VisibilityState::Visible)
}

fn held() -> bool {
    SENTINEL.with(|slot| {
        slot.borrow().as_ref().is_some_and(|sentinel| {
            js_sys::Reflect::get(sentinel, &"released".into())
                .ok()
                .and_then(|released| released.as_bool())
                != Some(true)
        })
    })
}

fn request() {
    if held() {
        return;
    }
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(wake_lock) = js_sys::Reflect::get(&window.navigator(), &"wakeLock".into())
        .ok()
        .filter(|value| !value.is_undefined() && !value.is_null())
    else {
        return;
    };
    let Ok(request) = js_sys::Reflect::get(&wake_lock, &"request".into())
        .and_then(|value| value.dyn_into::<js_sys::Function>())
    else {
        return;
    };
    let Ok(promise) = request
        .call1(&wake_lock, &"screen".into())
        .and_then(|value| value.dyn_into::<js_sys::Promise>())
    else {
        return;
    };
    wasm_bindgen_futures::spawn_local(async move {
        match JsFuture::from(promise).await {
            Ok(sentinel) => {
                if wanted() {
                    SENTINEL.with(|slot| *slot.borrow_mut() = Some(sentinel));
                } else {
                    release_sentinel(&sentinel);
                }
            }
            Err(error) => bevy::log::warn!("screen wake lock refused: {error:?}"),
        }
    });
}

fn release() {
    if let Some(sentinel) = SENTINEL.with(|slot| slot.borrow_mut().take()) {
        release_sentinel(&sentinel);
    }
}

fn release_sentinel(sentinel: &JsValue) {
    if let Ok(release) = js_sys::Reflect::get(sentinel, &"release".into())
        .and_then(|value| value.dyn_into::<js_sys::Function>())
    {
        let _ = release.call0(sentinel);
    }
}
