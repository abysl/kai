use crate::table::{SessionInfo, SessionRole};
use bevy::log::tracing::field::{Field, Visit};
use bevy::log::tracing::{Event, Level, Subscriber};
use bevy::log::tracing_subscriber::layer::{Context, Layer};
use bevy::log::BoxedLayer;
use bevy::prelude::*;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use web_time::{SystemTime, UNIX_EPOCH};

use bevy_egui::egui;

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
const SYSTEM_CONFIG: &str = "/etc/kai/telemetry.toml";
const QUEUE_CAP: usize = 2048;
const RECENT_CAP: usize = 400;
const BATCH_MAX: usize = 500;
const FLUSH_SECS: u64 = 5;

#[cfg(target_os = "android")]
const PLATFORM: &str = "android";
#[cfg(target_arch = "wasm32")]
const PLATFORM: &str = "wasm";
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
const PLATFORM: &str = "desktop";

struct Entry {
    ts: u128,
    level: &'static str,
    target: String,
    message: String,
}

static QUEUE: Mutex<VecDeque<Entry>> = Mutex::new(VecDeque::new());
static RECENT: Mutex<VecDeque<Line>> = Mutex::new(VecDeque::new());
static DROPPED: AtomicU64 = AtomicU64::new(0);
static SHIPPING: Mutex<Option<Shipping>> = Mutex::new(None);

#[derive(Clone)]
struct Shipping {
    url: String,
    token: String,
    client_id: String,
}

#[derive(Clone)]
pub struct Line {
    pub clock: String,
    pub level: &'static str,
    pub target: String,
    pub message: String,
}

fn clock(ts: u128) -> String {
    let secs = (ts / 1_000_000_000) as u64 % 86_400;
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

fn remember(entry: &Entry) {
    let mut recent = RECENT.lock();
    if recent.len() >= RECENT_CAP {
        recent.pop_front();
    }
    recent.push_back(Line {
        clock: clock(entry.ts),
        level: entry.level,
        target: entry.target.clone(),
        message: entry.message.clone(),
    });
}

pub fn recent_lines() -> Vec<Line> {
    RECENT.lock().iter().cloned().collect()
}

fn now_ns() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

fn enqueue(entry: Entry) {
    let mut queue = QUEUE.lock();
    if queue.len() >= QUEUE_CAP {
        queue.pop_front();
        DROPPED.fetch_add(1, Ordering::Relaxed);
    }
    queue.push_back(entry);
}

fn drain() -> Vec<Entry> {
    let mut queue = QUEUE.lock();
    let n = queue.len().min(BATCH_MAX);
    let mut out: Vec<Entry> = queue.drain(..n).collect();
    drop(queue);
    let dropped = DROPPED.swap(0, Ordering::Relaxed);
    if dropped > 0 {
        out.push(Entry {
            ts: now_ns(),
            level: "warning",
            target: "kai::telemetry".into(),
            message: format!("dropped {dropped} buffered log entries"),
        });
    }
    out
}

struct FieldGrab {
    message: String,
    extras: String,
}

impl Visit for FieldGrab {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _ = write!(self.message, "{value:?}");
        } else {
            let _ = write!(self.extras, " {}={:?}", field.name(), value);
        }
    }
}

struct CaptureLayer;

impl<S: Subscriber> Layer<S> for CaptureLayer {
    fn on_event(&self, event: &Event, _ctx: Context<S>) {
        let meta = event.metadata();
        let level = match *meta.level() {
            Level::ERROR => "err",
            Level::WARN => "warning",
            Level::INFO => {
                let target = meta.target();
                if target.starts_with("kai")
                    || target.starts_with("spirit_node")
                    || target.starts_with("iroh")
                {
                    "info"
                } else {
                    return;
                }
            }
            _ => return,
        };
        let mut grab = FieldGrab {
            message: String::new(),
            extras: String::new(),
        };
        event.record(&mut grab);
        let entry = Entry {
            ts: now_ns(),
            level,
            target: meta.target().to_string(),
            message: format!("{}{}", grab.message, grab.extras),
        };
        remember(&entry);
        enqueue(entry);
    }
}

pub fn layer(_app: &mut App) -> Option<BoxedLayer> {
    Some(Box::new(CaptureLayer))
}

fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "unknown".into()
    } else {
        trimmed
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_kv(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        if k.trim() != key {
            return None;
        }
        let value = v.trim().trim_matches('"').to_string();
        if value.is_empty() {
            None
        } else {
            Some(value)
        }
    })
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
fn config_file() -> std::path::PathBuf {
    crate::os::paths::config_dir().join("telemetry.toml")
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
fn load() -> Option<Shipping> {
    let file = std::fs::read_to_string(config_file()).unwrap_or_default();
    let system = std::fs::read_to_string(SYSTEM_CONFIG).unwrap_or_default();
    let token = std::env::var("KAI_INGEST_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
        .or_else(|| parse_kv(&file, "token"))
        .or_else(|| parse_kv(&system, "token"))?;
    let url = std::env::var("KAI_INGEST_URL")
        .ok()
        .filter(|u| !u.is_empty())
        .or_else(|| parse_kv(&file, "url"))
        .or_else(|| parse_kv(&system, "url"))?;
    let client_id = std::env::var("KAI_CLIENT_ID")
        .ok()
        .filter(|c| !c.is_empty())
        .or_else(|| parse_kv(&file, "client_id"))
        .or_else(|| parse_kv(&system, "client_id"))
        .unwrap_or_else(|| gethostname::gethostname().to_string_lossy().into_owned());
    Some(Shipping {
        url,
        token,
        client_id: sanitize(&client_id),
    })
}

#[cfg(target_os = "android")]
fn config_file() -> Option<std::path::PathBuf> {
    bevy::android::ANDROID_APP
        .get()
        .and_then(|app| app.internal_data_path())
        .map(|dir| dir.join("telemetry.toml"))
}

#[cfg(target_os = "android")]
fn baked_defaults() -> String {
    use std::io::Read;
    let Some(app) = bevy::android::ANDROID_APP.get() else {
        return String::new();
    };
    let Ok(name) = std::ffi::CString::new("telemetry-default.toml") else {
        return String::new();
    };
    let Some(mut asset) = app.asset_manager().open(&name) else {
        return String::new();
    };
    let mut contents = String::new();
    if asset.read_to_string(&mut contents).is_err() {
        return String::new();
    }
    contents
}

#[cfg(target_os = "android")]
fn load() -> Option<Shipping> {
    let file = std::fs::read_to_string(config_file()?).unwrap_or_default();
    let baked = baked_defaults();
    let token = parse_kv(&file, "token").or_else(|| parse_kv(&baked, "token"))?;
    let url = parse_kv(&file, "url").or_else(|| parse_kv(&baked, "url"))?;
    let client_id = parse_kv(&file, "client_id")
        .or_else(crate::os::android::device_model)
        .unwrap_or_else(|| "android".into());
    Some(Shipping {
        url,
        token,
        client_id: sanitize(&client_id),
    })
}

#[cfg(target_os = "android")]
fn save_token(token: &str) -> Result<(), String> {
    let path = config_file().ok_or("no app data path")?;
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let mut out = format!("token = \"{token}\"\n");
    for key in ["url", "client_id"] {
        if let Some(value) = parse_kv(&existing, key) {
            let _ = writeln!(out, "{key} = \"{value}\"");
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(&path, out).map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

#[cfg(target_arch = "wasm32")]
fn stored(key: &str) -> Option<String> {
    local_storage()?
        .get_item(key)
        .ok()?
        .filter(|value| !value.is_empty())
}

#[cfg(target_arch = "wasm32")]
static ORIGIN_DEFAULTS: Mutex<Option<(String, String)>> = Mutex::new(None);

#[cfg(target_arch = "wasm32")]
async fn fetch_origin_defaults() -> Option<(String, String)> {
    use wasm_bindgen::JsCast;
    let window = web_sys::window()?;
    let response = wasm_bindgen_futures::JsFuture::from(window.fetch_with_str("/telemetry.json"))
        .await
        .ok()?;
    let response: web_sys::Response = response.dyn_into().ok()?;
    if !response.ok() {
        return None;
    }
    let text = wasm_bindgen_futures::JsFuture::from(response.text().ok()?)
        .await
        .ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&text.as_string()?).ok()?;
    let token = parsed
        .get("token")?
        .as_str()
        .filter(|t| !t.is_empty())?
        .to_string();
    let url = parsed
        .get("url")
        .and_then(|u| u.as_str())
        .filter(|u| !u.is_empty())?
        .to_string();
    Some((token, url))
}

#[cfg(target_arch = "wasm32")]
fn spawn_defaults_fetch() {
    wasm_bindgen_futures::spawn_local(async {
        let Some(defaults) = fetch_origin_defaults().await else {
            return;
        };
        *ORIGIN_DEFAULTS.lock() = Some(defaults);
        if SHIPPING.lock().is_none() {
            activate();
        }
    });
}

#[cfg(target_arch = "wasm32")]
fn load() -> Option<Shipping> {
    let defaults = ORIGIN_DEFAULTS.lock().clone();
    let token = stored("kai_telemetry_token").or_else(|| defaults.as_ref().map(|d| d.0.clone()))?;
    let url = stored("kai_telemetry_url").or_else(|| defaults.map(|d| d.1))?;
    let client_id = stored("kai_client_id").unwrap_or_else(|| {
        let id = format!("web-{:08x}", (js_sys::Math::random() * 4294967296.0) as u32);
        if let Some(storage) = local_storage() {
            let _ = storage.set_item("kai_client_id", &id);
        }
        id
    });
    Some(Shipping {
        url,
        token,
        client_id: sanitize(&client_id),
    })
}

#[cfg(target_arch = "wasm32")]
fn save_token(token: &str) -> Result<(), String> {
    local_storage()
        .ok_or("no local storage")?
        .set_item("kai_telemetry_token", token)
        .map_err(|_| "local storage write failed".to_string())
}

fn push_body(shipping: &Shipping, entries: &[Entry]) -> String {
    use serde_json::{json, Value};
    let mut streams: Vec<Value> = Vec::new();
    for level in ["err", "warning", "info", "debug"] {
        let values: Vec<Value> = entries
            .iter()
            .filter(|entry| entry.level == level)
            .map(|entry| {
                json!([
                    entry.ts.to_string(),
                    entry.message,
                    { "target": entry.target }
                ])
            })
            .collect();
        if values.is_empty() {
            continue;
        }
        streams.push(json!({
            "stream": {
                "app": "kai",
                "platform": PLATFORM,
                "client_id": shipping.client_id,
                "version": env!("CARGO_PKG_VERSION"),
                "level": level,
            },
            "values": values,
        }));
    }
    json!({ "streams": streams }).to_string()
}

#[cfg(not(target_arch = "wasm32"))]
fn start_shipper() {
    use std::sync::atomic::AtomicBool;
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| loop {
        std::thread::sleep(std::time::Duration::from_secs(FLUSH_SECS));
        let Some(shipping) = SHIPPING.lock().clone() else {
            continue;
        };
        let batch = drain();
        if batch.is_empty() {
            continue;
        }
        let body = push_body(&shipping, &batch);
        let _ = ureq::post(&shipping.url)
            .timeout(std::time::Duration::from_secs(10))
            .set("Authorization", &format!("Bearer {}", shipping.token))
            .set("Content-Type", "application/json")
            .send_string(&body);
    });
}

#[cfg(target_arch = "wasm32")]
fn flush_wasm(time: Res<Time>, mut timer: Local<Option<Timer>>) {
    let timer =
        timer.get_or_insert_with(|| Timer::from_seconds(FLUSH_SECS as f32, TimerMode::Repeating));
    if !timer.tick(time.delta()).just_finished() {
        return;
    }
    let Some(shipping) = SHIPPING.lock().clone() else {
        return;
    };
    let batch = drain();
    if batch.is_empty() {
        return;
    }
    let body = push_body(&shipping, &batch);
    wasm_bindgen_futures::spawn_local(async move {
        let _ = send_wasm(&shipping, &body).await;
    });
}

#[cfg(target_arch = "wasm32")]
async fn send_wasm(shipping: &Shipping, body: &str) -> Result<(), wasm_bindgen::JsValue> {
    let init = web_sys::RequestInit::new();
    init.set_method("POST");
    init.set_body(&wasm_bindgen::JsValue::from_str(body));
    let request = web_sys::Request::new_with_str_and_init(&shipping.url, &init)?;
    request
        .headers()
        .set("Authorization", &format!("Bearer {}", shipping.token))?;
    request.headers().set("Content-Type", "application/json")?;
    let window = web_sys::window().ok_or(wasm_bindgen::JsValue::NULL)?;
    wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request)).await?;
    Ok(())
}

fn activate() {
    let loaded = load();
    let active = loaded.is_some();
    *SHIPPING.lock() = loaded;
    if !active {
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    start_shipper();
    if let Some(shipping) = SHIPPING.lock().as_ref() {
        info!(
            target: "kai::telemetry",
            "log shipping active client_id={} platform={} version={}",
            shipping.client_id,
            PLATFORM,
            env!("CARGO_PKG_VERSION")
        );
    }
}

fn watch_session(info: Res<SessionInfo>, mut last: Local<Option<(SessionRole, String, usize)>>) {
    let current = (info.role, info.status.clone(), info.roster.len());
    if last.as_ref() == Some(&current) {
        return;
    }
    *last = Some(current);
    info!(
        target: "kai::session",
        "session role={:?} seats={} status={}",
        info.role,
        info.roster.len(),
        info.status
    );
}

#[cfg(not(target_arch = "wasm32"))]
fn watch_node(mut last: Local<String>) {
    let status = crate::net::node::status();
    if status.is_empty() || *last == status {
        return;
    }
    *last = status.clone();
    info!(target: "kai::node", "{status}");
}

#[derive(Resource, Default)]
pub struct TelemetryPanel {
    #[cfg(any(target_os = "android", target_arch = "wasm32"))]
    token_input: String,
    status: String,
    log_filter: String,
}

fn shipping_line() -> String {
    match SHIPPING.lock().as_ref() {
        Some(shipping) => format!(
            "shipping logs as {} to {}",
            shipping.client_id, shipping.url
        ),
        None => "log shipping off".into(),
    }
}

#[cfg(any(target_os = "android", target_arch = "wasm32"))]
fn token_controls(ui: &mut egui::Ui, panel: &mut TelemetryPanel) {
    if SHIPPING.lock().is_none() {
        ui.label(
            egui::RichText::new("log shipping needs a configured collector URL and token").weak(),
        );
    }
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut panel.token_input)
                .hint_text("ingest token")
                .password(true)
                .desired_width(160.0),
        );
        if let Some(status) = crate::os::clipboard::paste_button(
            ui,
            crate::os::clipboard::TELEMETRY_TOKEN,
            &mut panel.token_input,
        ) {
            panel.status = status;
        }
        let ready = !panel.token_input.trim().is_empty();
        if ui
            .add_enabled(ready, egui::Button::new("save token"))
            .clicked()
        {
            let token = panel.token_input.trim().to_string();
            panel.token_input.clear();
            panel.status = match save_token(&token) {
                Ok(()) => {
                    activate();
                    "token saved".into()
                }
                Err(error) => format!("save failed: {error}"),
            };
        }
    });
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
fn token_controls(ui: &mut egui::Ui, _panel: &mut TelemetryPanel) {
    ui.label(
        egui::RichText::new(format!(
            "set KAI_INGEST_TOKEN and KAI_INGEST_URL, or configure {} or {SYSTEM_CONFIG}",
            config_file().display()
        ))
        .weak(),
    );
}

fn level_color(level: &str) -> egui::Color32 {
    match level {
        "err" => egui::Color32::from_rgb(230, 120, 110),
        "warning" => egui::Color32::from_rgb(220, 180, 110),
        _ => egui::Color32::from_gray(200),
    }
}

pub fn telemetry_section(ui: &mut egui::Ui, panel: &mut TelemetryPanel) {
    ui.label(shipping_line());
    ui.label(
        egui::RichText::new(format!(
            "client {PLATFORM} v{} — queue {} / {QUEUE_CAP}",
            env!("CARGO_PKG_VERSION"),
            QUEUE.lock().len()
        ))
        .weak(),
    );
    token_controls(ui, panel);
    ui.separator();
    ui.label(egui::RichText::new("logs").strong());
    let lines = recent_lines();
    let filter = panel.log_filter.trim().to_lowercase();
    let shown: Vec<&Line> = lines
        .iter()
        .filter(|line| {
            filter.is_empty()
                || line.target.to_lowercase().contains(&filter)
                || line.message.to_lowercase().contains(&filter)
                || line.level.contains(&filter)
        })
        .collect();
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut panel.log_filter)
                .hint_text("filter")
                .desired_width(200.0),
        );
        ui.label(egui::RichText::new(format!("{} of {} lines", shown.len(), lines.len())).weak());
        let text: String = shown
            .iter()
            .map(|line| {
                format!(
                    "{} {:<7} {} {}\n",
                    line.clock, line.level, line.target, line.message
                )
            })
            .collect();
        if let Some(status) = crate::os::clipboard::copy_button(ui, "copy logs", &text) {
            panel.status = status;
        }
        if ui.button("clear").clicked() {
            RECENT.lock().clear();
        }
    });
    egui::ScrollArea::vertical()
        .id_salt("kai logs")
        .stick_to_bottom(true)
        .max_height(ui.available_height().max(120.0))
        .show(ui, |ui| {
            if shown.is_empty() {
                ui.label(egui::RichText::new("nothing captured yet").weak());
            }
            for line in shown {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(egui::RichText::new(&line.clock).monospace().weak());
                    ui.label(
                        egui::RichText::new(line.level)
                            .monospace()
                            .color(level_color(line.level)),
                    );
                    ui.label(egui::RichText::new(&line.target).monospace().weak());
                    ui.label(egui::RichText::new(&line.message).monospace());
                });
            }
        });
    if !panel.status.is_empty() {
        ui.label(egui::RichText::new(&panel.status).weak());
    }
}

pub struct TelemetryPlugin;

impl Plugin for TelemetryPlugin {
    fn build(&self, app: &mut App) {
        activate();
        #[cfg(target_arch = "wasm32")]
        spawn_defaults_fetch();
        app.add_systems(Update, watch_session);
        #[cfg(not(target_arch = "wasm32"))]
        app.add_systems(Update, watch_node);
        #[cfg(target_arch = "wasm32")]
        app.add_systems(Update, flush_wasm);
        app.init_resource::<TelemetryPanel>();
    }
}
