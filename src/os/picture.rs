use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

pub const MAX_INPUT: usize = 16 << 20;
pub type Selection = Result<Vec<u8>, String>;
static RESULT: Mutex<Option<Selection>> = Mutex::new(None);
static PICKING: AtomicBool = AtomicBool::new(false);

pub fn take() -> Option<Selection> {
    RESULT.lock().take()
}

pub fn finish(result: Option<Selection>) {
    *RESULT.lock() = result;
    PICKING.store(false, Ordering::Relaxed);
}

pub fn choose() {
    if !PICKING.swap(true, Ordering::Relaxed) {
        choose_platform();
    }
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
fn choose_platform() {
    std::thread::spawn(|| {
        use std::io::Read;
        let Some(path) = rfd::FileDialog::new()
            .set_title("Choose your playmat picture")
            .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
            .pick_file()
        else {
            finish(None);
            return;
        };
        let result = (|| {
            let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
            let mut bytes = Vec::new();
            file.take((MAX_INPUT + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() > MAX_INPUT {
                return Err("Choose a picture smaller than 16 MiB".into());
            }
            Ok(bytes)
        })();
        finish(Some(result));
    });
}

#[cfg(target_os = "android")]
fn choose_platform() {
    if let Err(error) = super::android::with_activity(|env, activity| {
        env.call_method(activity, "choosePlaymatPicture", "()V", &[])?;
        Ok(())
    }) {
        finish(Some(Err(error)));
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_blue_rae_kai_MainActivity_nativePlaymatPicture(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    bytes: jni::objects::JByteArray,
    error: jni::objects::JString,
) {
    let message: String = env.get_string(&error).map(Into::into).unwrap_or_default();
    if !message.is_empty() {
        finish(Some(Err(message)));
    } else if bytes.is_null() {
        finish(None);
    } else {
        finish(Some(
            env.convert_byte_array(bytes).map_err(|e| e.to_string()),
        ));
    }
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static PICKER: std::cell::RefCell<Option<(web_sys::HtmlInputElement, wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>)>> = const { std::cell::RefCell::new(None) };
}

#[cfg(target_arch = "wasm32")]
fn choose_platform() {
    use wasm_bindgen::{closure::Closure, JsCast};
    let result = (|| {
        let document = web_sys::window()
            .and_then(|w| w.document())
            .ok_or("File picker unavailable")?;
        let input = document
            .create_element("input")
            .map_err(|_| "File picker unavailable")?
            .dyn_into::<web_sys::HtmlInputElement>()
            .map_err(|_| "File picker unavailable")?;
        input.set_type("file");
        input.set_accept("image/png,image/jpeg,image/webp");
        input
            .set_attribute("style", "display:none")
            .map_err(|_| "File picker unavailable")?;
        let callback = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
            let Some(input) = event
                .target()
                .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
            else {
                finish(None);
                return;
            };
            let file = input.files().and_then(|files| files.get(0));
            input.remove();
            let Some(file) = file else {
                finish(None);
                return;
            };
            if file.size() > MAX_INPUT as f64 {
                finish(Some(Err("Choose a picture smaller than 16 MiB".into())));
                return;
            }
            wasm_bindgen_futures::spawn_local(async move {
                let result = wasm_bindgen_futures::JsFuture::from(file.array_buffer())
                    .await
                    .map(|buffer| js_sys::Uint8Array::new(&buffer).to_vec())
                    .map_err(|_| "Could not read that picture".to_string());
                finish(Some(result));
            });
        });
        input
            .add_event_listener_with_callback("change", callback.as_ref().unchecked_ref())
            .map_err(|_| "File picker unavailable")?;
        input
            .add_event_listener_with_callback("cancel", callback.as_ref().unchecked_ref())
            .map_err(|_| "File picker unavailable")?;
        document
            .body()
            .ok_or("File picker unavailable")?
            .append_child(&input)
            .map_err(|_| "File picker unavailable")?;
        PICKER.with(|held| {
            if let Some((previous, _)) = held.borrow_mut().take() {
                previous.remove();
            }
            input.click();
            *held.borrow_mut() = Some((input, callback));
        });
        Ok::<_, &str>(())
    })();
    if let Err(error) = result {
        finish(Some(Err(error.into())));
    }
}
