use super::NetParams;
use crate::net::{identity, peers};

use crate::table::MySeat;
use crate::theme;
use bevy_egui::egui;

pub const NAME_NOTE: &str =
    "the name the roster shows; it travels with the next table you host or join";

pub fn you_tab(ui: &mut egui::Ui, net: &mut NetParams, my_seat: &MySeat) {
    identity::identity_header(ui, &mut net.identity, &[]);
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new("add a peer")
            .strong()
            .color(theme::tokens(ui.ctx()).ink),
    );
    identity::identity_section(ui, &mut net.identity);
    ui.add_space(8.0);
    crate::os::profile::name_field(ui, &mut net.name, 200.0);
    ui.label(
        egui::RichText::new(NAME_NOTE)
            .color(theme::tokens(ui.ctx()).ink_weak)
            .small(),
    );
    ui.add_space(8.0);
    egui::CollapsingHeader::new("details")
        .id_salt("peer details")
        .show(ui, |ui| {
            peers::peer_section(ui, &mut net.peers, &net.info, my_seat);
        });
}
