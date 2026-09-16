use crate::net::identity::{short_id, IdentityPanel};
use crate::net::node;
use bevy::android::ANDROID_APP;
use bevy::prelude::*;
use jni::objects::{JClass, JObject, JString, JValue};
use jni::sys::jobject;
use jni::{JNIEnv, JavaVM};
use parking_lot::Mutex;
use std::ffi::{c_char, c_int, CString};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub const AUTOPLAY_LOG_TAG: &str = "kai.autoplay";
const AUTOPLAY_LOG_POLL: Duration = Duration::from_millis(100);
const ANDROID_LOG_INFO: c_int = 4;
const ANDROID_LOG_ERROR: c_int = 6;

#[link(name = "log")]
extern "C" {
    fn __android_log_write(priority: c_int, tag: *const c_char, text: *const c_char) -> c_int;
}

static TICKETS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static INSETS: Mutex<Option<[f32; 5]>> = Mutex::new(None);
static AUTOPLAY_LOGGER: AtomicBool = AtomicBool::new(false);

fn logcat(priority: c_int, text: &str) {
    let Ok(tag) = CString::new(AUTOPLAY_LOG_TAG) else {
        return;
    };
    let Ok(text) = CString::new(text.replace('\0', "")) else {
        return;
    };
    unsafe {
        __android_log_write(priority, tag.as_ptr(), text.as_ptr());
    }
}

fn start_autoplay_logger() {
    if AUTOPLAY_LOGGER.swap(true, Ordering::Relaxed) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("kai-autoplay-log".into())
        .spawn(|| loop {
            for line in crate::autoplay::drain_events() {
                logcat(
                    ANDROID_LOG_INFO,
                    &format!("{}{line}", crate::autoplay::EVENT_PREFIX),
                );
            }
            std::thread::sleep(AUTOPLAY_LOG_POLL);
        });
    if let Err(error) = spawned {
        AUTOPLAY_LOGGER.store(false, Ordering::Relaxed);
        logcat(
            ANDROID_LOG_ERROR,
            &format!("autoplay logger thread failed: {error}"),
        );
    }
}

#[no_mangle]
pub extern "system" fn Java_blue_rae_kai_MainActivity_nativeAutoplay(
    mut env: JNIEnv,
    _class: JClass,
    plan: JString,
) {
    let plan: String = match env.get_string(&plan) {
        Ok(plan) => plan.into(),
        Err(error) => {
            logcat(
                ANDROID_LOG_ERROR,
                &format!("autoplay plan unreadable: {error}"),
            );
            return;
        }
    };
    start_autoplay_logger();
    if let Err(reason) = crate::autoplay::install(&plan, false) {
        logcat(ANDROID_LOG_ERROR, &format!("autoplay refused: {reason}"));
    }
}

#[no_mangle]
pub extern "system" fn Java_blue_rae_kai_MainActivity_nativeInsets(
    _env: JNIEnv,
    _class: JClass,
    top: jni::sys::jfloat,
    right: jni::sys::jfloat,
    bottom: jni::sys::jfloat,
    left: jni::sys::jfloat,
    ime: jni::sys::jfloat,
) {
    *INSETS.lock() = Some([top, right, bottom, left, ime]);
}

pub fn take_insets() -> Option<[f32; 5]> {
    INSETS.lock().take()
}

#[no_mangle]
pub extern "system" fn Java_blue_rae_kai_MainActivity_nativeTicket(
    mut env: JNIEnv,
    _class: JClass,
    ticket: JString,
) {
    if let Ok(ticket) = env.get_string(&ticket) {
        TICKETS.lock().push(ticket.into());
    }
}

pub fn store_dir() -> Option<PathBuf> {
    ANDROID_APP
        .get()
        .and_then(|app| app.internal_data_path())
        .map(|path| path.join("spirit-store"))
}

pub fn config_dir() -> PathBuf {
    ANDROID_APP
        .get()
        .and_then(|app| app.internal_data_path())
        .unwrap_or_default()
        .join("kai")
}

pub fn with_activity<T>(
    action: impl FnOnce(&mut JNIEnv, &JObject) -> Result<T, jni::errors::Error>,
) -> Result<T, String> {
    let app = ANDROID_APP.get().ok_or("android app not initialized")?;
    let vm =
        unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }.map_err(|error| error.to_string())?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr() as jobject) };
    let mut env = vm
        .attach_current_thread()
        .map_err(|error| error.to_string())?;
    action(&mut env, &activity).map_err(|error| error.to_string())
}

pub fn request_scan() -> Result<(), String> {
    with_activity(|env, activity| {
        env.call_method(activity, "startSpiritScan", "()V", &[])?;
        Ok(())
    })
}

pub fn set_keep_awake(on: bool) -> Result<(), String> {
    with_activity(|env, activity| {
        env.call_method(
            activity,
            "setKeepAwake",
            "(Z)V",
            &[JValue::Bool(u8::from(on))],
        )?;
        Ok(())
    })
}

pub fn device_model() -> Option<String> {
    with_activity(|env, _| {
        let model = env
            .get_static_field("android/os/Build", "MODEL", "Ljava/lang/String;")?
            .l()?;
        let model: String = env.get_string(&JString::from(model))?.into();
        Ok(model)
    })
    .ok()
}

pub fn drain_tickets(mut panel: ResMut<IdentityPanel>) {
    let tickets: Vec<String> = std::mem::take(&mut *TICKETS.lock());
    for ticket in tickets {
        panel.status = match node::add_peer(&ticket) {
            Ok(id) => format!("added peer {}", short_id(&id)),
            Err(error) => format!("add peer failed: {error}"),
        };
    }
}
