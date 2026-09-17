use super::import::{self, ResolvedImport};
use agni_importers::naming::encode_component;
use parking_lot::Mutex;
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone)]
pub enum Request {
    Search {
        site: String,
        query: String,
        page: u32,
    },
    Import {
        source: String,
    },
}

pub enum Reply {
    Search(Value),
    Import(ResolvedImport),
}

impl Request {
    pub fn query(&self) -> Result<String, String> {
        match self {
            Self::Search { site, query, page } => {
                agni_importers::riftbound::search::search_url(site, query, *page)?;
                Ok(format!(
                    "search_site={site}&q={}&page={page}",
                    encode_component(query)
                ))
            }
            Self::Import { source } => {
                if source.trim().is_empty() || source.len() > 16000 {
                    return Err("Import needs 1–16000 bytes".into());
                }
                let source = source.trim();
                let key = if source.starts_with("https://") || source.starts_with("http://") {
                    if agni_importers::riftbound::link::classify(source).is_none() {
                        return Err("Use a supported deck website URL".into());
                    }
                    "url"
                } else {
                    "text"
                };
                Ok(format!("{key}={}", encode_component(source)))
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn perform(request: &Request) -> Result<Reply, String> {
    use agni_importers::riftbound::query::DeckQuery;
    request.query()?;
    match request {
        Request::Search { site, query, page } => {
            agni_importers::riftbound::search::search(site, query, *page).map(Reply::Search)
        }
        Request::Import { source } => {
            let query = if import::is_link_line(source) {
                DeckQuery::Url(source.trim().into())
            } else {
                DeckQuery::Text(source.clone())
            };
            import::run_riftbound_query(&query).map(Reply::Import)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub async fn perform(request: &Request) -> Result<Reply, String> {
    let query = request.query()?;
    let base = crate::net::gateway::gateway_base().ok_or("No content gateway available")?;
    let (status, body) =
        crate::net::gateway::fetch_status_text(&format!("{base}/gateway/resolve/deck?{query}"))
            .await?;
    match request {
        Request::Import { .. } => import::parse_reply(status, &body).map(Reply::Import),
        Request::Search { .. } => {
            let value: Value =
                serde_json::from_str(&body).map_err(|_| "Invalid search response")?;
            if status != 200 {
                return Err(value["error"]
                    .as_str()
                    .unwrap_or("Deck search unavailable")
                    .into());
            }
            if !value["results"].is_array() {
                return Err("Gateway does not support deck search yet".into());
            }
            Ok(Reply::Search(value))
        }
    }
}

#[derive(Clone, Default)]
pub struct Job(Arc<Mutex<Option<Result<Reply, String>>>>);

impl Job {
    pub fn start(request: Request) -> Self {
        let job = Self::default();
        let result = job.0.clone();
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(move || *result.lock() = Some(perform(&request)));
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            *result.lock() = Some(perform(&request).await);
        });
        job
    }

    pub fn take(&self) -> Option<Result<Reply, String>> {
        self.0.lock().take()
    }
}

#[derive(Default)]
pub struct SearchPanel {
    query: String,
    site: usize,
    page: u32,
    job: Option<Job>,
    results: Vec<(String, String)>,
    error: Option<String>,
}

impl SearchPanel {
    pub fn ui(&mut self, ui: &mut bevy_egui::egui::Ui) -> Option<String> {
        use bevy_egui::egui;
        if let Some(result) = self.job.as_ref().and_then(Job::take) {
            self.job = None;
            match result {
                Ok(Reply::Search(value)) => {
                    self.results = value["results"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|r| {
                            Some((r["title"].as_str()?.into(), r["url"].as_str()?.into()))
                        })
                        .collect()
                }
                Err(error) => self.error = Some(error),
                _ => {}
            }
        }
        ui.label("Find a public Riftbound deck");
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.site, 0, "Piltover Archive");
            ui.selectable_value(&mut self.site, 1, "RiftDecks");
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.query)
                .hint_text("Deck, player or legend")
                .char_limit(160)
                .desired_width(f32::INFINITY),
        );
        self.page = self.page.clamp(1, 10);
        let mut search = false;
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.page)
                    .range(1..=10)
                    .prefix("page "),
            );
            search = ui
                .add_enabled(self.job.is_none(), egui::Button::new("search"))
                .clicked();
        });
        if search {
            self.error = None;
            self.results.clear();
            self.job = Some(Job::start(Request::Search {
                site: if self.site == 0 {
                    "piltover"
                } else {
                    "riftdecks"
                }
                .into(),
                query: self.query.clone(),
                page: self.page,
            }));
        }
        if self.job.is_some() {
            ui.spinner();
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(error) = &self.error {
            ui.label(error);
        }
        let mut chosen = None;
        egui::ScrollArea::vertical()
            .id_salt("deck search")
            .max_height(180.0)
            .show(ui, |ui| {
                for (title, url) in &self.results {
                    if ui
                        .selectable_label(false, title)
                        .on_hover_text(url)
                        .clicked()
                    {
                        chosen = Some(url.clone());
                    }
                }
            });
        chosen
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn importer_never_treats_a_local_path_or_arbitrary_host_as_a_fetch() {
        assert!(Request::Import {
            source: "https://evil.example/deck".into()
        }
        .query()
        .is_err());
        assert!(Request::Import {
            source: "/etc/passwd".into()
        }
        .query()
        .unwrap()
        .starts_with("text="));
        assert!(Request::Import {
            source: "3 Example\n2 Other".into()
        }
        .query()
        .unwrap()
        .contains("%0A"));
    }
}
