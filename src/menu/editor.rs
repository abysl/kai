use super::browser::{self, BrowserAction, BrowserContext};
use super::{
    back_button, chip, content_width, float_note, gear_button, link, margin, segmented, step_title,
    Menu, Screen, EDITOR_MAX_W,
};
use crate::deck::catalog::{Catalog, Filter};
use crate::deck::editor::{self, Draft, Edit, Pane, Placed, Zone};
use crate::deck::rows::{self, RowAction, RowEvent};
use crate::deck::thumbs::Thumbs;
use crate::deck::{battlefield, exchange, history, import, sideboard};
use crate::net::{self, TableGame};
use crate::render::art::ArtCache;
use crate::settings::{DeckParams, NetParams, Settings, TableParams};
use crate::table::hud;
use crate::table::MySeat;
use crate::theme::{self, Tokens};
use crate::viewport::{InputKind, ViewportClass};
use agni_importers::riftbound::catalog::CardKind;
use agni_riftbound::legality::{Rule, Verdict, COPY_LIMIT};
use agni_riftbound::{DeckEntry, ResolvedCard, RUNE_DECK_SIZE};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use std::collections::{BTreeMap, HashMap};

pub const TITLE: &str = "deck editor";
pub const LIST_W_DESKTOP: f32 = 360.0;
pub const LIST_W_TABLET: f32 = 300.0;
pub const TWO_PANE_MIN_W: f32 = 700.0;
pub const COMPACT_MAX_H: f32 = 520.0;
pub const METER_H: f32 = 40.0;
pub const CURVE_H: f32 = 40.0;
pub const FOOTER_H_PHONE: f32 = 56.0;
pub const FOOTER_H_DESKTOP: f32 = 44.0;
pub const TOAST_SECS: f64 = 1.5;
pub const STATUS_SECS: f64 = 4.0;
pub const CURVE_BARS: usize = 8;
pub const ENFORCED_NOTE: &str = "rules enforced seats a legal deck only";
pub const WEB_SAVE_NOTE: &str = "no deck store on the web — copy the code to keep it";

#[derive(Resource, Default)]
pub struct EditorSheet {
    pub renaming: Option<String>,
    pub drawer: bool,
    pub discard_armed: bool,
    pub clear_armed: bool,
    pub seat_armed: bool,
    pub qr: Option<String>,
    pub toast: Option<(String, f64)>,
    pub status: Option<(String, f64)>,
    pub art_edits: Option<u64>,
    pub seen_open: Option<u64>,
    pub importing: bool,
}

pub const IMPORT_SLOT: &str = "editor-import";
pub const IMPORT_TITLE: &str = "import from the clipboard";
pub const IMPORT_READING: &str = "reading the clipboard…";
pub const IMPORT_NOTE: &str =
    "copy a deck list, a Piltover Archive deck code, or a riftdecks / piltover / rift atlas link, then press import";

impl EditorSheet {
    pub fn toast(&mut self, text: impl Into<String>, now: f64) {
        self.toast = Some((text.into(), now));
    }

    pub fn status(&mut self, text: impl Into<String>, now: f64) {
        self.status = Some((text.into(), now));
    }

    pub fn live_toast(&mut self, now: f64) -> Option<&str> {
        if self
            .toast
            .as_ref()
            .is_some_and(|(_, since)| now - since > TOAST_SECS)
        {
            self.toast = None;
        }
        self.toast.as_ref().map(|(text, _)| text.as_str())
    }

    pub fn live_status(&mut self, now: f64) -> Option<&str> {
        if self
            .status
            .as_ref()
            .is_some_and(|(_, since)| now - since > STATUS_SECS)
        {
            self.status = None;
        }
        self.status.as_ref().map(|(text, _)| text.as_str())
    }

    pub fn begin_open(&mut self, opens: u64) {
        if self.seen_open == Some(opens) {
            return;
        }
        self.seen_open = Some(opens);
        self.renaming = None;
        self.drawer = false;
        self.discard_armed = false;
        self.clear_armed = false;
        self.seat_armed = false;
        self.toast = None;
        self.status = None;
        self.importing = false;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Layout {
    TwoPane { list_w: f32 },
    Split,
    Segmented,
}

pub fn compact(class: ViewportClass, screen_h: f32) -> bool {
    class == ViewportClass::PhoneLandscape || screen_h < COMPACT_MAX_H
}

pub fn layout_for(class: ViewportClass, screen: egui::Vec2) -> Layout {
    match class {
        ViewportClass::PhonePortrait => Layout::Segmented,
        ViewportClass::PhoneLandscape => Layout::Split,
        ViewportClass::Tablet if compact(class, screen.y) => Layout::Split,
        ViewportClass::Tablet if screen.x < TWO_PANE_MIN_W => Layout::Segmented,
        ViewportClass::Tablet => Layout::TwoPane {
            list_w: LIST_W_TABLET,
        },
        ViewportClass::Desktop => Layout::TwoPane {
            list_w: LIST_W_DESKTOP,
        },
    }
}

pub fn footer_height(class: ViewportClass) -> f32 {
    if class.is_phone() {
        FOOTER_H_PHONE
    } else {
        FOOTER_H_DESKTOP
    }
}

pub fn shows_curve(class: ViewportClass, screen_h: f32) -> bool {
    !class.is_phone() && !compact(class, screen_h)
}

pub fn meter_only_pill(class: ViewportClass, screen_h: f32) -> bool {
    compact(class, screen_h)
}

pub fn pill_in_title(class: ViewportClass, screen_h: f32, pane: Pane) -> bool {
    meter_only_pill(class, screen_h)
        || (class == ViewportClass::PhonePortrait && pane == Pane::Cards)
}

pub fn shows_curve_line(class: ViewportClass, pane: Pane) -> bool {
    class == ViewportClass::PhonePortrait && pane == Pane::List
}

pub fn meter_scrolls(class: ViewportClass) -> bool {
    class == ViewportClass::PhonePortrait
}

pub fn footer_folds_share(class: ViewportClass) -> bool {
    class == ViewportClass::PhonePortrait
}

pub fn meter_chips(meter: &agni_riftbound::legality::Meter) -> Vec<String> {
    let mark = |ok: bool| if ok { "✔" } else { "–" };
    vec![
        format!("legend {}", mark(meter.legend)),
        format!("champion {}", mark(meter.champion)),
        format!("main {}/{}", meter.main.0, meter.main.1),
        format!("runes {}/{}", meter.runes.0, meter.runes.1),
        format!("fields {}/{}", meter.battlefields.0, meter.battlefields.1),
        format!("sig {}/{}", meter.signatures.0, meter.signatures.1),
    ]
}

pub fn curve_line(curve: &[u32; CURVE_BARS]) -> String {
    let bars: Vec<String> = curve.iter().map(u32::to_string).collect();
    format!("curve {}", bars.join(" · "))
}

pub fn seat_label(
    enforced: bool,
    verdict: Verdict,
    armed: bool,
    next_game: bool,
) -> (String, bool) {
    match verdict {
        Verdict::Broken(_) if armed && !enforced => ("seat anyway".into(), true),
        Verdict::Broken(n) => (
            format!("seat · {}", rows::verdict_text(Verdict::Broken(n))),
            crate::deck::actions::selection_legal(verdict, enforced),
        ),
        _ if next_game => ("use this list next game".into(), true),
        _ => ("seat this deck".into(), true),
    }
}

pub fn seat_arms(verdict: Verdict, enforced: bool, armed: bool) -> bool {
    matches!(verdict, Verdict::Broken(_)) && !enforced && !armed
}

pub fn is_shortfall(rule: &Rule) -> bool {
    editor::shortfall_filter(rule).is_some()
}

enum Action {
    Edit(Edit),
    ImportClipboard,
    Load {
        deck: agni_riftbound::ResolvedDeck,
        label: String,
        source: String,
        unresolved: Vec<String>,
    },
    SaveImport {
        deck: import::ImportedDeck,
        label: String,
        source: String,
    },
    CloseImport,
    Seat {
        next_game: bool,
    },
    Save,
    SaveCopy,
    Undo,
    Discard,
    FillRunes,
    Status(String),
    Browse(Filter, String),
}

pub fn stage(contexts: &mut EguiContexts, decks: &mut DeckParams) {
    if decks.editor.draft.is_some() {
        stage_draft_art(contexts, decks);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn editor_screen(
    ui: &mut egui::Ui,
    class: ViewportClass,
    input: InputKind,
    menu: &mut Menu,
    settings: &mut Settings,
    my_seat: &MySeat,
    table: &mut TableParams,
    net: &mut NetParams,
    decks: &mut DeckParams,
) {
    if decks.editor.draft.is_none() {
        editor::close(&mut decks.editor, menu);
        return;
    }
    let opens = decks.editor.opens;
    decks.sheet.begin_open(opens);
    let enforced = net::rules_enforced(&net.info, &net.choice);
    let riftbound_table = decks
        .mirror
        .view
        .zones
        .iter()
        .any(|decl| decl.name == agni_riftbound::ZONE_NAME_MAIN_DECK);
    let between_games = hud::between_games(&net.info, &table.panel.view);
    let next_game = sideboard::reload_allowed(
        net.info.active(),
        riftbound_table,
        table.tools.free,
        between_games,
    );
    let screen_w = ui.max_rect().width();
    let width = content_width(class, screen_w, EDITOR_MAX_W);
    let left = ui.max_rect().min.x + ((screen_w - width) / 2.0).max(margin(class));
    let rect = egui::Rect::from_min_size(
        egui::pos2(left, ui.max_rect().min.y + margin(class) / 2.0),
        egui::vec2(width, ui.max_rect().height() - margin(class)),
    );
    let mut column = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    column.set_max_width(width);
    column.set_min_width(width);
    let ui = &mut column;
    let mut leave = false;
    ui.horizontal(|ui| {
        if back_button(ui) {
            leave = true;
        }
        ui.label(
            egui::RichText::new(TITLE)
                .size(20.0)
                .strong()
                .color(theme::tokens(ui.ctx()).ink),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if gear_button(ui) {
                settings.open = true;
            }
        });
    });
    ui.separator();
    let mut actions = Vec::new();
    {
        let context = ui.ctx().clone();
        let DeckParams {
            editor,
            catalog,
            browser,
            sheet,
            art,
            ..
        } = decks;
        let Some(draft) = editor.draft.as_mut() else {
            return;
        };
        let undo_key = !context.egui_wants_keyboard_input()
            && context.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::Z));
        if undo_key {
            actions.push(Action::Undo);
        }
        let copies = draft.copies();
        let identity = draft.identity();
        let champion_tags = draft.champion_tags();
        let mut frame = Frame {
            draft,
            catalog,
            thumbs: &mut browser.0,
            art,
            sheet,
            class,
            input,
            enforced,
            next_game,
            screen: context.content_rect().size(),
            opens,
            copies,
            identity,
            champion_tags,
            actions: &mut actions,
        };
        let body_rect = egui::Rect::from_min_max(ui.cursor().min, rect.max);
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(body_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
            |ui| {
                ui.set_clip_rect(body_rect.intersect(ui.clip_rect()));
                frame.body(ui);
            },
        );
    }
    if decks.sheet.importing {
        import_modal(ui.ctx(), &mut decks.import, &mut actions);
    }
    if leave {
        editor::close(&mut decks.editor, menu);
    }
    let now = ui.input(|input| input.time);
    for action in actions {
        run_action(action, now, menu, my_seat, decks);
    }
}

pub fn poll_clipboard(panel: &mut import::ImportPanel) -> Option<String> {
    match crate::os::clipboard::take_paste(IMPORT_SLOT)? {
        Ok(text) => {
            panel.paste = text;
            panel.note = None;
            panel.busy = false;
            import::begin_import_any(panel);
            None
        }
        Err(error) => {
            panel.busy = false;
            Some(format!("clipboard read failed: {error}"))
        }
    }
}

fn import_modal(
    context: &egui::Context,
    panel: &mut import::ImportPanel,
    actions: &mut Vec<Action>,
) {
    if let Some(error) = poll_clipboard(panel) {
        panel.error = Some(error);
    }
    let modal = egui::Modal::new(egui::Id::new("editor import")).show(context, |ui| {
        let tokens = theme::dress(ui);
        ui.set_width(ui.ctx().content_rect().width().min(480.0) - 32.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(IMPORT_TITLE).strong().color(tokens.ink));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("×").clicked() {
                    actions.push(Action::CloseImport);
                }
            });
        });
        if panel.busy {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        import::import_status(ui, panel);
        ui.collapsing("search public decklists", |ui| {
            if let Some(url) = panel.search.ui(ui) {
                panel.paste = url;
                import::begin_import_any(panel);
            }
        });
        if let Some(action) = import::result_panel(ui, panel, import::LOAD_LABEL) {
            actions.push(match action {
                import::ImportAction::Edit {
                    deck,
                    label,
                    source,
                    unresolved,
                } => Action::Load {
                    deck,
                    label,
                    source,
                    unresolved,
                },
                import::ImportAction::Save {
                    deck,
                    label,
                    source,
                } => Action::SaveImport {
                    deck,
                    label,
                    source,
                },
            });
        }
        if !panel.busy {
            ui.horizontal(|ui| {
                if ui.button("read the clipboard again").clicked() {
                    actions.push(Action::ImportClipboard);
                }
                if link(ui, "close").clicked() {
                    actions.push(Action::CloseImport);
                }
            });
            ui.label(
                egui::RichText::new(IMPORT_NOTE)
                    .small()
                    .color(tokens.ink_weak),
            );
        }
    });
    if modal.should_close() {
        actions.push(Action::CloseImport);
    }
}

pub fn start_import(sheet: &mut EditorSheet, panel: &mut import::ImportPanel) {
    sheet.importing = true;
    panel.clear_resolved();
    panel.error = None;
    panel.paste.clear();
    panel.busy = true;
    panel.note = Some(IMPORT_READING.into());
    crate::os::clipboard::request_paste(IMPORT_SLOT);
}

pub fn import_text(sheet: &mut EditorSheet, panel: &mut import::ImportPanel, text: String) {
    sheet.importing = true;
    panel.clear_resolved();
    panel.error = None;
    panel.paste = text;
    import::begin_import_any(panel);
}

fn stage_draft_art(contexts: &mut EguiContexts, decks: &mut DeckParams) {
    let DeckParams {
        editor,
        browser,
        art,
        images,
        registry,
        sheet,
        catalog,
        ..
    } = decks;
    let Some(draft) = editor.draft.as_ref() else {
        return;
    };
    browser.0.stage_cards(
        contexts,
        registry,
        images,
        catalog.cards_at(&draft.browser.visible),
        art,
    );
    let cards = draft_cards(&draft.deck);
    if sheet.art_edits != Some(draft.edits) {
        sheet.art_edits = Some(draft.edits);
        request_art(&cards, art);
    }
    let pending: Vec<DeckEntry> = cards
        .into_iter()
        .filter(|card| browser.0.id(&card.riftbound_id).is_none())
        .map(|card| DeckEntry {
            card: card.clone(),
            count: 1,
        })
        .collect();
    if pending.is_empty() {
        return;
    }
    let faces: HashMap<String, agni_core::CardFace> = pending
        .iter()
        .map(|entry| {
            (
                entry.card.riftbound_id.clone(),
                import::placeholder_face(&entry.card.name),
            )
        })
        .collect();
    browser
        .0
        .stage(contexts, registry, images, pending.iter(), &faces, art);
}

fn draft_cards(deck: &agni_riftbound::ResolvedDeck) -> Vec<&ResolvedCard> {
    let zones: [&[DeckEntry]; 4] = [
        &deck.main_deck,
        &deck.runes,
        &deck.battlefields,
        &deck.sideboard,
    ];
    deck.legend
        .iter()
        .chain(deck.chosen_champion.iter())
        .chain(zones.into_iter().flatten().map(|entry| &entry.card))
        .collect()
}

#[cfg(not(target_arch = "wasm32"))]
fn request_art(cards: &[&ResolvedCard], art: &ArtCache) {
    use crate::render::art::{ArtGame, ArtRequest};
    let wanted: Vec<ArtRequest> = cards
        .iter()
        .filter(|card| !art.has(&card.name))
        .map(|card| ArtRequest::by_id(ArtGame::Riftbound, &card.riftbound_id, &card.name))
        .collect();
    if !wanted.is_empty() {
        crate::render::art::enqueue(wanted);
    }
}

#[cfg(target_arch = "wasm32")]
fn request_art(cards: &[&ResolvedCard], art: &mut ArtCache) {
    let landed: Vec<(String, Vec<u8>)> = cards
        .iter()
        .filter(|card| !art.has(&card.name))
        .filter_map(|card| {
            let bytes = crate::net::gateway::riftbound_art(&card.riftbound_id)?;
            Some((card.name.clone(), bytes))
        })
        .collect();
    if !landed.is_empty() {
        art.extend(landed);
    }
}

struct Frame<'a> {
    draft: &'a mut Draft,
    catalog: &'a Catalog,
    thumbs: &'a mut Thumbs,
    art: &'a mut ArtCache,
    sheet: &'a mut EditorSheet,
    class: ViewportClass,
    input: InputKind,
    enforced: bool,
    next_game: bool,
    screen: egui::Vec2,
    opens: u64,
    copies: BTreeMap<String, u32>,
    identity: Vec<String>,
    champion_tags: Vec<String>,
    actions: &'a mut Vec<Action>,
}

impl Frame<'_> {
    fn body(&mut self, ui: &mut egui::Ui) {
        let tokens = theme::tokens(ui.ctx());
        let now = ui.input(|input| input.time);
        let footer_h = footer_height(self.class);
        let pane = self.draft.pane;
        self.title_row(ui, &tokens);
        if !pill_in_title(self.class, self.screen.y, pane) {
            self.meter_strip(ui);
        }
        if self.sheet.drawer {
            self.findings_drawer(ui, &tokens);
        }
        if shows_curve(self.class, self.screen.y) {
            self.curve(ui, &tokens);
        } else if shows_curve_line(self.class, pane) {
            ui.label(
                egui::RichText::new(curve_line(&self.draft.curve()))
                    .small()
                    .color(tokens.ink_weak),
            );
        }
        let layout = layout_for(self.class, self.screen);
        if layout == Layout::Segmented {
            segmented(
                ui,
                &mut self.draft.pane,
                &[(Pane::List, "deck"), (Pane::Cards, "cards")],
            );
        }
        if let Some(text) = self.sheet.live_toast(now) {
            float_note(
                ui.ctx(),
                "editor toast",
                ui.cursor().min + egui::vec2(8.0, 8.0),
                text,
                tokens.amber,
            );
        }
        let spacing = ui.spacing().item_spacing.y;
        let max = ui.max_rect();
        let footer_rect =
            egui::Rect::from_min_max(egui::pos2(max.min.x, max.max.y - footer_h), max.max);
        let pane_rect = egui::Rect::from_min_max(
            ui.cursor().min,
            egui::pos2(
                max.max.x,
                (footer_rect.min.y - spacing).max(ui.cursor().min.y + 80.0),
            ),
        );
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(pane_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
            |ui| {
                ui.set_clip_rect(pane_rect.intersect(ui.clip_rect()));
                self.panes(ui, layout, pane_rect.width());
            },
        );
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(footer_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
            |ui| {
                ui.set_clip_rect(footer_rect.intersect(ui.clip_rect()));
                self.footer(ui, &tokens, footer_h);
            },
        );
    }

    fn title_row(&mut self, ui: &mut egui::Ui, tokens: &Tokens) {
        ui.horizontal(|ui| {
            match self.sheet.renaming.as_mut() {
                Some(label) => {
                    let field = ui.add(
                        egui::TextEdit::singleline(label)
                            .desired_width((ui.available_width() - 120.0).max(80.0)),
                    );
                    let done =
                        field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                    if done || ui.button("done").clicked() {
                        let label = label.clone();
                        self.draft.rename(&label);
                        self.sheet.renaming = None;
                    } else {
                        field.request_focus();
                    }
                }
                None => {
                    let title = if self.draft.dirty {
                        format!("{} ·", self.draft.label)
                    } else {
                        self.draft.label.clone()
                    };
                    ui.label(egui::RichText::new(title).strong().color(tokens.ink));
                    if link(ui, "rename").clicked() {
                        self.sheet.renaming = Some(self.draft.label.clone());
                    }
                }
            }
            if pill_in_title(self.class, self.screen.y, self.draft.pane) {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if rows::verdict_chip(ui, self.draft.report.verdict).clicked() {
                        self.sheet.drawer = !self.sheet.drawer;
                    }
                });
            }
        });
    }

    fn meter_strip(&mut self, ui: &mut egui::Ui) {
        let mut toggle = false;
        let chips = meter_chips(&self.draft.report.meter);
        let verdict = self.draft.report.verdict;
        if meter_scrolls(self.class) {
            egui::ScrollArea::horizontal()
                .id_salt("editor meter")
                .max_height(METER_H)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    ui.horizontal(|ui| {
                        toggle |= rows::verdict_chip(ui, verdict).clicked();
                        for text in &chips {
                            toggle |= chip(ui, text, false).clicked();
                        }
                    });
                });
        } else {
            ui.horizontal_wrapped(|ui| {
                ui.set_min_height(METER_H);
                for text in &chips {
                    toggle |= chip(ui, text, false).clicked();
                }
                toggle |= rows::verdict_chip(ui, verdict).clicked();
            });
        }
        if toggle {
            self.sheet.drawer = !self.sheet.drawer;
        }
    }

    fn findings_drawer(&mut self, ui: &mut egui::Ui, tokens: &Tokens) {
        let findings = self.draft.report.findings.clone();
        let unresolved = self.draft.unresolved.clone();
        if findings.is_empty() && unresolved.is_empty() {
            ui.label(
                egui::RichText::new("nothing to fix")
                    .small()
                    .color(tokens.green),
            );
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt("editor findings")
            .max_height(160.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for finding in &findings {
                    let color = rows::grade_color(tokens, finding.grade);
                    let text = format!(
                        "{} {} · rule {}",
                        rows::grade_glyph(finding.grade),
                        finding.detail,
                        finding.cite
                    );
                    let row = ui.add(
                        egui::Button::new(egui::RichText::new(text).color(color).small())
                            .frame(false)
                            .wrap(),
                    );
                    if row.clicked() {
                        match editor::shortfall_filter(&finding.rule) {
                            Some(filter) => {
                                self.actions.push(Action::Browse(filter, String::new()))
                            }
                            None => {
                                if let Some(first) = finding.cards.first() {
                                    self.draft.scroll_to = Some(first.clone());
                                    self.draft.pane = Pane::List;
                                }
                            }
                        }
                    }
                }
                for name in &unresolved {
                    let row = ui.add(
                        egui::Button::new(
                            egui::RichText::new(format!("? not found: {name}"))
                                .color(tokens.amber)
                                .small(),
                        )
                        .frame(false)
                        .wrap(),
                    );
                    if row.clicked() {
                        self.actions
                            .push(Action::Browse(Filter::default(), name.clone()));
                    }
                }
            });
    }

    fn curve(&mut self, ui: &mut egui::Ui, tokens: &Tokens) {
        let curve = self.draft.curve();
        let peak = curve.iter().copied().max().unwrap_or(0).max(1) as f32;
        let width = ui.available_width().min(320.0);
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, CURVE_H), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        let gap = 3.0;
        let bar_w = (rect.width() - gap * (CURVE_BARS as f32 - 1.0)) / CURVE_BARS as f32;
        let label_h = 12.0;
        let mut hovered = None;
        for (cost, count) in curve.iter().enumerate() {
            let x = rect.min.x + cost as f32 * (bar_w + gap);
            let bar_h = ((*count as f32 / peak) * (rect.height() - label_h)).max(1.0);
            let bar = egui::Rect::from_min_max(
                egui::pos2(x, rect.max.y - label_h - bar_h),
                egui::pos2(x + bar_w, rect.max.y - label_h),
            );
            let column = egui::Rect::from_min_max(
                egui::pos2(x, rect.min.y),
                egui::pos2(x + bar_w, rect.max.y),
            );
            let hot = response.hover_pos().is_some_and(|pos| column.contains(pos));
            if hot {
                hovered = Some((cost, *count));
            }
            let fill = if hot { tokens.ink } else { tokens.ink_weak };
            painter.rect_filled(bar, 2.0, fill);
            let label = if cost == CURVE_BARS - 1 {
                format!("{cost}+")
            } else {
                cost.to_string()
            };
            painter.text(
                egui::pos2(x + bar_w / 2.0, rect.max.y),
                egui::Align2::CENTER_BOTTOM,
                label,
                egui::FontId::proportional(10.0),
                tokens.ink_weak,
            );
        }
        if let Some((cost, count)) = hovered {
            let at = if cost == CURVE_BARS - 1 {
                format!("{cost}+")
            } else {
                cost.to_string()
            };
            response.on_hover_text(format!("{count} cards at {at}"));
        }
    }

    fn panes(&mut self, ui: &mut egui::Ui, layout: Layout, width: f32) {
        let list_w = match layout {
            Layout::TwoPane { list_w } => list_w.min(width * 0.5),
            Layout::Split => (width - ui.spacing().item_spacing.x * 2.0 - 1.0) / 2.0,
            Layout::Segmented => {
                match self.draft.pane {
                    Pane::List => self.list_pane(ui),
                    Pane::Cards => self.browser_pane(ui),
                }
                return;
            }
        };
        let area = ui.max_rect();
        let list_rect = egui::Rect::from_min_size(area.min, egui::vec2(list_w, area.height()));
        let browser_rect = egui::Rect::from_min_max(
            egui::pos2(
                list_rect.max.x + ui.spacing().item_spacing.x * 2.0 + 1.0,
                area.min.y,
            ),
            area.max,
        );
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(list_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
            |ui| {
                ui.set_clip_rect(list_rect.intersect(ui.clip_rect()));
                self.list_pane(ui);
            },
        );
        ui.painter().vline(
            list_rect.max.x + ui.spacing().item_spacing.x,
            area.y_range(),
            egui::Stroke::new(1.0, theme::tokens(ui.ctx()).hairline),
        );
        ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(browser_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
            |ui| {
                ui.set_clip_rect(browser_rect.intersect(ui.clip_rect()));
                self.browser_pane(ui);
            },
        );
    }

    fn browser_pane(&mut self, ui: &mut egui::Ui) {
        let ctx = BrowserContext {
            identity: &self.identity,
            champion_tags: &self.champion_tags,
            copies: &self.copies,
            champion_empty: self.draft.deck.chosen_champion.is_none(),
            class: self.class,
            input: self.input,
        };
        let actions = browser::browser_pane(
            ui,
            &mut self.draft.browser,
            self.catalog,
            self.thumbs,
            self.art,
            &ctx,
        );
        for action in actions {
            if let Some(edit) = browser_edit(self.draft, self.catalog, action) {
                self.actions.push(Action::Edit(edit));
            }
        }
    }

    fn list_pane(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .id_salt(("editor list", self.opens))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.legend_section(ui);
                self.champion_section(ui);
                self.battlefield_section(ui);
                self.rune_section(ui);
                self.main_section(ui);
                self.sideboard_section(ui);
                ui.add_space(8.0);
            });
    }

    fn tile_for(
        &mut self,
        ui: &mut egui::Ui,
        card: Option<&ResolvedCard>,
        empty: &str,
    ) -> egui::Response {
        let size = egui::vec2(battlefield::TILE_W, battlefield::TILE_H);
        match card {
            Some(card) => battlefield::wide_tile(
                ui,
                battlefield::WideTile {
                    size,
                    texture: self.thumbs.id(&card.riftbound_id),
                    name: &card.name,
                    domain: &card.domain,
                    selected: false,
                    dashed: false,
                    portrait: card.kind.as_deref() != Some(agni_riftbound::KIND_BATTLEFIELD),
                },
            ),
            None => battlefield::wide_tile(
                ui,
                battlefield::WideTile {
                    size,
                    texture: None,
                    name: empty,
                    domain: &[],
                    selected: false,
                    dashed: true,
                    portrait: false,
                },
            ),
        }
    }

    fn legend_section(&mut self, ui: &mut egui::Ui) {
        step_title(ui, "legend");
        let legend = self.draft.deck.legend.clone();
        let tile = self.tile_for(ui, legend.as_ref(), "choose a legend");
        let legends = Filter::only_kind(CardKind::Legend);
        if tile.clicked() {
            self.actions
                .push(Action::Browse(legends.clone(), String::new()));
        }
        if let Some(legend) = legend {
            self.flag_row(ui, &legend.riftbound_id);
            ui.horizontal(|ui| {
                if link(ui, "swap").clicked() {
                    self.actions.push(Action::Browse(legends, String::new()));
                }
                if link(ui, "remove").clicked() {
                    self.actions.push(Action::Edit(Edit::ClearLegend));
                }
            });
        }
    }

    fn champion_section(&mut self, ui: &mut egui::Ui) {
        step_title(ui, "champion");
        let champion = self.draft.deck.chosen_champion.clone();
        let tile = self.tile_for(ui, champion.as_ref(), "choose a champion");
        let champions = Filter {
            kinds: [CardKind::Unit].into_iter().collect(),
            champion_only: true,
            fits_identity: true,
            ..Default::default()
        };
        if tile.clicked() {
            self.actions
                .push(Action::Browse(champions.clone(), String::new()));
        }
        if let Some(champion) = champion {
            self.flag_row(ui, &champion.riftbound_id);
            let also = self
                .copies
                .get(&champion.name)
                .copied()
                .unwrap_or(1)
                .saturating_sub(1);
            ui.horizontal(|ui| {
                if link(ui, "swap").clicked() {
                    self.actions.push(Action::Browse(champions, String::new()));
                }
                if link(ui, "remove").clicked() {
                    self.actions.push(Action::Edit(Edit::ClearChampion));
                }
                if also > 0 {
                    ui.label(
                        egui::RichText::new(format!("also in main ×{also}"))
                            .small()
                            .color(theme::tokens(ui.ctx()).ink_weak),
                    );
                }
            });
        }
    }

    fn flag_row(&mut self, ui: &mut egui::Ui, id: &str) {
        if let Some(grade) = self.draft.flagged(id) {
            let tokens = theme::tokens(ui.ctx());
            let detail = self
                .draft
                .report
                .findings
                .iter()
                .find(|finding| finding.cards.iter().any(|card| card == id))
                .map(|finding| finding.detail.clone())
                .unwrap_or_default();
            ui.label(
                egui::RichText::new(format!("{} {detail}", rows::grade_glyph(grade)))
                    .small()
                    .color(rows::grade_color(&tokens, grade)),
            );
        }
        if self.draft.scroll_to.as_deref() == Some(id) {
            ui.scroll_to_cursor(Some(egui::Align::Center));
            self.draft.scroll_to = None;
        }
    }

    fn battlefield_section(&mut self, ui: &mut egui::Ui) {
        step_title(ui, "battlefields");
        let gap = ui.spacing().item_spacing.x;
        let (columns, size) = battlefield::tile_fit(ui.available_width(), gap);
        let fields = self.draft.deck.battlefields.clone();
        let slots: Vec<Option<&DeckEntry>> = (0..agni_riftbound::BATTLEFIELD_COUNT)
            .map(|index| fields.get(index))
            .collect();
        let filter = Filter::only_kind(CardKind::Battlefield);
        for row in slots.chunks(columns.max(1)) {
            ui.horizontal_top(|ui| {
                for slot in row {
                    ui.vertical(|ui| {
                        ui.set_width(size.x);
                        match slot {
                            Some(entry) => {
                                let hit = battlefield::wide_tile(
                                    ui,
                                    battlefield::WideTile {
                                        size,
                                        texture: self.thumbs.id(&entry.card.riftbound_id),
                                        name: &entry.card.name,
                                        domain: &entry.card.domain,
                                        selected: false,
                                        dashed: false,
                                        portrait: false,
                                    },
                                );
                                if hit.clicked() {
                                    self.open_row(&entry.card.riftbound_id, hit.rect);
                                }
                                self.flag_row(ui, &entry.card.riftbound_id);
                                if link(ui, "×").on_hover_text("clear").clicked() {
                                    self.actions.push(Action::Edit(Edit::SetCount {
                                        zone: Zone::Battlefields,
                                        id: entry.card.riftbound_id.clone(),
                                        count: 0,
                                    }));
                                }
                            }
                            None => {
                                let hit = battlefield::wide_tile(
                                    ui,
                                    battlefield::WideTile {
                                        size,
                                        texture: None,
                                        name: "add a battlefield",
                                        domain: &[],
                                        selected: false,
                                        dashed: true,
                                        portrait: false,
                                    },
                                );
                                if hit.clicked() {
                                    self.actions
                                        .push(Action::Browse(filter.clone(), String::new()));
                                }
                            }
                        }
                    });
                }
            });
        }
    }

    fn rune_section(&mut self, ui: &mut egui::Ui) {
        let have = agni_deck::total(&self.draft.deck.runes);
        step_title(ui, &format!("runes · {have}/{RUNE_DECK_SIZE}"));
        let runes: Vec<DeckEntry> = editor::sorted_by_name(&self.draft.deck.runes)
            .into_iter()
            .cloned()
            .collect();
        for entry in &runes {
            self.stepper_row(ui, entry, Zone::Runes, RUNE_DECK_SIZE as u32);
        }
        if have < RUNE_DECK_SIZE as u32 {
            let has_legend = self.draft.deck.legend.is_some();
            let button = ui.add_enabled(has_legend, egui::Button::new("fill runes"));
            if button.clicked() {
                self.actions.push(Action::FillRunes);
            }
            if !has_legend {
                button.on_disabled_hover_text("choose a legend first");
            }
        }
    }

    fn main_section(&mut self, ui: &mut egui::Ui) {
        let main = self.draft.report.meter.main;
        step_title(ui, &format!("main deck · {}/{}", main.0, main.1));
        let tokens = theme::tokens(ui.ctx());
        let groups: Vec<(&'static str, Vec<DeckEntry>)> =
            editor::main_groups(&self.draft.deck.main_deck)
                .into_iter()
                .map(|(name, entries)| (name, entries.into_iter().cloned().collect()))
                .collect();
        if groups.is_empty() {
            ui.label(
                egui::RichText::new("tap a card in the browser to add it")
                    .small()
                    .color(tokens.ink_weak),
            );
        }
        for (name, entries) in groups {
            let count: u32 = entries.iter().map(|entry| entry.count).sum();
            ui.label(
                egui::RichText::new(format!("{name} · {count}"))
                    .small()
                    .color(tokens.ink_weak),
            );
            for entry in &entries {
                self.stepper_row(ui, entry, Zone::Main, COPY_LIMIT);
            }
        }
    }

    fn sideboard_section(&mut self, ui: &mut egui::Ui) {
        let side = self.draft.report.meter.sideboard;
        step_title(ui, &format!("sideboard · {side}"));
        let rows: Vec<DeckEntry> = editor::sorted_by_name(&self.draft.deck.sideboard)
            .into_iter()
            .cloned()
            .collect();
        if rows.is_empty() {
            ui.label(
                egui::RichText::new("nothing benched — a card's detail sends it here")
                    .small()
                    .color(theme::tokens(ui.ctx()).ink_weak),
            );
        }
        for entry in &rows {
            self.stepper_row(ui, entry, Zone::Sideboard, COPY_LIMIT);
            if link(ui, "to main").clicked() {
                self.actions.push(Action::Edit(Edit::ToMain {
                    id: entry.card.riftbound_id.clone(),
                }));
            }
        }
    }

    fn stepper_row(&mut self, ui: &mut egui::Ui, entry: &DeckEntry, zone: Zone, cap: u32) {
        let id = entry.card.riftbound_id.clone();
        let top = ui.cursor().min;
        let event = rows::card_row(
            ui,
            &entry.card,
            entry.count,
            self.thumbs.id(&id),
            self.draft.flagged(&id),
            RowAction::Stepper { cap },
        );
        let rect =
            egui::Rect::from_min_max(top, ui.cursor().min + egui::vec2(ui.available_width(), 0.0));
        if self.draft.scroll_to.as_deref() == Some(id.as_str()) {
            ui.scroll_to_rect(rect, Some(egui::Align::Center));
            self.draft.scroll_to = None;
        }
        match event {
            Some(RowEvent::Plus) => self.actions.push(Action::Edit(Edit::SetCount {
                zone,
                id,
                count: entry.count + 1,
            })),
            Some(RowEvent::Minus) => self.actions.push(Action::Edit(Edit::SetCount {
                zone,
                id,
                count: entry.count.saturating_sub(1),
            })),
            Some(RowEvent::Open) => self.open_row(&id, rect),
            Some(RowEvent::Swap) | None => {}
        }
    }

    fn open_row(&mut self, id: &str, anchor: egui::Rect) {
        match self.catalog.group_of(id) {
            Some(group) => {
                self.draft.browser.open_detail(group, Some(anchor));
                if layout_for(self.class, self.screen) == Layout::Segmented {
                    self.draft.pane = Pane::Cards;
                }
            }
            None => self.actions.push(Action::Status(format!(
                "{id} is not in this catalog — download the full set in Settings › advanced"
            ))),
        }
    }

    fn footer(&mut self, ui: &mut egui::Ui, tokens: &Tokens, footer_h: f32) {
        let (label, enabled) = seat_label(
            self.enforced,
            self.draft.report.verdict,
            self.sheet.seat_armed,
            self.next_game,
        );
        let top = ui.max_rect().min.y;
        ui.painter().hline(
            ui.max_rect().x_range(),
            top,
            egui::Stroke::new(1.0, tokens.hairline),
        );
        ui.add_space(4.0);
        ui.allocate_ui(egui::vec2(ui.available_width(), footer_h - 4.0), |ui| {
            ui.horizontal(|ui| {
                let seat = ui.add_enabled(
                    enabled,
                    egui::Button::new(egui::RichText::new(&label).strong())
                        .fill(tokens.green.gamma_multiply(0.35)),
                );
                if seat.clicked() {
                    if seat_arms(
                        self.draft.report.verdict,
                        self.enforced,
                        self.sheet.seat_armed,
                    ) {
                        self.sheet.seat_armed = true;
                    } else {
                        self.actions.push(Action::Seat {
                            next_game: self.next_game,
                        });
                    }
                }
                if self.enforced {
                    seat.on_hover_text(ENFORCED_NOTE);
                }
                self.save_button(ui);
                let folds = footer_folds_share(self.class);
                if !folds {
                    if ui.button("import").on_hover_text(IMPORT_NOTE).clicked() {
                        self.actions.push(Action::ImportClipboard);
                    }
                    if let Some(status) =
                        exchange::share_menu(ui, &self.draft.deck, &mut self.sheet.qr)
                    {
                        self.actions.push(Action::Status(status));
                    }
                }
                ui.menu_button("more", |ui| {
                    if folds {
                        if ui.button("import from the clipboard").clicked() {
                            self.actions.push(Action::ImportClipboard);
                            ui.close();
                        }
                        if let Some(status) =
                            exchange::share_menu(ui, &self.draft.deck, &mut self.sheet.qr)
                        {
                            self.actions.push(Action::Status(status));
                            ui.close();
                        }
                    }
                    if ui.button("save as copy").clicked() {
                        self.actions.push(Action::SaveCopy);
                        ui.close();
                    }
                    if ui
                        .add_enabled(!self.draft.undo.is_empty(), egui::Button::new("undo"))
                        .clicked()
                    {
                        self.actions.push(Action::Undo);
                        ui.close();
                    }
                    let verb = if self.sheet.discard_armed {
                        "discard changes · are you sure?"
                    } else {
                        "discard changes"
                    };
                    if ui.button(verb).clicked() {
                        if self.sheet.discard_armed {
                            self.sheet.discard_armed = false;
                            self.actions.push(Action::Discard);
                            ui.close();
                        } else {
                            self.sheet.discard_armed = true;
                        }
                    }
                    let clear = if self.sheet.clear_armed {
                        "clear deck · are you sure?"
                    } else {
                        "clear deck"
                    };
                    if ui.button(clear).clicked() {
                        if self.sheet.clear_armed {
                            self.sheet.clear_armed = false;
                            self.actions.push(Action::Edit(Edit::Clear));
                            ui.close();
                        } else {
                            self.sheet.clear_armed = true;
                        }
                    }
                });
                let now = ui.input(|input| input.time);
                if let Some(status) = self.sheet.live_status(now) {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(status).small().color(tokens.ink_weak),
                        )
                        .truncate(),
                    );
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(200));
                }
            });
        });
    }

    fn save_button(&mut self, ui: &mut egui::Ui) {
        let save = ui.add_enabled(self.draft.dirty, egui::Button::new("save"));
        if save.clicked() {
            self.actions.push(Action::Save);
        }
        if cfg!(target_arch = "wasm32") {
            match exchange::render(&self.draft.deck, exchange::Share::DeckCode) {
                Ok(code) => {
                    if let Some(status) = crate::os::clipboard::copy_button(ui, "copy code", &code)
                    {
                        self.actions.push(Action::Status(status));
                    }
                }
                Err(reason) => {
                    ui.add_enabled(false, egui::Button::new("copy code"))
                        .on_disabled_hover_text(reason);
                }
            }
        }
    }
}

pub fn browser_edit(draft: &Draft, catalog: &Catalog, action: BrowserAction) -> Option<Edit> {
    let name_of = |print: usize| catalog.card(print).name.clone();
    Some(match action {
        BrowserAction::Add(print) => Edit::Add(catalog.resolved(print)),
        BrowserAction::SetLegend(print) => Edit::SetLegend(catalog.resolved(print)),
        BrowserAction::SetChampion(print) => Edit::SetChampion(catalog.resolved(print)),
        BrowserAction::ChangePrint(print) => {
            let (zone, id) = draft.locate(&name_of(print))?;
            Edit::ChangePrint {
                zone,
                id,
                card: catalog.resolved(print),
            }
        }
        BrowserAction::ToSideboard(print) => {
            let (zone, id) = draft.locate(&name_of(print))?;
            match zone {
                Zone::Main => Edit::ToSideboard { id },
                Zone::Champion => return None,
                _ => Edit::AddTo {
                    zone: Zone::Sideboard,
                    card: catalog.resolved(print),
                },
            }
        }
        BrowserAction::Remove(print) => {
            let (zone, id) = draft.locate(&name_of(print))?;
            match zone {
                Zone::Legend => Edit::ClearLegend,
                Zone::Champion => Edit::ClearChampion,
                zone => Edit::SetCount { zone, id, count: 0 },
            }
        }
    })
}

fn run_action(action: Action, now: f64, menu: &mut Menu, my_seat: &MySeat, decks: &mut DeckParams) {
    let game_tag = agni_riftbound::GAME.to_string();
    let DeckParams {
        editor,
        catalog,
        sheet,
        seated,
        art,
        import: panel,
        reloads,
        ..
    } = decks;
    let Some(draft) = editor.draft.as_mut() else {
        return;
    };
    match action {
        Action::Edit(edit) => {
            let named = match &edit {
                Edit::Add(card) | Edit::SetLegend(card) | Edit::SetChampion(card) => {
                    Some(card.name.clone())
                }
                _ => None,
            };
            match draft.apply(edit) {
                Ok(placed) => {
                    sheet.seat_armed = false;
                    if let Some(name) = named {
                        let held = draft.copies().get(&name).copied().unwrap_or(0);
                        let text = match placed {
                            Placed::Main => format!("+1 {name} ({held}/{COPY_LIMIT})"),
                            other => format!("{name} · {}", other.label()),
                        };
                        sheet.toast(text, now);
                    }
                }
                Err(reason) => sheet.toast(reason, now),
            }
        }
        Action::Undo => {
            if draft.undo() {
                sheet.toast("undone", now);
            }
        }
        Action::ImportClipboard => start_import(sheet, panel),
        Action::CloseImport => {
            sheet.importing = false;
            panel.clear_resolved();
            panel.error = None;
        }
        Action::Load {
            deck,
            label,
            source,
            unresolved,
        } => {
            draft.load(deck, &label, editor::Origin::Import(source), unresolved);
            sheet.importing = false;
            panel.clear_resolved();
            sheet.toast(format!("loaded {label}"), now);
        }
        Action::SaveImport {
            deck,
            label,
            source,
        } => match history::store::remember_as(&deck, &source, &label) {
            Ok(_) => sheet.status(format!("saved {label} to your decks"), now),
            Err(error) => sheet.status(format!("not saved: {error}"), now),
        },
        Action::FillRunes => {
            let fill = draft.rune_fill(|domain| {
                catalog
                    .basic_rune(domain)
                    .map(|print| catalog.resolved(print))
            });
            match fill.and_then(|runes| draft.apply(Edit::FillRunes(runes))) {
                Ok(_) => sheet.toast("runes filled", now),
                Err(reason) => sheet.toast(reason, now),
            }
        }
        Action::Browse(filter, search) => {
            draft.browser.open_with(filter, &search);
            draft.pane = Pane::Cards;
            sheet.drawer = false;
        }
        Action::Status(text) => sheet.status(text, now),
        Action::Save => match exchange::save(draft, &game_tag) {
            Ok(_) => {
                crate::os::drafts::clear();
                sheet.status(format!("saved {}", draft.label), now);
            }
            Err(error) => sheet.status(format!("not saved: {error}"), now),
        },
        Action::SaveCopy => match exchange::save_as_copy(draft) {
            Ok(_) => {
                crate::os::drafts::clear();
                sheet.status(format!("saved a copy of {}", draft.label), now);
            }
            Err(error) => sheet.status(format!("not saved: {error}"), now),
        },
        Action::Discard => {
            editor.draft = None;
            crate::os::drafts::clear();
            sheet.status = None;
            sheet.toast = None;
            editor::close(editor, menu);
        }
        Action::Seat { next_game } => {
            let saved = if cfg!(target_arch = "wasm32") {
                Err(WEB_SAVE_NOTE.to_string())
            } else {
                exchange::save(draft, &game_tag)
            };
            let label = draft.label.clone();
            let deck = import::ImportedDeck::Riftbound(draft.deck.clone());
            let faces = import::seat_deck(deck, my_seat.0, seated, art, None);
            panel.auto_deal = true;
            panel.error = None;
            let note = match saved {
                Ok(_) => format!("seated {label} — saved, {faces} faces staged for the deal"),
                Err(error) => format!("seated {label} — {faces} faces staged ({error})"),
            };
            panel.note = Some(note);
            if next_game {
                reloads.write(sideboard::ReloadDeckRequested);
            }
            sheet.seat_armed = false;
            history::set_note(format!("{label} seated from the editor"));
            seated_leave(menu);
        }
    }
}

pub fn seated_leave(menu: &mut Menu) {
    menu.leave_decks();
    if !matches!(
        menu.screen,
        Screen::Lobby(TableGame::Riftbound) | Screen::Table
    ) {
        menu.open_lobby(TableGame::Riftbound);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::editor::Origin;
    use agni_riftbound::legality::Meter;

    #[test]
    fn seating_from_the_editor_lands_where_the_deck_plays() {
        let mut menu = Menu::default();
        menu.open_lobby(TableGame::Riftbound);
        menu.open_editor();
        seated_leave(&mut menu);
        assert_eq!(menu.screen, Screen::Lobby(TableGame::Riftbound));
        let mut from_home = Menu::default();
        from_home.open_editor();
        seated_leave(&mut from_home);
        assert_eq!(
            from_home.screen,
            Screen::Lobby(TableGame::Riftbound),
            "opened from home, seating opens the Riftbound lobby"
        );
        let mut from_mtg = Menu::default();
        from_mtg.open_lobby(TableGame::Mtg);
        from_mtg.open_decks();
        seated_leave(&mut from_mtg);
        assert_eq!(from_mtg.screen, Screen::Lobby(TableGame::Riftbound));
        let mut at_table = Menu::default();
        at_table.open_lobby(TableGame::Riftbound);
        at_table.screen = Screen::Table;
        at_table.open_editor();
        seated_leave(&mut at_table);
        assert_eq!(
            at_table.screen,
            Screen::Table,
            "a live table takes the deck back"
        );
    }

    #[test]
    fn the_layout_follows_the_class_and_the_screen() {
        let screen = egui::vec2;
        assert_eq!(
            layout_for(ViewportClass::Desktop, screen(1280.0, 800.0)),
            Layout::TwoPane { list_w: 360.0 }
        );
        assert_eq!(
            layout_for(ViewportClass::Tablet, screen(650.0, 900.0)),
            Layout::Segmented,
            "a screen under 700 takes the phone-portrait layout"
        );
        assert_eq!(
            layout_for(ViewportClass::Tablet, screen(1024.0, 768.0)),
            Layout::TwoPane { list_w: 300.0 }
        );
        assert_eq!(
            layout_for(ViewportClass::Tablet, screen(800.0, 480.0)),
            Layout::Split,
            "a short tablet screen is treated like phone landscape"
        );
        assert_eq!(
            layout_for(ViewportClass::PhonePortrait, screen(360.0, 640.0)),
            Layout::Segmented
        );
        assert_eq!(
            layout_for(ViewportClass::PhoneLandscape, screen(780.0, 360.0)),
            Layout::Split
        );
        assert_eq!(footer_height(ViewportClass::PhonePortrait), 56.0);
        assert_eq!(footer_height(ViewportClass::Desktop), 44.0);
        assert!(shows_curve(ViewportClass::Desktop, 800.0));
        assert!(!shows_curve(ViewportClass::PhoneLandscape, 360.0));
        assert!(!shows_curve(ViewportClass::Tablet, 480.0));
        assert!(shows_curve(ViewportClass::Tablet, 768.0));
        assert!(meter_only_pill(ViewportClass::PhoneLandscape, 360.0));
        assert!(meter_only_pill(ViewportClass::Tablet, 480.0));
        assert!(!meter_only_pill(ViewportClass::PhonePortrait, 640.0));
        assert!(
            pill_in_title(ViewportClass::PhonePortrait, 640.0, Pane::Cards),
            "the cards pane gives the grid the meter's row"
        );
        assert!(!pill_in_title(
            ViewportClass::PhonePortrait,
            640.0,
            Pane::List
        ));
        assert!(!pill_in_title(ViewportClass::Desktop, 800.0, Pane::Cards));
        assert!(shows_curve_line(ViewportClass::PhonePortrait, Pane::List));
        assert!(!shows_curve_line(ViewportClass::PhonePortrait, Pane::Cards));
        assert!(meter_scrolls(ViewportClass::PhonePortrait));
        assert!(!meter_scrolls(ViewportClass::Tablet));
        assert!(footer_folds_share(ViewportClass::PhonePortrait));
        assert!(!footer_folds_share(ViewportClass::Desktop));
    }

    #[test]
    fn the_meter_reads_each_slot_and_the_curve_folds_into_one_line() {
        let meter = Meter {
            legend: true,
            champion: false,
            main: (37, 40),
            runes: (12, 12),
            battlefields: (2, 3),
            signatures: (1, 3),
            sideboard: 4,
        };
        assert_eq!(
            meter_chips(&meter),
            [
                "legend ✔",
                "champion –",
                "main 37/40",
                "runes 12/12",
                "fields 2/3",
                "sig 1/3"
            ]
        );
        assert_eq!(
            curve_line(&[0, 2, 6, 9, 8, 4, 3, 1]),
            "curve 0 · 2 · 6 · 9 · 8 · 4 · 3 · 1"
        );
    }

    #[test]
    fn the_seat_button_names_the_problems_and_arms_on_a_free_table() {
        assert_eq!(
            seat_label(true, Verdict::Broken(3), false, false),
            ("seat · 3 problems".into(), false)
        );
        assert_eq!(
            seat_label(false, Verdict::Broken(3), false, false),
            ("seat · 3 problems".into(), true)
        );
        assert!(seat_arms(Verdict::Broken(3), false, false));
        assert!(!seat_arms(Verdict::Broken(3), true, false));
        assert!(!seat_arms(Verdict::Legal, false, false));
        assert_eq!(
            seat_label(false, Verdict::Broken(1), true, false),
            ("seat anyway".into(), true)
        );
        assert_eq!(
            seat_label(true, Verdict::Broken(1), true, false),
            ("seat · 1 problem".into(), false),
            "arming never beats enforcement"
        );
        assert_eq!(
            seat_label(false, Verdict::Unverified, false, true),
            ("use this list next game".into(), true)
        );
        assert_eq!(
            seat_label(true, Verdict::Legal, false, false),
            ("seat this deck".into(), true)
        );
    }

    #[test]
    fn a_toast_lives_a_second_and_a_half() {
        let mut sheet = EditorSheet::default();
        assert_eq!(sheet.live_toast(10.0), None);
        sheet.toast("+1 Lonely Poro (2/3)", 10.0);
        assert_eq!(sheet.live_toast(11.0), Some("+1 Lonely Poro (2/3)"));
        assert_eq!(sheet.live_toast(11.6), None);
        assert!(sheet.toast.is_none());
        sheet.status("copied to clipboard", 20.0);
        sheet.toast("+1 Charm (1/3)", 20.5);
        assert_eq!(sheet.live_toast(21.0), Some("+1 Charm (1/3)"));
        assert_eq!(
            sheet.live_status(21.0),
            Some("copied to clipboard"),
            "the status lives in the footer beside the toast"
        );
        assert_eq!(sheet.live_toast(22.5), None);
        assert_eq!(sheet.live_status(22.5), Some("copied to clipboard"));
        assert_eq!(sheet.live_status(24.5), None);
        sheet.drawer = true;
        sheet.clear_armed = true;
        sheet.begin_open(3);
        assert!(
            !sheet.drawer && !sheet.clear_armed,
            "a fresh open resets the sheet"
        );
        sheet.drawer = true;
        sheet.begin_open(3);
        assert!(sheet.drawer, "the same open keeps its state");
    }

    #[test]
    fn shortfall_rules_open_the_browser_and_card_rules_scroll_the_list() {
        assert!(is_shortfall(&Rule::LegendMissing));
        assert!(is_shortfall(&Rule::MainSize { have: 3 }));
        assert!(!is_shortfall(&Rule::OutOfIdentity { name: "x".into() }));
        assert!(!is_shortfall(&Rule::TagsUnverified));
    }

    fn catalog() -> Catalog {
        crate::deck::catalog::pool_catalog()
    }

    #[test]
    fn browser_actions_become_edits_against_the_draft() {
        let catalog = catalog();
        let poro = catalog
            .find_name("Lonely Poro")
            .expect("the pool knows the poro");
        let print = catalog.groups[poro].default_print;
        let mut draft = Draft::new("t", Origin::New);
        let edit = browser_edit(&draft, &catalog, BrowserAction::Add(print)).unwrap();
        assert!(matches!(edit, Edit::Add(ref card) if card.name == "Lonely Poro"));
        draft.apply(edit).unwrap();
        let side = browser_edit(&draft, &catalog, BrowserAction::ToSideboard(print)).unwrap();
        assert!(matches!(side, Edit::ToSideboard { .. }));
        draft.apply(side).unwrap();
        let back = browser_edit(&draft, &catalog, BrowserAction::ToSideboard(print)).unwrap();
        assert!(
            matches!(
                back,
                Edit::AddTo {
                    zone: Zone::Sideboard,
                    ..
                }
            ),
            "a card only in the sideboard gets another copy there"
        );
        let remove = browser_edit(&draft, &catalog, BrowserAction::Remove(print)).unwrap();
        assert!(matches!(
            remove,
            Edit::SetCount {
                zone: Zone::Sideboard,
                count: 0,
                ..
            }
        ));
        draft.apply(remove).unwrap();
        assert!(
            browser_edit(&draft, &catalog, BrowserAction::Remove(print)).is_none(),
            "nothing left to remove"
        );
        let legend = catalog.find_name("Lillia - Bashful Bloom").unwrap();
        let legend_print = catalog.groups[legend].default_print;
        draft
            .apply(browser_edit(&draft, &catalog, BrowserAction::SetLegend(legend_print)).unwrap())
            .unwrap();
        assert!(draft.deck.legend.is_some());
        let remove = browser_edit(&draft, &catalog, BrowserAction::Remove(legend_print)).unwrap();
        assert_eq!(remove, Edit::ClearLegend);
    }

    fn run_frame(
        context: &egui::Context,
        size: egui::Vec2,
        class: ViewportClass,
        draft: &mut Draft,
        sheet: &mut EditorSheet,
        catalog: &Catalog,
    ) -> egui::FullOutput {
        let mut thumbs = Thumbs::default();
        let mut art = ArtCache::default();
        let mut actions = Vec::new();
        let mut output = None;
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            context.begin_pass(input);
            let copies = draft.copies();
            let identity = draft.identity();
            let champion_tags = draft.champion_tags();
            let mut frame = Frame {
                draft,
                catalog,
                thumbs: &mut thumbs,
                art: &mut art,
                sheet,
                class,
                input: InputKind::assumed(),
                enforced: false,
                next_game: false,
                screen: size,
                opens: 1,
                copies,
                identity,
                champion_tags,
                actions: &mut actions,
            };
            egui::Area::new(egui::Id::new("editor test"))
                .fixed_pos(egui::Pos2::ZERO)
                .show(context, |ui| {
                    ui.set_min_size(size);
                    ui.set_max_size(size);
                    frame.body(ui);
                });
            let mut full = context.end_pass();
            full.textures_delta.clear();
            output = Some(full);
        }
        output.expect("two passes ran")
    }

    fn texts(output: &egui::FullOutput) -> Vec<(String, egui::Rect)> {
        output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Text(text) => Some((
                    text.galley.text().to_string(),
                    text.galley.rect.translate(text.pos.to_vec2()),
                )),
                _ => None,
            })
            .collect()
    }

    fn find(texts: &[(String, egui::Rect)], needle: &str) -> Option<egui::Rect> {
        texts
            .iter()
            .find(|(text, _)| text.contains(needle))
            .map(|(_, rect)| *rect)
    }

    #[test]
    fn the_sheet_keeps_meter_and_footer_inside_every_screen() {
        let catalog = catalog();
        let cases = [
            (egui::vec2(360.0, 780.0), ViewportClass::PhonePortrait),
            (egui::vec2(780.0, 360.0), ViewportClass::PhoneLandscape),
            (egui::vec2(1024.0, 768.0), ViewportClass::Tablet),
            (egui::vec2(1280.0, 800.0), ViewportClass::Desktop),
        ];
        for (size, class) in cases {
            let context = egui::Context::default();
            let mut draft = Draft::from_deck(
                crate::deck::pool::deck("lillia-house").unwrap(),
                "Lillia (house) (copy)",
                Origin::Pool("lillia-house".into()),
            );
            let mut sheet = EditorSheet::default();
            let output = run_frame(&context, size, class, &mut draft, &mut sheet, &catalog);
            let texts = texts(&output);
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            let seat = find(&texts, "seat this deck")
                .unwrap_or_else(|| panic!("{class:?}: the seat button is drawn"));
            assert!(
                screen.contains_rect(seat.shrink(0.5)),
                "{class:?}: the footer sits inside the screen: {seat:?} in {screen:?}"
            );
            let pill = find(&texts, "problems")
                .or_else(|| find(&texts, "unverified"))
                .or_else(|| find(&texts, "legal"))
                .unwrap_or_else(|| panic!("{class:?}: the verdict pill is drawn"));
            assert!(
                screen.contains_rect(pill.shrink(0.5)),
                "{class:?}: the meter sits inside the screen: {pill:?}"
            );
            let has_deck_tab = find(&texts, "deck").is_some() && find(&texts, "cards").is_some();
            match layout_for(class, size) {
                Layout::Segmented => {
                    assert!(
                        has_deck_tab,
                        "{class:?}: the segmented row shows deck · cards"
                    );
                    assert!(
                        find(&texts, "legend").is_some(),
                        "{class:?}: the list pane opens first"
                    );
                    assert!(
                        find(&texts, "search name").is_none(),
                        "{class:?}: one pane at a time"
                    );
                }
                Layout::TwoPane { .. } | Layout::Split => {
                    assert!(
                        find(&texts, "legend").is_some() && find(&texts, "search name").is_some(),
                        "{class:?}: both panes are on screen"
                    );
                }
            }
            if meter_only_pill(class, size.y) {
                assert!(
                    find(&texts, "runes 12/12").is_none(),
                    "{class:?}: landscape keeps only the verdict pill"
                );
            } else {
                assert!(
                    find(&texts, "runes 12/12").is_some(),
                    "{class:?}: the meter strip is drawn"
                );
            }
            if footer_folds_share(class) {
                assert!(
                    find(&texts, "share").is_none(),
                    "{class:?}: share folds into more"
                );
            } else {
                assert!(
                    find(&texts, "share").is_some(),
                    "{class:?}: share sits in the footer"
                );
            }
        }
    }

    #[test]
    fn the_desktop_editor_fills_the_screen_with_the_list_pane_at_360() {
        let catalog = catalog();
        let context = egui::Context::default();
        let mut draft = Draft::new("new deck", Origin::New);
        let mut sheet = EditorSheet::default();
        let output = run_frame(
            &context,
            egui::vec2(1280.0, 800.0),
            ViewportClass::Desktop,
            &mut draft,
            &mut sheet,
            &catalog,
        );
        let rects: Vec<egui::Rect> = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Rect(rect) => Some(rect.rect),
                _ => None,
            })
            .collect();
        assert!(
            !rects
                .iter()
                .any(|rect| (rect.width() - 760.0).abs() <= 2.0 && rect.min.x >= 519.0),
            "the editor is a screen, not a 760 pt sheet on the right: {rects:?}"
        );
        let texts = texts(&output);
        let legend = find(&texts, "legend").expect("the list pane shows the legend section");
        let search = find(&texts, "search name").expect("the browser pane shows its search");
        assert!(
            legend.min.x < search.min.x,
            "the list is left of the browser"
        );
        assert!(
            search.min.x >= LIST_W_DESKTOP - 8.0,
            "the browser starts past the list pane: {search:?}"
        );
        assert!(
            rects
                .iter()
                .any(|rect| rect.min.x >= LIST_W_DESKTOP && rect.max.x > 1000.0),
            "the browser pane takes the width a screen gives it: {rects:?}"
        );
    }
}
