use crate::table::{MySeat, SessionInfo};
use bevy::prelude::*;
use bevy_egui::egui;
use spirit_node::peers::{format_age, format_bytes, LinkState, PeerSnapshot, RefStatus};

const REFRESH_SECS: f32 = 1.0;

#[derive(Resource, Default)]
pub struct PeerPanel {
    pub open: bool,
    peers: Vec<PeerSnapshot>,
    refs: Vec<RefStatus>,
    since_refresh: f32,
    clip_status: String,
}

impl PeerPanel {
    pub fn summary(&self) -> String {
        if self.peers.is_empty() {
            return "no peers yet".into();
        }
        let live = self
            .peers
            .iter()
            .filter(|peer| matches!(peer.state, LinkState::Connected | LinkState::Reachable))
            .count();
        format!("{} peer(s), {live} reachable", self.peers.len())
    }
}

pub fn refresh_peers(time: Res<Time>, mut panel: ResMut<PeerPanel>) {
    if !panel.open {
        if panel.since_refresh < REFRESH_SECS {
            panel.since_refresh = REFRESH_SECS;
        }
        return;
    }
    panel.since_refresh += time.delta_secs();
    if panel.since_refresh < REFRESH_SECS {
        return;
    }
    panel.since_refresh = 0.0;
    panel.peers = spirit_node::peers::snapshot();
    panel.refs = spirit_node::peers::registry().ref_statuses();
}

fn state_color(state: LinkState) -> egui::Color32 {
    match state {
        LinkState::Connected => egui::Color32::from_rgb(120, 200, 120),
        LinkState::Reachable => egui::Color32::from_rgb(150, 195, 140),
        LinkState::Disconnected => egui::Color32::from_rgb(210, 180, 110),
        LinkState::NeverReached => egui::Color32::from_rgb(190, 120, 120),
    }
}

fn ref_color(status: &RefStatus) -> egui::Color32 {
    if status.complete() {
        egui::Color32::from_rgb(120, 200, 120)
    } else if status.held > 0 {
        egui::Color32::from_rgb(210, 180, 110)
    } else {
        egui::Color32::from_rgb(190, 120, 120)
    }
}

fn roles(peer: &PeerSnapshot) -> String {
    let mut parts = Vec::new();
    if peer.we_served_them {
        parts.push("we served them");
    }
    if peer.we_fetched_from_them {
        parts.push("we fetched from them");
    }
    if parts.is_empty() {
        return "known only".into();
    }
    parts.join(" + ")
}

fn field(ui: &mut egui::Ui, label: &str, value: String) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).weak());
        ui.label(value);
    });
}

fn peer_row(ui: &mut egui::Ui, peer: &PeerSnapshot) -> Option<String> {
    let mut status = None;
    let heading = match crate::net::defaults::label_of(&peer.id) {
        Some(label) => format!("{label} {} — {}", peer.short_id(), peer.state.label()),
        None => format!("{} — {}", peer.short_id(), peer.state.label()),
    };
    let heading = egui::RichText::new(heading)
        .monospace()
        .color(state_color(peer.state));
    egui::CollapsingHeader::new(heading)
        .id_salt(peer.id.clone())
        .default_open(true)
        .show(ui, |ui| {
            field(ui, "role", roles(peer));
            field(ui, "learned", peer.discovery.label());
            field(ui, "path", peer.path_kind().label().into());
            if !peer.refs_advertised.is_empty() {
                field(ui, "advertises", peer.refs_advertised.join(", "));
            }
            if peer.dial_failures > 0 {
                ui.label(
                    egui::RichText::new(format!(
                        "{} failed dial(s): {}",
                        peer.dial_failures,
                        peer.last_dial_error.as_deref().unwrap_or("unknown")
                    ))
                    .color(egui::Color32::from_rgb(210, 130, 130)),
                );
            }
            if peer.wire_bytes_known {
                field(
                    ui,
                    "wire sent",
                    format!(
                        "{} ({} conns)",
                        format_bytes(peer.wire_bytes_sent),
                        peer.total_connections
                    ),
                );
                field(ui, "wire received", format_bytes(peer.wire_bytes_received));
            }
            if peer.payload_bytes_received > 0 {
                field(
                    ui,
                    "payload received",
                    format_bytes(peer.payload_bytes_received),
                );
            }
            if !peer.wire_bytes_known && peer.we_fetched_from_them {
                ui.label(
                    egui::RichText::new("wire byte counts unavailable on the fetching side")
                        .weak()
                        .italics(),
                );
            }
            if let Some(rtt) = peer.rtt {
                field(ui, "rtt", format!("{:.1} ms", rtt.as_secs_f64() * 1000.0));
            }
            for addr in &peer.active_addrs {
                field(ui, "active", addr.clone());
            }
            for addr in &peer.inactive_addrs {
                field(ui, "inactive", addr.clone());
            }
            if let Some(checked) = peer.addrs_checked {
                field(ui, "addresses checked", format_age(checked));
            }
            if let Some(name) = &peer.introduced_by_ref {
                field(ui, "ref", name.clone());
            }
            if let Some(outcome) = &peer.outcome {
                field(ui, "outcome", outcome.clone());
            }
            field(ui, "first seen", format_age(peer.first_seen));
            field(ui, "last activity", format_age(peer.last_activity));
            ui.horizontal(|ui| {
                if let Some(copied) =
                    crate::os::clipboard::copy_button(ui, "copy node id", &peer.id)
                {
                    status = Some(copied);
                }
                if let Some(ticket) = &peer.ticket {
                    if let Some(copied) =
                        crate::os::clipboard::copy_button(ui, "copy ticket", ticket)
                    {
                        status = Some(copied);
                    }
                }
            });
        });
    status
}

pub fn peer_section(
    ui: &mut egui::Ui,
    panel: &mut PeerPanel,
    info: &SessionInfo,
    my_seat: &MySeat,
) {
    let mut clip_status = None;
    ui.label(format!("table session: {}", info.label(my_seat.0)));
    ui.separator();
    if !panel.refs.is_empty() {
        ui.label(egui::RichText::new("refs").strong());
        #[cfg(target_arch = "wasm32")]
        ui.label(
            egui::RichText::new(
                "the browser keeps no card sets — incomplete refs are expected here, art rides the gateway bridge",
            )
            .weak(),
        );
        for status in &panel.refs {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(status.label()).color(ref_color(status)));
            });
            if !status.providers.is_empty() {
                let names: Vec<String> = status
                    .providers
                    .iter()
                    .map(|id| id.chars().take(12).collect())
                    .collect();
                field(ui, "complete on", names.join(", "));
            }
        }
        ui.separator();
    }
    let defaults: Vec<String> = crate::net::defaults::seeds()
        .into_iter()
        .map(|(label, forms)| {
            let first = forms.first().map(String::as_str).unwrap_or_default();
            format!("{label} {}", first.chars().take(28).collect::<String>())
        })
        .collect();
    if !defaults.is_empty() {
        field(ui, "default peers", defaults.join(", "));
    }
    if panel.peers.is_empty() {
        ui.label("no peers yet — scan or paste an identity QR");
        return;
    }
    ui.label(egui::RichText::new(panel.summary()).weak());
    ui.separator();
    for peer in &panel.peers {
        if let Some(status) = peer_row(ui, peer) {
            clip_status = Some(status);
        }
    }
    if let Some(status) = clip_status {
        panel.clip_status = status;
    }
    if !panel.clip_status.is_empty() {
        ui.label(egui::RichText::new(&panel.clip_status).weak());
    }
}
