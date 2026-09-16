use bevy_egui::egui;
use parking_lot::Mutex;

pub const IMPORT_PASTE: &str = "import-paste";
pub const IDENTITY_TICKET: &str = "identity-ticket";
pub const TELEMETRY_TOKEN: &str = "telemetry-token";

static PENDING: Mutex<Option<&'static str>> = Mutex::new(None);
static RESULTS: Mutex<Vec<(&'static str, Result<String, String>)>> = Mutex::new(Vec::new());

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
mod platform {
    use std::cell::RefCell;

    thread_local! {
        static CLIPBOARD: RefCell<Option<arboard::Clipboard>> = const { RefCell::new(None) };
    }

    fn with<T>(
        action: impl FnOnce(&mut arboard::Clipboard) -> Result<T, String>,
    ) -> Result<T, String> {
        CLIPBOARD.with(|cell| {
            let mut slot = cell.borrow_mut();
            if slot.is_none() {
                *slot = Some(arboard::Clipboard::new().map_err(|error| error.to_string())?);
            }
            let clipboard = slot.as_mut().expect("clipboard just initialized");
            action(clipboard)
        })
    }

    pub fn set_text(text: &str) -> Result<(), String> {
        with(|clipboard| {
            clipboard
                .set_text(text.to_owned())
                .map_err(|error| error.to_string())
        })
    }

    pub fn read() -> Result<String, String> {
        with(|clipboard| match clipboard.get_text() {
            Ok(text) => Ok(text),
            Err(arboard::Error::ContentNotAvailable) => Ok(String::new()),
            Err(error) => Err(error.to_string()),
        })
    }
}

#[cfg(target_arch = "wasm32")]
mod platform {
    use wasm_bindgen::{JsCast, JsValue};

    const MISSING: &str = "the page clipboard glue is missing — reload the page";

    fn call(name: &str, args: &[JsValue]) -> Result<JsValue, String> {
        let window = web_sys::window().ok_or("no window object")?;
        let value = js_sys::Reflect::get(window.as_ref(), &JsValue::from_str(name))
            .map_err(|_| MISSING.to_string())?;
        let function: js_sys::Function = value.dyn_into().map_err(|_| MISSING.to_string())?;
        let arguments = js_sys::Array::new();
        for argument in args {
            arguments.push(argument);
        }
        js_sys::Reflect::apply(&function, &JsValue::NULL, &arguments)
            .map_err(|error| describe(&error))
    }

    fn describe(error: &JsValue) -> String {
        error
            .as_string()
            .or_else(|| {
                js_sys::Reflect::get(error, &JsValue::from_str("message"))
                    .ok()?
                    .as_string()
            })
            .unwrap_or_else(|| "clipboard call failed".to_string())
    }

    fn state() -> Result<JsValue, String> {
        let window = web_sys::window().ok_or("no window object")?;
        let value = js_sys::Reflect::get(window.as_ref(), &JsValue::from_str("__kaiClipboard"))
            .map_err(|_| MISSING.to_string())?;
        if value.is_undefined() || value.is_null() {
            return Err(MISSING.to_string());
        }
        Ok(value)
    }

    fn field(state: &JsValue, name: &str) -> JsValue {
        js_sys::Reflect::get(state, &JsValue::from_str(name)).unwrap_or(JsValue::UNDEFINED)
    }

    pub fn set_text(text: &str) -> Result<(), String> {
        let outcome = call("__kaiClipboardWrite", &[JsValue::from_str(text)])?;
        match outcome.as_string() {
            Some(message) if !message.is_empty() => Err(message),
            _ => Ok(()),
        }
    }

    pub fn start_read() -> Result<(), String> {
        call("__kaiClipboardRead", &[]).map(|_| ())
    }

    pub fn poll() -> Option<Result<String, String>> {
        let state = state().ok()?;
        if !field(&state, "ready").as_bool().unwrap_or(false) {
            return None;
        }
        let _ = js_sys::Reflect::set(&state, &JsValue::from_str("ready"), &JsValue::FALSE);
        let error = field(&state, "error").as_string().unwrap_or_default();
        if !error.is_empty() {
            return Some(Err(error));
        }
        Some(Ok(field(&state, "text").as_string().unwrap_or_default()))
    }
}

#[cfg(target_os = "android")]
mod platform {
    use crate::os::android::with_activity;
    use jni::objects::JValue;

    pub fn set_text(text: &str) -> Result<(), String> {
        with_activity(|env, activity| {
            let value = env.new_string(text)?;
            env.call_method(
                activity,
                "setClipboardText",
                "(Ljava/lang/String;)V",
                &[JValue::Object(&value)],
            )?;
            Ok(())
        })
    }

    pub fn start_read() -> Result<(), String> {
        with_activity(|env, activity| {
            env.call_method(activity, "requestClipboardPaste", "()V", &[])?;
            Ok(())
        })
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_blue_rae_kai_MainActivity_nativeClipboard(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    text: jni::objects::JString,
    failure: jni::objects::JString,
) {
    let failure = env
        .get_string(&failure)
        .map(String::from)
        .unwrap_or_default();
    if !failure.is_empty() {
        deliver(Err(failure));
        return;
    }
    let result = match env.get_string(&text) {
        Ok(text) => Ok(String::from(text)),
        Err(error) => Err(error.to_string()),
    };
    deliver(result);
}

fn deliver(result: Result<String, String>) {
    let Some(slot) = PENDING.lock().take() else {
        return;
    };
    let mut results = RESULTS.lock();
    results.retain(|(name, _)| *name != slot);
    results.push((slot, result));
}

pub fn set_text(text: &str) -> Result<(), String> {
    platform::set_text(text)
}

pub fn request_paste(slot: &'static str) {
    *PENDING.lock() = Some(slot);
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
    deliver(platform::read());
    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    if let Err(error) = platform::start_read() {
        deliver(Err(error));
    }
}

pub fn take_paste(slot: &'static str) -> Option<Result<String, String>> {
    #[cfg(target_arch = "wasm32")]
    if let Some(result) = platform::poll() {
        deliver(result);
    }
    let mut results = RESULTS.lock();
    let index = results.iter().position(|(name, _)| *name == slot)?;
    Some(results.remove(index).1)
}

pub const COPIED: &str = "copied to clipboard";

pub fn copy_status(text: &str) -> String {
    match set_text(text) {
        Ok(()) => COPIED.to_string(),
        Err(error) => format!("copy failed: {error}"),
    }
}

pub fn copy_button(ui: &mut egui::Ui, label: &str, text: &str) -> Option<String> {
    if !ui.small_button(label).clicked() {
        return None;
    }
    Some(copy_status(text))
}

pub fn paste_button(ui: &mut egui::Ui, slot: &'static str, target: &mut String) -> Option<String> {
    let mut status = match take_paste(slot) {
        Some(Ok(text)) if text.trim().is_empty() => Some("clipboard is empty".to_string()),
        Some(Ok(text)) => {
            *target = text;
            Some("pasted from clipboard".to_string())
        }
        Some(Err(error)) => Some(format!("paste failed: {error}")),
        None => None,
    };
    if ui.button("paste").clicked() {
        request_paste(slot);
        status = None;
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_paste_lands_only_in_the_slot_that_asked_for_it() {
        *PENDING.lock() = Some(IMPORT_PASTE);
        deliver(Ok("deck list".to_string()));
        assert!(take_paste(IDENTITY_TICKET).is_none());
        assert_eq!(take_paste(IMPORT_PASTE), Some(Ok("deck list".to_string())));
        assert!(take_paste(IMPORT_PASTE).is_none());

        deliver(Ok("stray".to_string()));
        assert!(take_paste(IMPORT_PASTE).is_none());

        *PENDING.lock() = Some(IDENTITY_TICKET);
        deliver(Err("refused".to_string()));
        assert_eq!(
            take_paste(IDENTITY_TICKET),
            Some(Err("refused".to_string()))
        );
    }
}
