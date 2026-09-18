use super::hud::{Drawer, DrawerState};
use super::*;
use crate::viewport::{back_pressed, consume_back, BackKey};
use history::{EventClass, History};

pub const LOG_KEY: KeyCode = KeyCode::KeyL;
pub const CHAT_KEY: KeyCode = KeyCode::KeyC;
pub const TOKENS_KEY: KeyCode = KeyCode::KeyK;
pub const CHAT_INPUT_W: f32 = 200.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Log,
    Chat,
    Tokens,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Log, Tab::Chat, Tab::Tokens];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Log => "log",
            Tab::Chat => "chat",
            Tab::Tokens => "tokens",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Tab::Log => "log",
            Tab::Chat => "chat",
            Tab::Tokens => "tok",
        }
    }

    pub fn key(self) -> KeyCode {
        match self {
            Tab::Log => LOG_KEY,
            Tab::Chat => CHAT_KEY,
            Tab::Tokens => TOKENS_KEY,
        }
    }

    pub fn hotkey(self) -> &'static str {
        match self {
            Tab::Log => "L",
            Tab::Chat => "C",
            Tab::Tokens => "K",
        }
    }
}

pub fn chat_available() -> bool {
    true
}

pub fn available(tab: Tab, free: bool) -> bool {
    match tab {
        Tab::Log => true,
        Tab::Chat => chat_available(),
        Tab::Tokens => free,
    }
}

pub fn tabs_shown(free: bool) -> Vec<Tab> {
    Tab::ALL
        .into_iter()
        .filter(|tab| available(*tab, free))
        .collect()
}

#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct DrawerPanel {
    pub tab: Tab,
}

pub fn is_open(drawer: &Drawer) -> bool {
    drawer.0 == DrawerState::Raised
}

pub fn toggle(open: bool, current: Tab, wanted: Tab, free: bool) -> (bool, Tab) {
    if !available(wanted, free) {
        return (open, current);
    }
    if open && current == wanted {
        (false, current)
    } else {
        (true, wanted)
    }
}

pub fn set(drawer: &mut Drawer, panel: &mut DrawerPanel, open: bool, tab: Tab) {
    let state = if open {
        DrawerState::Raised
    } else {
        DrawerState::Tucked
    };
    if drawer.0 != state {
        drawer.0 = state;
    }
    if panel.tab != tab {
        panel.tab = tab;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LadderRung {
    CancelPlacement,
    CloseDrawer,
    Pass,
}

pub fn ladder_rung(covered: bool, placing: bool, open: bool) -> LadderRung {
    if covered {
        LadderRung::Pass
    } else if placing {
        LadderRung::CancelPlacement
    } else if open {
        LadderRung::CloseDrawer
    } else {
        LadderRung::Pass
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn drawer_keys(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut back: ResMut<BackKey>,
    mut contexts: EguiContexts,
    covers: interaction::Covers,
    tools: Res<plugin_ui::Tools>,
    mut drawer: ResMut<Drawer>,
    mut panel: ResMut<DrawerPanel>,
    mut tokens: ResMut<tokens::TokenPanel>,
) {
    if !covers.menu.at_table() {
        return;
    }
    if let Ok(context) = contexts.ctx_mut() {
        if context.egui_wants_keyboard_input() {
            return;
        }
    }
    let covered = covers.covered();
    if back_pressed(&keys, &back) {
        match ladder_rung(covered, tokens.placing.is_some(), is_open(&drawer)) {
            LadderRung::CancelPlacement => tokens.placing = None,
            LadderRung::CloseDrawer => {
                let tab = panel.tab;
                set(&mut drawer, &mut panel, false, tab);
            }
            LadderRung::Pass => return,
        }
        consume_back(&mut keys, &mut back);
        return;
    }
    if covered {
        return;
    }
    for tab in [Tab::Log, Tab::Chat] {
        if keys.just_pressed(tab.key()) {
            let (open, next) = toggle(is_open(&drawer), panel.tab, tab, tools.free);
            set(&mut drawer, &mut panel, open, next);
            keys.clear_just_pressed(tab.key());
        }
    }
}

pub(super) fn sync_tokens_tab(
    menu: Res<crate::menu::Menu>,
    tools: Res<plugin_ui::Tools>,
    mut tokens: ResMut<tokens::TokenPanel>,
    mut drawer: ResMut<Drawer>,
    mut panel: ResMut<DrawerPanel>,
    mut seen: Local<bool>,
) {
    if !menu.at_table() {
        if is_open(&drawer) {
            let tab = panel.tab;
            set(&mut drawer, &mut panel, false, tab);
        }
        if tokens.open {
            tokens.open = false;
        }
        *seen = false;
        return;
    }
    if !tools.free && panel.tab == Tab::Tokens {
        panel.tab = Tab::Log;
    }
    let showing = is_open(&drawer) && panel.tab == Tab::Tokens;
    if tokens.open != *seen {
        let (open, next) = toggle(is_open(&drawer), panel.tab, Tab::Tokens, tools.free);
        if tokens.open {
            if available(Tab::Tokens, tools.free) {
                set(&mut drawer, &mut panel, open, next);
            } else {
                tokens.open = false;
            }
        } else if showing {
            set(&mut drawer, &mut panel, false, Tab::Tokens);
        }
    } else if tokens.open != showing {
        tokens.open = showing;
    }
    *seen = tokens.open;
}

pub fn tab_button(ui: &mut egui::Ui, tab: Tab, selected: bool, size: egui::Vec2) -> bool {
    let text = egui::RichText::new(tab.short())
        .size(11.0)
        .color(if selected { hud::INK } else { hud::INK_WEAK });
    let button = egui::Button::new(text)
        .min_size(size)
        .fill(if selected {
            hud::SURFACE_2
        } else {
            hud::SURFACE
        })
        .stroke(egui::Stroke::new(1.0, hud::HAIRLINE))
        .corner_radius(6.0);
    ui.add(button)
        .on_hover_text(format!("{} ({})", tab.label(), tab.hotkey()))
        .clicked()
}

pub(super) fn drawer_tabs_ui(
    mut contexts: EguiContexts,
    hud: Res<hud::Hud>,
    menu: Res<crate::menu::Menu>,
    tools: Res<plugin_ui::Tools>,
    mut drawer: ResMut<Drawer>,
    mut panel: ResMut<DrawerPanel>,
) -> Result {
    let Some(rect) = hud.0.drawer_tabs else {
        return Ok(());
    };
    if !menu.at_table() {
        return Ok(());
    }
    let context = contexts.ctx_mut()?.clone();
    let open = is_open(&drawer);
    let current = panel.tab;
    let mut pressed = None;
    hud::slot(&context, "drawer tabs", rect, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            for tab in tabs_shown(tools.free).into_iter().rev() {
                let selected = open && current == tab;
                if tab_button(ui, tab, selected, egui::vec2(hud::TAB_W, hud::TAB_H)) {
                    pressed = Some(tab);
                }
            }
        });
    });
    if let Some(tab) = pressed {
        let (next_open, next) = toggle(open, current, tab, tools.free);
        set(&mut drawer, &mut panel, next_open, next);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Text(String),
    Seat(u8),
    Card(u32),
    Zone(u16),
}

pub fn segments(text: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut rest = text;
    let mut plain = String::new();
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            break;
        };
        let token = &after[..end];
        let parsed = match token.split_once(' ') {
            Some(("seat", index)) => index.parse().ok().map(Segment::Seat),
            Some(("card", index)) => index.parse().ok().map(Segment::Card),
            Some(("zone", index)) => index.parse().ok().map(Segment::Zone),
            _ => None,
        };
        match parsed {
            Some(segment) => {
                plain.push_str(&rest[..start]);
                if !plain.is_empty() {
                    out.push(Segment::Text(std::mem::take(&mut plain)));
                }
                out.push(segment);
                rest = &after[end + 1..];
            }
            None => {
                plain.push_str(&rest[..start + 1]);
                rest = after;
            }
        }
    }
    plain.push_str(rest);
    if !plain.is_empty() {
        out.push(Segment::Text(plain));
    }
    out
}

pub fn log_line(
    ui: &mut egui::Ui,
    event: &history::Event,
    seats: &hud::Seats,
    colour_blind: bool,
) -> Option<u32> {
    let mut tapped = None;
    let table = &seats.table.0;
    let view = &seats.mirror.view;
    let me = seats.me();
    let toast = event.class == EventClass::Toast;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(3.0, 2.0);
        if let Some(seat) = event.seat {
            let rgb = seats.label(PlayerId(seat)).1;
            colors::swatch(ui, rgb, 10.0, colour_blind);
        }
        if toast {
            let (mark, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            history::paint_glyph(ui.painter(), mark, EventClass::Toast, hud::AMBER);
        }
        let ink = if toast { hud::AMBER } else { hud::INK };
        for segment in segments(&event.text) {
            match segment {
                Segment::Text(text) => {
                    ui.label(egui::RichText::new(text).size(13.0).color(ink));
                }
                Segment::Seat(seat) => {
                    let (name, rgb, _) = seats.label(PlayerId(seat));
                    let [r, g, b] = rgb;
                    ui.label(
                        egui::RichText::new(name)
                            .size(13.0)
                            .strong()
                            .color(egui::Color32::from_rgb(r, g, b)),
                    );
                }
                Segment::Zone(zone) => {
                    ui.label(
                        egui::RichText::new(plugin_ui::zone_label(&view.zones, zone))
                            .size(13.0)
                            .color(ink),
                    );
                }
                Segment::Card(card) => {
                    let name = plugin_ui::card_label(table, view, me, card);
                    let link = egui::Label::new(
                        egui::RichText::new(name).size(13.0).underline().color(ink),
                    )
                    .sense(egui::Sense::click());
                    if ui
                        .add(link)
                        .on_hover_text("show in the inspector")
                        .clicked()
                    {
                        tapped = Some(card);
                    }
                }
            }
        }
    });
    tapped
}

pub const PRESETS: [(&str, &str); 3] = [
    ("fast", crate::ai::nanogpt::DEFAULT_MODEL),
    ("fastest", crate::ai::nanogpt::FASTEST_MODEL),
    ("thinking", crate::ai::nanogpt::THINKING_MODEL),
];

pub fn preset_row(ui: &mut egui::Ui, model: &mut String) -> bool {
    let mut switched = false;
    ui.horizontal_wrapped(|ui| {
        for (label, preset) in PRESETS {
            if ui
                .add(egui::Button::new(label).selected(model == preset))
                .on_hover_text(preset)
                .clicked()
                && model != preset
            {
                *model = preset.to_string();
                switched = true;
            }
        }
    });
    switched
}

pub fn model_controls(ui: &mut egui::Ui, lobby: &mut crate::ai::seat::AiLobby) {
    use crate::ai::seat;
    ui.horizontal(|ui| {
        ui.label("model");
        ui.add(egui::TextEdit::singleline(&mut lobby.model).desired_width(220.0));
    });
    if lobby.credentials.provider == crate::ai::provider::Provider::NanoGpt
        && preset_row(ui, &mut lobby.model)
    {
        lobby.kind = crate::ai::driver::MindKind::Llm;
        if let Some(status) = seat::status().filter(|status| status.alive) {
            lobby.status = seat::switch_live_model(&status, &lobby.model);
        }
    }
    if let Some(status) = seat::status() {
        ui.label(
            egui::RichText::new(format!("log: {}", status.log.display()))
                .weak()
                .small(),
        );
    }
    if !lobby.status.is_empty() {
        ui.label(egui::RichText::new(&lobby.status).weak().small());
    }
}

pub fn chat_line(ui: &mut egui::Ui, line: &str) {
    if let Some(text) = line.strip_prefix("you: ") {
        ui.label(
            egui::RichText::new(format!("you: {text}"))
                .strong()
                .color(hud::INK),
        );
    } else if let Some(text) = line.strip_prefix("bot: ") {
        ui.label(egui::RichText::new(format!("bot: {text}")).color(hud::INK));
    } else if let Some(model) = line.strip_prefix("model: ") {
        ui.label(
            egui::RichText::new(format!("switched to {model}"))
                .weak()
                .small(),
        );
    }
}

pub fn chat_tab(ui: &mut egui::Ui, lobby: &mut crate::ai::seat::AiLobby, info: &SessionInfo) {
    use crate::ai::seat;
    let status = seat::status();
    let lines = &info.chat.lines;
    let mut send = false;
    let mut switch = false;
    let mut stop = false;
    match &status {
        Some(status) if status.alive => {
            ui.label(egui::RichText::new(status.seat_line()).color(hud::INK));
            if let Some(fault) = status.fault_line() {
                ui.label(egui::RichText::new(fault).color(hud::INK_WEAK).small());
            }
        }
        Some(status) => {
            ui.label(
                egui::RichText::new(format!(
                    "the AI player exited ({})",
                    status.exit.clone().unwrap_or_default()
                ))
                .color(hud::INK_WEAK),
            );
        }
        None => {
            ui.label(
                egui::RichText::new("table chat · everyone at this table can read messages")
                    .color(hud::INK_WEAK),
            );
        }
    }
    egui::ScrollArea::vertical()
        .id_salt("chat lines")
        .max_height(ui.available_height() - 96.0)
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for line in lines {
                let name = info
                    .roster
                    .iter()
                    .find(|p| p.seat == line.seat)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| format!("player {}", line.seat + 1));
                ui.label(format!("{name}: {}", line.text));
            }
            if lines.is_empty() {
                ui.label(
                    egui::RichText::new("say hi, ask for a matchup, or tell it which deck to play")
                        .weak(),
                );
            }
        });
    ui.horizontal(|ui| {
        let response = ui.add(
            egui::TextEdit::singleline(&mut lobby.draft)
                .hint_text("message everyone at the table")
                .char_limit(2000)
                .desired_width((ui.available_width() - 64.0).max(80.0)),
        );
        if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
            send = true;
        }
        if ui.button("send").clicked() {
            send = true;
        }
    });
    ui.label(egui::RichText::new("Messages may be sent to the provider of any AI at this table. Never share API keys here.").small().weak());
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("switch model").weak());
        switch = lobby.credentials.provider == crate::ai::provider::Provider::NanoGpt
            && preset_row(ui, &mut lobby.model);
        if status.as_ref().is_some_and(|status| status.alive)
            && ui.small_button("stop AI").clicked()
        {
            stop = true;
        }
    });
    if !lobby.status.is_empty() {
        ui.label(egui::RichText::new(&lobby.status).weak().small());
    }
    let live = status.as_ref().filter(|status| status.alive);
    if send && !lobby.draft.trim().is_empty() {
        match info.chat.send(&lobby.draft) {
            Ok(()) => lobby.draft.clear(),
            Err(error) => lobby.status = error,
        }
    }
    if switch {
        lobby.kind = crate::ai::driver::MindKind::Llm;
        lobby.status = match live {
            Some(status) => seat::switch_live_model(status, &lobby.model),
            None => format!("{} takes over when the AI is next added", lobby.model),
        };
    }
    if stop {
        seat::stop();
        lobby.status = "AI player stopped".into();
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn drawer_ui(
    mut contexts: EguiContexts,
    hud: Res<hud::Hud>,
    viewport: Res<crate::viewport::Viewport>,
    menu: Res<crate::menu::Menu>,
    tools: Res<plugin_ui::Tools>,
    tuning: Res<Tuning>,
    history: Res<History>,
    seats: hud::Seats,
    plugin_tokens: Res<tokens::PluginTokens>,
    mut token_panel: ResMut<tokens::TokenPanel>,
    mut drawer: ResMut<Drawer>,
    mut panel: ResMut<DrawerPanel>,
    mut selected: ResMut<Selected>,
    mut lobby: ResMut<crate::ai::seat::AiLobby>,
    cards: Query<(Entity, &CardView)>,
) -> Result {
    if !menu.at_table() || !is_open(&drawer) {
        return Ok(());
    }
    let context = contexts.ctx_mut()?.clone();
    let tab = panel.tab;
    let mut switch_to = None;
    let mut close = false;
    let mut tapped: Option<u32> = None;
    let mut placing_started = false;
    let colour_blind = tuning.colour_blind;
    let mut body = |ui: &mut egui::Ui| {
        ui.horizontal(|ui| {
            for shown in tabs_shown(tools.free) {
                if crate::menu::chip(ui, shown.label(), shown == tab).clicked() && shown != tab {
                    switch_to = Some(shown);
                }
            }
        });
        ui.separator();
        match tab {
            Tab::Log => {
                egui::ScrollArea::vertical()
                    .id_salt("drawer log")
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        if history.events.is_empty() {
                            ui.label(
                                egui::RichText::new("nothing has happened yet")
                                    .color(hud::INK_WEAK),
                            );
                        }
                        for event in &history.events {
                            if let Some(card) = log_line(ui, event, &seats, colour_blind) {
                                tapped = Some(card);
                            }
                        }
                    });
            }
            Tab::Chat => {
                chat_tab(ui, &mut lobby, &seats.info);
            }
            Tab::Tokens => {
                egui::ScrollArea::vertical()
                    .id_salt("drawer tokens")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if tokens::tokens_tab(
                            ui,
                            &mut token_panel,
                            &plugin_tokens,
                            &seats.mirror.view.zones,
                        )
                        .is_some()
                        {
                            placing_started = true;
                        }
                    });
            }
        }
    };
    match hud.0.drawer {
        Some(rect) => {
            hud::slot(&context, "drawer", rect, |ui| {
                hud::panel_frame(ui.style()).show(ui, |ui| {
                    ui.set_min_size(rect.size() - egui::vec2(16.0, 16.0));
                    ui.set_max_size(rect.size() - egui::vec2(16.0, 16.0));
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(tab.label()).strong().color(hud::INK));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(egui::Button::new(egui::RichText::new("×").size(16.0)))
                                .clicked()
                            {
                                close = true;
                            }
                        });
                    });
                    body(ui);
                });
            });
        }
        None => {
            let mut open = true;
            hud::sheet(
                &context,
                "drawer sheet",
                viewport.class,
                hud::Side::Right,
                tab.label(),
                &mut open,
                &mut body,
            );
            close = !open;
        }
    }
    if let Some(next) = switch_to {
        set(&mut drawer, &mut panel, true, next);
    }
    if close || placing_started {
        let tab = panel.tab;
        set(&mut drawer, &mut panel, false, tab);
    }
    if let Some(card) = tapped {
        if let Some((entity, _)) = cards.iter().find(|(_, view)| view.0 .0 == card) {
            selected.0 = Some(entity);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drawer_opens_on_l_c_k_and_a_second_press_closes_it() {
        assert_eq!(Tab::Log.key(), KeyCode::KeyL);
        assert_eq!(Tab::Chat.key(), KeyCode::KeyC);
        assert_eq!(Tab::Tokens.key(), KeyCode::KeyK);
        let (open, tab) = toggle(false, Tab::Log, Tab::Log, false);
        assert!(open && tab == Tab::Log);
        let (open, tab) = toggle(true, Tab::Log, Tab::Log, false);
        assert!(!open && tab == Tab::Log, "the same key again closes it");
        let (open, tab) = toggle(true, Tab::Log, Tab::Chat, false);
        assert_eq!(
            (open, tab),
            (true, Tab::Chat),
            "another key switches the tab"
        );
        assert_eq!(
            toggle(false, Tab::Log, Tab::Tokens, false),
            (false, Tab::Log),
            "tokens need a free table"
        );
        assert_eq!(
            toggle(false, Tab::Log, Tab::Tokens, true),
            (true, Tab::Tokens)
        );
        assert_eq!(tabs_shown(false), vec![Tab::Log, Tab::Chat]);
        assert_eq!(tabs_shown(true), vec![Tab::Log, Tab::Chat, Tab::Tokens]);
        assert!(available(Tab::Chat, false) == chat_available());
        let mut drawer = Drawer::default();
        let mut panel = DrawerPanel::default();
        set(&mut drawer, &mut panel, true, Tab::Chat);
        assert!(is_open(&drawer));
        assert_eq!(panel.tab, Tab::Chat);
        set(&mut drawer, &mut panel, false, Tab::Chat);
        assert!(!is_open(&drawer));
        assert_eq!(Tab::Tokens.short(), "tok");
        assert_eq!(Tab::Chat.hotkey(), "C");
    }

    #[test]
    fn the_drawer_closes_on_the_ladder_after_a_placement_and_never_under_a_sheet() {
        assert_eq!(ladder_rung(false, false, true), LadderRung::CloseDrawer);
        assert_eq!(ladder_rung(false, true, true), LadderRung::CancelPlacement);
        assert_eq!(ladder_rung(false, true, false), LadderRung::CancelPlacement);
        assert_eq!(ladder_rung(false, false, false), LadderRung::Pass);
        assert_eq!(
            ladder_rung(true, false, true),
            LadderRung::Pass,
            "a sheet above the drawer takes the escape first"
        );
    }

    #[test]
    fn a_log_sentence_splits_into_plain_text_and_tappable_names() {
        assert_eq!(
            segments("{seat 0} plays {card 12} at {zone 9}"),
            vec![
                Segment::Seat(0),
                Segment::Text(" plays ".into()),
                Segment::Card(12),
                Segment::Text(" at ".into()),
                Segment::Zone(9),
            ]
        );
        assert_eq!(
            segments("no damage is dealt"),
            vec![Segment::Text("no damage is dealt".into())]
        );
        assert_eq!(
            segments("{bogus} and {card x}"),
            vec![Segment::Text("{bogus} and {card x}".into())]
        );
        assert_eq!(segments(""), Vec::<Segment>::new());
    }
}
