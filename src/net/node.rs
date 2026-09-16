use agni_net::table::TableProtocol;
use parking_lot::Mutex;
use spirit_node::iroh::Endpoint;
use spirit_node::mesh::Mesh;
use std::sync::Arc;

static NODE: Mutex<Option<Node>> = Mutex::new(None);
static STATUS: Mutex<String> = Mutex::new(String::new());

#[derive(Clone)]
pub struct Node {
    pub node_id: String,
    pub ticket: String,
    pub mesh: Arc<Mesh>,
    pub table: TableProtocol,
    pub matchmaking: agni_net::matchmaking::Matchmaker,
    pub endpoint: Endpoint,
    #[cfg(not(target_arch = "wasm32"))]
    runtime: tokio::runtime::Handle,
    pub blobs: spirit_node::iroh_blobs::api::Store,
}

impl Node {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn block_on<F: std::future::Future>(&self, future: F) -> F::Output {
        self.runtime.block_on(future)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.runtime.spawn(future);
    }

    #[cfg(target_arch = "wasm32")]
    pub fn spawn<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + 'static,
    {
        wasm_bindgen_futures::spawn_local(future);
    }
}

fn set_status(status: String) {
    *STATUS.lock() = status;
}

pub fn status() -> String {
    STATUS.lock().clone()
}

pub fn get() -> Option<Node> {
    NODE.lock().clone()
}

pub fn add_peer(ticket: &str) -> Result<String, String> {
    let node = get().ok_or("node not ready yet")?;
    node.mesh.seed(ticket).map_err(|error| error.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn start() {
    if get().is_some() {
        return;
    }
    let Some(dir) = crate::os::paths::store_dir() else {
        set_status("no store path available".into());
        return;
    };
    set_status("starting spirit node…".into());
    std::thread::spawn(move || run(dir));
}

#[cfg(not(target_arch = "wasm32"))]
fn run(dir: std::path::PathBuf) {
    crate::engine::modules::seed_bundled(&dir);
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            set_status(format!("spirit node failed: {error}"));
            return;
        }
    };
    runtime.block_on(async move {
        let table = TableProtocol::new();
        let accepted = table.clone();
        let matchmaking = agni_net::matchmaking::Matchmaker::default();
        let matches = matchmaking.clone();
        match spirit_node::serve_with(&dir, move |router| {
            router.accept(agni_net::table::ALPN, accepted)
                .accept(agni_net::matchmaking::ALPN, matches)
        })
        .await
        {
            Ok(serving) => {
                let node = Node {
                    node_id: serving.node_id.clone(),
                    ticket: serving.ticket.clone(),
                    mesh: serving.mesh.clone(),
                    table,
                    matchmaking,
                    endpoint: serving.endpoint.clone(),
                    runtime: tokio::runtime::Handle::current(),
                    blobs: serving.blobs.clone(),
                };
                *NODE.lock() = Some(node);
                serving.mesh.set_fetch(std::sync::Arc::new(fetch_status));
                match crate::render::art::assets::publish_index(&dir) {
                    Ok(count) => bevy::log::info!(target: "kai::node", "assets index: {count} entries"),
                    Err(error) => bevy::log::warn!(target: "kai::node", "assets index not published: {error}"),
                }
                let mut seeded = Vec::new();
                for (label, forms) in crate::net::defaults::seeds() {
                    let mesh = serving.mesh.clone();
                    let outcome = tokio::task::spawn_blocking(move || seed_any(&mesh, &forms)).await;
                    match outcome {
                        Ok(Ok(form)) => seeded.push(format!("{label} ({form})")),
                        Ok(Err(error)) => {
                            bevy::log::warn!(target: "kai::node", "default peer {label} rejected: {error}")
                        }
                        Err(error) => {
                            bevy::log::warn!(target: "kai::node", "default peer {label} seeding panicked: {error}")
                        }
                    }
                }
                if seeded.is_empty() {
                    set_status(format!("serving {} blobs", serving.imported));
                } else {
                    set_status(format!(
                        "serving {} blobs, default peers {}",
                        serving.imported,
                        seeded.join(", ")
                    ));
                }
                std::future::pending::<()>().await;
            }
            Err(error) => set_status(format!("spirit node failed: {error}")),
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn fetch_status(url: &str) -> Result<String, String> {
    let agent = agni_importers::art::art_agent();
    let response = agent.get(url).call().map_err(|error| error.to_string())?;
    response
        .into_string()
        .map_err(|error| format!("{url}: {error}"))
}

#[cfg(not(target_arch = "wasm32"))]
fn seed_any(mesh: &spirit_node::mesh::Mesh, forms: &[String]) -> Result<&'static str, String> {
    let mut last = String::from("no seed form given");
    for form in forms {
        match mesh.seed(form) {
            Ok(_) => return Ok("by node id"),
            Err(error) => last = format!("{form}: {error}"),
        }
    }
    Err(last)
}

#[cfg(target_arch = "wasm32")]
static PENDING_SEEDS: Mutex<Vec<String>> = Mutex::new(Vec::new());

#[cfg(target_arch = "wasm32")]
static PENDING_BLOBS: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

#[cfg(target_arch = "wasm32")]
pub fn serve_bytes(bytes: Vec<u8>) {
    match get() {
        Some(node) => node.spawn(add_blob(node.blobs.clone(), bytes)),
        None => PENDING_BLOBS.lock().push(bytes),
    }
}

#[cfg(target_arch = "wasm32")]
async fn add_blob(blobs: spirit_node::iroh_blobs::api::Store, bytes: Vec<u8>) {
    let hash = spirit_node::spirit_core::BlobHash::of(&bytes);
    match blobs.add_bytes(bytes).await {
        Ok(_) => bevy::log::info!("serving {hash} from the browser's store"),
        Err(error) => bevy::log::warn!("{hash} not served: {error}"),
    }
}

#[cfg(target_arch = "wasm32")]
pub fn seed_when_ready(id: String) {
    match get() {
        Some(node) => {
            let _ = node.mesh.seed(&id);
        }
        None => PENDING_SEEDS.lock().push(id),
    }
}

#[cfg(target_arch = "wasm32")]
pub fn start() {
    if get().is_some() {
        return;
    }
    set_status("joining the mesh via relay…".into());
    wasm_bindgen_futures::spawn_local(async {
        let secret = match browser_secret() {
            Ok(secret) => secret,
            Err(error) => {
                set_status(format!("no browser identity: {error}"));
                return;
            }
        };
        let seeds: Vec<String> = crate::net::defaults::seeds()
            .into_iter()
            .filter_map(|(_, forms)| forms.into_iter().find(|form| !form.starts_with("http")))
            .collect();
        let table = TableProtocol::new();
        let accepted = table.clone();
        let matchmaking = agni_net::matchmaking::Matchmaker::default();
        let matches = matchmaking.clone();
        match spirit_node::serve_in_memory_with(secret, &seeds, move |router| {
            router
                .accept(agni_net::table::ALPN, accepted)
                .accept(agni_net::matchmaking::ALPN, matches)
        })
        .await
        {
            Ok(serving) => {
                let node = Node {
                    node_id: serving.node_id.clone(),
                    ticket: serving.ticket.clone(),
                    mesh: serving.mesh.clone(),
                    table,
                    matchmaking,
                    endpoint: serving.endpoint.clone(),
                    blobs: serving.blobs.clone(),
                };
                *NODE.lock() = Some(node);
                for seed in std::mem::take(&mut *PENDING_SEEDS.lock()) {
                    let _ = serving.mesh.seed(&seed);
                }
                for bytes in std::mem::take(&mut *PENDING_BLOBS.lock()) {
                    wasm_bindgen_futures::spawn_local(add_blob(serving.blobs.clone(), bytes));
                }
                set_status("meshed via relay".into());
                bevy::log::info!("meshed via relay as {}", serving.node_id);
                let _serving = serving;
                std::future::pending::<()>().await;
            }
            Err(error) => {
                bevy::log::warn!("mesh join failed: {error}");
                set_status(format!("mesh join failed: {error}"));
            }
        }
    });
}

#[cfg(target_arch = "wasm32")]
fn browser_secret() -> Result<spirit_node::iroh::SecretKey, String> {
    let window = web_sys::window().ok_or("no window")?;
    let storage = window
        .local_storage()
        .map_err(|_| "localStorage unavailable")?
        .ok_or("localStorage disabled")?;
    let tab = window
        .session_storage()
        .map_err(|_| "sessionStorage unavailable")?
        .ok_or("sessionStorage disabled")?;
    let slot = tab.get_item("kai-key-slot").ok().flatten();
    if key_slot_is_per_tab(slot.as_deref()) {
        set_status("another tab holds this browser's identity — this tab gets its own".into());
        return Ok(stored_or_fresh_key(&tab, "kai-node-key-tab"));
    }
    Ok(stored_or_fresh_key(&storage, "kai-node-key"))
}

pub fn key_slot_is_per_tab(slot: Option<&str>) -> bool {
    slot == Some("tab")
}

#[cfg(target_arch = "wasm32")]
fn stored_or_fresh_key(storage: &web_sys::Storage, slot: &str) -> spirit_node::iroh::SecretKey {
    if let Ok(Some(stored)) = storage.get_item(slot) {
        if let Ok(key) = stored.trim().parse::<spirit_node::iroh::SecretKey>() {
            return key;
        }
    }
    let key = spirit_node::iroh::SecretKey::generate();
    let hex = agni_sim::pins::hash_hex(&key.to_bytes());
    let _ = storage.set_item(slot, &hex);
    key
}

#[cfg(test)]
mod key_slot_tests {
    use super::key_slot_is_per_tab;

    #[test]
    fn only_the_pages_tab_verdict_sends_this_tab_to_its_own_key() {
        assert!(key_slot_is_per_tab(Some("tab")));
        assert!(!key_slot_is_per_tab(Some("browser")));
        assert!(!key_slot_is_per_tab(None));
    }
}
