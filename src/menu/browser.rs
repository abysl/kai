use crate::deck::catalog::{Catalog, EnergyBand, Filter, Group};
use crate::deck::thumbs::Thumbs;
use crate::menu::{chip, float_note, link, TOUCH};
use crate::render::art::{self, ArtCache};
use crate::theme::{self, domain_color, Tokens};
use crate::viewport::{InputKind, ViewportClass};
use agni_importers::riftbound::card_code::CardCode;
use agni_importers::riftbound::catalog::CardKind;
use agni_riftbound::legality::COPY_LIMIT;
use bevy_egui::egui;
use std::collections::{BTreeMap, BTreeSet};

pub const TILE_MIN_W_CARD: f32 = 92.0;
pub const TILE_MAX_W_CARD: f32 = 150.0;
pub const CARD_ASPECT: f32 = 1039.0 / 744.0;
pub const WIDE_ASPECT: f32 = 744.0 / 1039.0;

pub fn is_wide(kind: CardKind) -> bool {
    kind == CardKind::Battlefield
}

pub fn art_box(cell: egui::Vec2, wide: bool) -> egui::Vec2 {
    if wide {
        egui::vec2(cell.x, cell.x * WIDE_ASPECT)
    } else {
        cell
    }
}
pub const CAPTION_H: f32 = 32.0;
pub const CAPTION_H_PHONE: f32 = 48.0;
pub const TILE_GAP_Y: f32 = 2.0;
pub const ART_REQUESTS_PER_FRAME: usize = 8;
pub const NOTE_SECS: f64 = 1.5;
pub const DETAIL_W: f32 = 220.0;
pub const DETAIL_ART_SHARE: f32 = 0.6;
pub const DETAIL_MAX_SHARE: f32 = 0.85;
pub const SEARCH_HINT: &str = "search name or text";
pub const COPY_LIMIT_NOTE: &str = "three copies is the limit — rule 103.2.b";
pub const CHAMPION_OFFER_NOTE: &str =
    "this champion does not share your legend's tag — the verdict will say so";
pub const CAPTION_LEGEND: &str = "set as legend";
pub const CAPTION_CHAMPION: &str = "★ champion";

pub const KIND_CHIPS: [(CardKind, &str); 6] = [
    (CardKind::Unit, "units"),
    (CardKind::Spell, "spells"),
    (CardKind::Gear, "gear"),
    (CardKind::Rune, "runes"),
    (CardKind::Battlefield, "fields"),
    (CardKind::Legend, "legends"),
];

pub fn tile_columns_for(available: f32, gap: f32, min_w: f32) -> usize {
    (((available + gap) / (min_w + gap)).floor() as usize).max(1)
}

pub fn tile_size_for(available: f32, gap: f32, columns: usize) -> egui::Vec2 {
    let width = ((available - gap * (columns.saturating_sub(1)) as f32) / columns as f32)
        .clamp(1.0, TILE_MAX_W_CARD);
    egui::vec2(width, width * CARD_ASPECT)
}

pub fn caption_height(class: ViewportClass) -> f32 {
    if class.is_phone() {
        CAPTION_H_PHONE
    } else {
        CAPTION_H
    }
}

pub fn row_height(tile: egui::Vec2, class: ViewportClass) -> f32 {
    tile.y + TILE_GAP_Y + caption_height(class)
}

pub fn chips_wrap(class: ViewportClass) -> bool {
    !class.is_phone()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserAction {
    Add(usize),
    SetLegend(usize),
    SetChampion(usize),
    ChangePrint(usize),
    ToSideboard(usize),
    Remove(usize),
}

pub struct BrowserContext<'a> {
    pub identity: &'a [String],
    pub champion_tags: &'a [String],
    pub copies: &'a BTreeMap<String, u32>,
    pub champion_empty: bool,
    pub class: ViewportClass,
    pub input: InputKind,
}

impl BrowserContext<'_> {
    pub fn copies_of(&self, name: &str) -> u32 {
        self.copies.get(name).copied().unwrap_or(0)
    }

    pub fn hover_details(&self) -> bool {
        !self.class.is_phone() && !self.input.is_touch()
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Cache {
    filter: Filter,
    search: String,
    identity: Vec<String>,
    generation: u32,
    groups: Vec<usize>,
}

#[derive(Default)]
pub struct BrowserState {
    pub search: String,
    pub filter: Filter,
    pub detail: Option<usize>,
    pub note: Option<(String, f64)>,
    pub visible: Vec<usize>,
    pub focus_search: bool,
    cache: Option<Cache>,
    requested: BTreeSet<String>,
    detail_anchor: Option<egui::Rect>,
    identity_armed: bool,
    shown: bool,
}

impl BrowserState {
    pub fn with_filter(filter: Filter, search: &str) -> Self {
        Self {
            filter,
            search: search.to_string(),
            ..Default::default()
        }
    }

    pub fn open_with(&mut self, filter: Filter, search: &str) {
        self.filter = filter;
        self.search = search.to_string();
        self.detail = None;
        self.cache = None;
    }

    pub fn set_note(&mut self, text: impl Into<String>, now: f64) {
        self.note = Some((text.into(), now));
    }

    pub fn live_note(&mut self, now: f64) -> Option<&str> {
        if self
            .note
            .as_ref()
            .is_some_and(|(_, since)| now - since > NOTE_SECS)
        {
            self.note = None;
        }
        self.note.as_ref().map(|(text, _)| text.as_str())
    }

    pub fn reset_filters(&mut self) {
        self.filter = Filter::default();
        self.identity_armed = false;
    }

    pub fn arm_identity(&mut self, identity: &[String]) {
        if identity.is_empty() {
            self.identity_armed = false;
        } else if !self.identity_armed {
            self.filter.fits_identity = true;
            self.identity_armed = true;
        }
    }

    pub fn results(&mut self, catalog: &Catalog, identity: &[String]) -> &[usize] {
        let stale = self.cache.as_ref().is_none_or(|cache| {
            cache.generation != catalog.generation
                || cache.filter != self.filter
                || cache.search != self.search
                || cache.identity != identity
        });
        if stale {
            self.cache = Some(Cache {
                filter: self.filter.clone(),
                search: self.search.clone(),
                identity: identity.to_vec(),
                generation: catalog.generation,
                groups: catalog.filter(&self.filter, &self.search, identity),
            });
        }
        self.cache
            .as_ref()
            .map(|cache| cache.groups.as_slice())
            .unwrap_or(&[])
    }

    pub fn open_detail(&mut self, group: usize, anchor: Option<egui::Rect>) {
        self.detail = Some(group);
        self.detail_anchor = anchor;
    }

    pub fn close_detail(&mut self) {
        self.detail = None;
        self.detail_anchor = None;
    }
}

pub fn shares_tag(left: &[String], right: &[String]) -> bool {
    left.iter()
        .any(|tag| right.iter().any(|other| other.eq_ignore_ascii_case(tag)))
}

pub fn champion_cue(group: &Group, ctx: &BrowserContext) -> bool {
    group.kind == CardKind::Unit
        && group.champion
        && ctx.champion_empty
        && (ctx.champion_tags.is_empty()
            || shares_tag(&group.tags, ctx.champion_tags)
            || (group.tags.is_empty() && shares_stem(&group.name, ctx.champion_tags)))
}

pub fn champion_offer(group: &Group, ctx: &BrowserContext) -> bool {
    group.kind == CardKind::Unit
        && group.champion
        && ctx.champion_empty
        && !champion_cue(group, ctx)
}

pub fn shares_stem(name: &str, champion_tags: &[String]) -> bool {
    let stem = agni_riftbound::legality::name_stem(name);
    stem != name.trim()
        && champion_tags
            .iter()
            .any(|tag| tag.eq_ignore_ascii_case(stem))
}

pub fn tap_action(group: &Group, ctx: &BrowserContext) -> Result<BrowserAction, String> {
    let print = group.default_print;
    match group.kind {
        CardKind::Legend => Ok(BrowserAction::SetLegend(print)),
        CardKind::Unit if champion_cue(group, ctx) => Ok(BrowserAction::SetChampion(print)),
        CardKind::Unit | CardKind::Spell | CardKind::Gear => {
            if ctx.copies_of(&group.name) >= COPY_LIMIT {
                Err(COPY_LIMIT_NOTE.to_string())
            } else {
                Ok(BrowserAction::Add(print))
            }
        }
        CardKind::Battlefield => {
            if ctx.copies_of(&group.name) >= 1 {
                Err(format!(
                    "{} is already one of your battlefields",
                    group.name
                ))
            } else {
                Ok(BrowserAction::Add(print))
            }
        }
        CardKind::Rune | CardKind::Other => Ok(BrowserAction::Add(print)),
    }
}

pub fn badge_text(group: &Group, copies: u32) -> Option<String> {
    if copies == 0 {
        return None;
    }
    Some(if group.in_deck_kind() {
        format!("{copies}/{COPY_LIMIT}")
    } else {
        copies.to_string()
    })
}

pub fn capped(group: &Group, copies: u32) -> bool {
    (group.in_deck_kind() && copies >= COPY_LIMIT)
        || (group.kind == CardKind::Battlefield && copies >= 1)
}

pub fn caption_text(group: &Group, ctx: &BrowserContext) -> String {
    match group.kind {
        CardKind::Legend => CAPTION_LEGEND.to_string(),
        _ if champion_cue(group, ctx) => CAPTION_CHAMPION.to_string(),
        _ => group.name.clone(),
    }
}

pub fn add_label(group: &Group, ctx: &BrowserContext) -> &'static str {
    match group.kind {
        CardKind::Legend => "set as legend",
        CardKind::Unit if champion_cue(group, ctx) => "set as champion",
        _ => "add",
    }
}

pub fn stats_line(group: &Group) -> String {
    let mut parts = Vec::new();
    if let Some(energy) = group.energy {
        parts.push(format!("{energy} energy"));
    }
    if let Some(power) = group.power {
        parts.push(format!("{power} power"));
    }
    if let Some(might) = group.might {
        parts.push(format!("{might} might"));
    }
    parts.join(" · ")
}

pub fn print_label(riftbound_id: &str, set_id: Option<&str>) -> String {
    match CardCode::from_riftbound_id(riftbound_id) {
        Ok(code) => match set_id {
            Some(set) if set != code.set.as_str() => format!("{set} · {code}"),
            _ => code.to_string().replacen('-', " · ", 1),
        },
        Err(_) => riftbound_id.to_string(),
    }
}

pub const PIP_R: f32 = 5.0;

fn domain_chip(ui: &mut egui::Ui, tokens: &Tokens, domain: &str, selected: bool) -> egui::Response {
    let response = chip(ui, &format!("    {}", domain.to_lowercase()), selected);
    let pad = ui.spacing().button_padding.x;
    let center = egui::pos2(
        response.rect.min.x + pad + PIP_R + 1.0,
        response.rect.center().y,
    );
    ui.painter()
        .circle_filled(center, PIP_R, domain_color(tokens, domain));
    ui.painter()
        .circle_stroke(center, PIP_R, egui::Stroke::new(1.0, tokens.hairline));
    response
}

fn paint_pips(
    painter: &egui::Painter,
    tokens: &Tokens,
    domains: &[String],
    at: egui::Pos2,
    radius: f32,
) {
    let step = radius * 2.0 + 3.0;
    for (index, domain) in domains
        .iter()
        .filter(|domain| !domain.eq_ignore_ascii_case(theme::COLORLESS))
        .enumerate()
    {
        let center = at + egui::vec2(radius + step * index as f32, 0.0);
        painter.circle_filled(center, radius, domain_color(tokens, domain));
        painter.circle_stroke(center, radius, egui::Stroke::new(1.0, tokens.hairline));
    }
}

fn pips_inline(ui: &mut egui::Ui, tokens: &Tokens, domains: &[String]) {
    let shown = domains
        .iter()
        .filter(|domain| !domain.eq_ignore_ascii_case(theme::COLORLESS))
        .count();
    if shown == 0 {
        return;
    }
    let radius = 5.0;
    let width = shown as f32 * (radius * 2.0 + 3.0);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(width, radius * 2.0 + 2.0), egui::Sense::hover());
    paint_pips(
        ui.painter(),
        tokens,
        domains,
        egui::pos2(rect.min.x, rect.center().y),
        radius,
    );
}

fn chips_row(
    ui: &mut egui::Ui,
    state: &mut BrowserState,
    catalog: &Catalog,
    ctx: &BrowserContext,
    tokens: &Tokens,
) {
    let identity_chip = |ui: &mut egui::Ui, state: &mut BrowserState| {
        if chip(ui, "fits identity", state.filter.fits_identity)
            .on_hover_text("only cards inside your legend's domains")
            .clicked()
        {
            state.filter.fits_identity = !state.filter.fits_identity;
        }
    };
    let shows_identity = !ctx.identity.is_empty();
    if shows_identity && state.filter.fits_identity {
        identity_chip(ui, state);
    }
    for (kind, label) in KIND_CHIPS {
        if chip(ui, label, state.filter.kinds.contains(&kind)).clicked() {
            state.filter.toggle_kind(kind);
        }
    }
    for domain in catalog.domain_names() {
        if domain_chip(ui, tokens, domain, state.filter.domains.contains(*domain)).clicked() {
            state.filter.toggle_domain(domain);
        }
    }
    if shows_identity && !state.filter.fits_identity {
        identity_chip(ui, state);
    }
    for band in EnergyBand::ALL {
        if chip(ui, band.label(), state.filter.energy == Some(band)).clicked() {
            state.filter.toggle_energy(band);
        }
    }
    ui.menu_button("more", |ui| {
        ui.checkbox(&mut state.filter.champion_only, "champions only");
        ui.checkbox(&mut state.filter.signature_only, "signatures only");
        let sets = catalog.sets();
        if !sets.is_empty() {
            ui.separator();
            for set in sets {
                let mut on = state.filter.sets.contains(set);
                if ui.checkbox(&mut on, set).changed() {
                    state.filter.toggle_set(set);
                }
            }
        }
        ui.separator();
        if ui.button("reset filters").clicked() {
            state.reset_filters();
            state.search.clear();
            ui.close();
        }
    });
}

fn search_id() -> egui::Id {
    egui::Id::new("browser search")
}

fn search_row(ui: &mut egui::Ui, state: &mut BrowserState, ctx: &BrowserContext) {
    let focus_now = !ctx.class.is_phone()
        && (state.focus_search
            || ui.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::F))
            || (ui.memory(|memory| memory.focused().is_none())
                && ui.input(|input| {
                    input
                        .events
                        .iter()
                        .any(|event| matches!(event, egui::Event::Text(_)))
                })));
    state.focus_search = false;
    if focus_now {
        ui.memory_mut(|memory| memory.request_focus(search_id()));
    }
    let clear_w = TOUCH;
    ui.horizontal(|ui| {
        let width = (ui.available_width() - clear_w - ui.spacing().item_spacing.x).max(80.0);
        ui.add_sized(
            egui::vec2(width, ui.spacing().interact_size.y),
            egui::TextEdit::singleline(&mut state.search)
                .id(search_id())
                .hint_text(SEARCH_HINT),
        );
        if ui
            .add_enabled(
                !state.search.is_empty(),
                egui::Button::new("×").min_size(egui::vec2(clear_w, ui.spacing().interact_size.y)),
            )
            .clicked()
        {
            state.search.clear();
        }
    });
}

pub const COST_PIP_R: f32 = 10.0;

fn cost_fill(tokens: &Tokens, domains: &[String]) -> egui::Color32 {
    domains
        .iter()
        .find(|domain| !domain.eq_ignore_ascii_case(theme::COLORLESS))
        .map(|domain| domain_color(tokens, domain))
        .unwrap_or(tokens.surface_opaque())
}

fn placeholder(
    ui: &mut egui::Ui,
    tokens: &Tokens,
    group: &Group,
    size: egui::Vec2,
    dim: bool,
    named: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let painter = ui.painter();
    let fill = if dim { tokens.grey } else { tokens.surface_2 };
    painter.rect_filled(rect, 6.0, fill);
    painter.rect_stroke(
        rect,
        6.0,
        egui::Stroke::new(1.0, tokens.hairline),
        egui::StrokeKind::Inside,
    );
    let font = egui::FontId::proportional((size.x / 8.0).clamp(11.0, 14.0));
    if named {
        let galley = painter.layout(
            group.name.clone(),
            font.clone(),
            tokens.ink,
            (size.x - 10.0).max(20.0),
        );
        let text_pos = egui::pos2(
            rect.center().x - galley.size().x / 2.0,
            (rect.center().y - galley.size().y / 2.0).max(rect.min.y + 6.0),
        );
        painter.galley(text_pos, galley, tokens.ink);
    }
    if let Some(energy) = group.energy {
        let center = rect.min + egui::vec2(COST_PIP_R + 4.0, COST_PIP_R + 4.0);
        painter.circle_filled(center, COST_PIP_R, cost_fill(tokens, &group.domain));
        painter.circle_stroke(center, COST_PIP_R, egui::Stroke::new(1.0, tokens.hairline));
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            energy.to_string(),
            egui::FontId::proportional(12.0),
            egui::Color32::WHITE,
        );
    }
    paint_pips(
        painter,
        tokens,
        &group.domain,
        egui::pos2(rect.min.x + 6.0, rect.max.y - 9.0),
        4.0,
    );
    response
}

fn art_tile(
    ui: &mut egui::Ui,
    texture: egui::TextureId,
    size: egui::Vec2,
    dim: bool,
    wide: bool,
) -> egui::Response {
    let art = art_box(size, wide);
    let mut image = egui::Image::new(egui::load::SizedTexture::new(texture, art))
        .corner_radius(6.0)
        .fit_to_exact_size(art);
    if dim {
        image = image.tint(egui::Color32::from_gray(110));
    }
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(0.0, (size.y - art.y) / 2.0);
        ui.add(egui::Button::image(image).frame(false))
    })
    .inner
}

fn paint_badge(painter: &egui::Painter, tokens: &Tokens, rect: egui::Rect, text: &str) {
    let font = egui::FontId::proportional(12.0);
    let galley = painter.layout_no_wrap(text.to_string(), font, tokens.ink);
    let pad = egui::vec2(6.0, 3.0);
    let badge = egui::Rect::from_min_size(
        egui::pos2(
            rect.max.x - galley.size().x - pad.x * 2.0 - 4.0,
            rect.min.y + 4.0,
        ),
        galley.size() + pad * 2.0,
    );
    painter.rect_filled(badge, 8.0, tokens.surface_opaque());
    painter.rect_stroke(
        badge,
        8.0,
        egui::Stroke::new(1.0, tokens.hairline),
        egui::StrokeKind::Inside,
    );
    painter.galley(badge.min + pad, galley, tokens.ink);
}

fn hover_summary(ui: &mut egui::Ui, tokens: &Tokens, catalog: &Catalog, group: &Group) {
    ui.set_max_width(DETAIL_W + 40.0);
    ui.label(egui::RichText::new(&group.name).strong().color(tokens.ink));
    let stats = stats_line(group);
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "{}{}",
                group.kind.as_str().to_lowercase(),
                if stats.is_empty() {
                    String::new()
                } else {
                    format!(" · {stats}")
                }
            ))
            .color(tokens.ink_weak)
            .small(),
        );
        pips_inline(ui, tokens, &group.domain);
    });
    if let Some(text) = catalog.card(group.default_print).text.as_deref() {
        ui.label(egui::RichText::new(text).color(tokens.ink));
    }
}

struct TileOutcome {
    action: Option<BrowserAction>,
    note: Option<String>,
    open_detail: Option<egui::Rect>,
}

#[allow(clippy::too_many_arguments)]
fn tile(
    ui: &mut egui::Ui,
    tokens: &Tokens,
    catalog: &Catalog,
    group_index: usize,
    size: egui::Vec2,
    thumbs: &Thumbs,
    ctx: &BrowserContext,
) -> TileOutcome {
    let group = &catalog.groups[group_index];
    let card = catalog.card(group.default_print);
    let copies = ctx.copies_of(&group.name);
    let dim = capped(group, copies);
    let mut outcome = TileOutcome {
        action: None,
        note: None,
        open_detail: None,
    };
    let caption = caption_text(group, ctx);
    ui.vertical(|ui| {
        ui.set_width(size.x);
        ui.spacing_mut().item_spacing.y = TILE_GAP_Y;
        let response = match thumbs.id(&card.riftbound_id) {
            Some(texture) => art_tile(ui, texture, size, dim, is_wide(group.kind)),
            None => placeholder(ui, tokens, group, size, dim, caption != group.name),
        };
        if let Some(badge) = badge_text(group, copies) {
            paint_badge(ui.painter(), tokens, response.rect, &badge);
        }
        let response = if ctx.hover_details() {
            response.on_hover_ui(|ui| hover_summary(ui, tokens, catalog, group))
        } else {
            response
        };
        if response.secondary_clicked() || response.long_touched() {
            outcome.open_detail = Some(response.rect);
        } else if response.clicked() {
            match tap_action(group, ctx) {
                Ok(action) => outcome.action = Some(action),
                Err(note) => outcome.note = Some(note),
            }
        }
        let caption_ink = match group.kind {
            CardKind::Legend => tokens.ink_weak,
            _ if champion_cue(group, ctx) => tokens.amber,
            _ => tokens.ink,
        };
        let caption_hit = ui.add(
            egui::Button::new(egui::RichText::new(caption).size(12.0).color(caption_ink))
                .frame(false)
                .truncate()
                .min_size(egui::vec2(size.x, caption_height(ctx.class))),
        );
        if caption_hit.clicked() {
            outcome.open_detail = Some(response.rect);
        }
    });
    outcome
}

fn grid_height(ui: &egui::Ui, class: ViewportClass) -> f32 {
    let screen = ui.ctx().content_rect().height();
    let available = ui.available_height();
    let one_row = TILE_MIN_W_CARD * CARD_ASPECT + caption_height(class);
    if available.is_finite() && available > 0.0 && available <= screen {
        available.max(one_row)
    } else {
        (screen * 0.6).max(one_row)
    }
}

fn stream_art(state: &mut BrowserState, catalog: &Catalog, thumbs: &Thumbs, art: &mut ArtCache) {
    let mut wanted = Vec::new();
    for print in &state.visible {
        let card = catalog.card(*print);
        if thumbs.id(&card.riftbound_id).is_some()
            || art.has(&card.name)
            || art.has(&card.riftbound_id)
            || state.requested.contains(&card.riftbound_id)
        {
            continue;
        }
        wanted.push(card);
        if wanted.len() >= ART_REQUESTS_PER_FRAME {
            break;
        }
    }
    if wanted.is_empty() {
        return;
    }
    for card in &wanted {
        state.requested.insert(card.riftbound_id.clone());
    }
    art::request_cards(wanted, art);
}

pub fn browser_pane(
    ui: &mut egui::Ui,
    state: &mut BrowserState,
    catalog: &Catalog,
    thumbs: &mut Thumbs,
    art: &mut ArtCache,
    ctx: &BrowserContext,
) -> Vec<BrowserAction> {
    let tokens = theme::tokens(ui.ctx());
    let mut actions = Vec::new();
    if !state.shown {
        state.shown = true;
        state.focus_search = !ctx.class.is_phone();
    }
    state.arm_identity(ctx.identity);
    search_row(ui, state, ctx);
    let now = ui.input(|input| input.time);
    if chips_wrap(ctx.class) {
        ui.horizontal_wrapped(|ui| chips_row(ui, state, catalog, ctx, &tokens));
    } else {
        egui::ScrollArea::horizontal()
            .id_salt("browser chips")
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                ui.horizontal(|ui| chips_row(ui, state, catalog, ctx, &tokens));
            });
    }
    ui.add_space(2.0);
    let gap = ui.spacing().item_spacing;
    let available = ui.available_width();
    let columns = tile_columns_for(available, gap.x, TILE_MIN_W_CARD);
    let size = tile_size_for(available, gap.x, columns);
    let row_h = row_height(size, ctx.class);
    let results = state.results(catalog, ctx.identity).to_vec();
    let rows = results.len().div_ceil(columns);
    let footer_h = 20.0 + gap.y;
    let grid_h = (grid_height(ui, ctx.class) - footer_h).max(row_h);
    let grid_top = ui.cursor().min;
    if let Some(note) = state.live_note(now) {
        float_note(
            ui.ctx(),
            "browser note",
            grid_top + egui::vec2(8.0, 8.0),
            note,
            tokens.amber,
        );
    }
    let mut visible = Vec::new();
    let mut note = None;
    let mut open_detail = None;
    egui::ScrollArea::vertical()
        .id_salt("browser grid")
        .max_height(grid_h)
        .auto_shrink([false, false])
        .show_rows(ui, row_h, rows, |ui, range| {
            for row in range {
                ui.horizontal_top(|ui| {
                    for column in 0..columns {
                        let Some(group_index) = results.get(row * columns + column) else {
                            break;
                        };
                        visible.push(catalog.groups[*group_index].default_print);
                        let outcome = tile(ui, &tokens, catalog, *group_index, size, thumbs, ctx);
                        if let Some(action) = outcome.action {
                            actions.push(action);
                        }
                        if let Some(text) = outcome.note {
                            note = Some(text);
                        }
                        if let Some(rect) = outcome.open_detail {
                            open_detail = Some((*group_index, rect));
                        }
                    }
                });
            }
        });
    if let Some(text) = note {
        state.set_note(text, now);
    }
    if let Some((group, rect)) = open_detail {
        state.open_detail(group, Some(rect));
    }
    state.visible = visible;
    stream_art(state, catalog, thumbs, art);
    ui.label(
        egui::RichText::new(format!("{} cards · {}", results.len(), catalog.note()))
            .color(tokens.ink_weak)
            .small(),
    );
    actions.extend(detail_surface(ui, state, catalog, thumbs, ctx));
    actions
}

fn detail_surface(
    ui: &mut egui::Ui,
    state: &mut BrowserState,
    catalog: &Catalog,
    thumbs: &Thumbs,
    ctx: &BrowserContext,
) -> Vec<BrowserAction> {
    let Some(group) = state.detail else {
        return Vec::new();
    };
    if group >= catalog.groups.len() {
        state.close_detail();
        return Vec::new();
    }
    let mut actions = Vec::new();
    let id = egui::Id::new("browser detail");
    if ctx.class.is_phone() {
        let modal = egui::Modal::new(id).show(ui.ctx(), |ui| {
            actions = card_detail(ui, state, catalog, group, thumbs, ctx);
        });
        if modal.should_close() {
            state.close_detail();
        }
    } else {
        let anchor = match state.detail_anchor {
            Some(rect) => egui::PopupAnchor::ParentRect(rect),
            None => egui::PopupAnchor::Position(ui.ctx().content_rect().center()),
        };
        let mut open = true;
        egui::Popup::new(id, ui.ctx().clone(), anchor, ui.layer_id())
            .open_bool(&mut open)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                egui::ScrollArea::vertical()
                    .id_salt("browser detail scroll")
                    .max_height(ui.ctx().content_rect().height() * DETAIL_MAX_SHARE)
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .show(ui, |ui| {
                        actions = card_detail(ui, state, catalog, group, thumbs, ctx);
                    });
            });
        if !open {
            state.close_detail();
        }
    }
    if !actions.is_empty() {
        ui.ctx().request_repaint();
    }
    actions
}

pub fn card_detail(
    ui: &mut egui::Ui,
    state: &mut BrowserState,
    catalog: &Catalog,
    group: usize,
    thumbs: &Thumbs,
    ctx: &BrowserContext,
) -> Vec<BrowserAction> {
    let tokens = theme::dress(ui);
    let mut actions = Vec::new();
    let Some(group_ref) = catalog.groups.get(group) else {
        return actions;
    };
    let card = catalog.card(group_ref.default_print);
    let copies = ctx.copies_of(&group_ref.name);
    let screen_h = ui.ctx().content_rect().height();
    let texture = thumbs.id(&card.riftbound_id);
    let art_size = detail_art_size(screen_h, texture.is_some(), is_wide(group_ref.kind));
    ui.vertical(|ui| {
        ui.set_width(DETAIL_W);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(&group_ref.name)
                    .strong()
                    .color(tokens.ink),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("×").clicked() {
                    state.close_detail();
                }
            });
        });
        detail_actions(ui, state, catalog, group_ref, copies, ctx, &mut actions);
        match texture {
            Some(texture) => {
                ui.add(
                    egui::Image::new(egui::load::SizedTexture::new(texture, art_size))
                        .corner_radius(8.0)
                        .fit_to_exact_size(art_size),
                );
            }
            None => {
                placeholder(ui, &tokens, group_ref, art_size, false, false);
            }
        }
        let stats = stats_line(group_ref);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!(
                    "{}{}",
                    group_ref.kind.as_str().to_lowercase(),
                    if stats.is_empty() {
                        String::new()
                    } else {
                        format!(" · {stats}")
                    }
                ))
                .color(tokens.ink_weak)
                .small(),
            );
            pips_inline(ui, &tokens, &group_ref.domain);
        });
        if let Some(text) = card.text.as_deref() {
            ui.label(egui::RichText::new(text).color(tokens.ink));
        }
        ui.label(
            egui::RichText::new(print_label(&card.riftbound_id, card.set_id.as_deref()))
                .color(tokens.ink_weak)
                .small(),
        );
        if copies > 0 {
            ui.label(
                egui::RichText::new(format!("in your deck ×{copies}"))
                    .color(tokens.ink_weak)
                    .small(),
            );
        }
        if link(ui, "close").clicked() {
            state.close_detail();
        }
    });
    actions
}

pub const DETAIL_PLACEHOLDER_H: f32 = 140.0;

pub fn detail_art_size(screen_h: f32, has_art: bool, wide: bool) -> egui::Vec2 {
    let cap = screen_h * DETAIL_ART_SHARE;
    let aspect = if wide { WIDE_ASPECT } else { CARD_ASPECT };
    let art_h = if has_art {
        (DETAIL_W * aspect).min(cap)
    } else {
        DETAIL_PLACEHOLDER_H.min(cap)
    };
    egui::vec2(art_h / aspect, art_h)
}

fn detail_actions(
    ui: &mut egui::Ui,
    state: &mut BrowserState,
    catalog: &Catalog,
    group_ref: &Group,
    copies: u32,
    ctx: &BrowserContext,
    actions: &mut Vec<BrowserAction>,
) {
    ui.horizontal_wrapped(|ui| {
        let add = add_label(group_ref, ctx);
        let can_add = !capped(group_ref, copies) || champion_cue(group_ref, ctx);
        if ui.add_enabled(can_add, egui::Button::new(add)).clicked() {
            match tap_action(group_ref, ctx) {
                Ok(action) => actions.push(action),
                Err(note) => {
                    let now = ui.input(|input| input.time);
                    state.set_note(note, now);
                }
            }
        }
        if champion_offer(group_ref, ctx)
            && ui
                .button("set as champion")
                .on_hover_text(CHAMPION_OFFER_NOTE)
                .clicked()
        {
            actions.push(BrowserAction::SetChampion(group_ref.default_print));
        }
        if group_ref.in_deck_kind() && ui.button("to side").clicked() {
            actions.push(BrowserAction::ToSideboard(group_ref.default_print));
        }
        if copies > 0 && ui.button("remove").clicked() {
            actions.push(BrowserAction::Remove(group_ref.default_print));
        }
        if group_ref.prints.len() > 1 {
            ui.menu_button("print", |ui| {
                for print in &group_ref.prints {
                    let other = catalog.card(*print);
                    let label = format!(
                        "{}{}",
                        print_label(&other.riftbound_id, other.set_id.as_deref()),
                        if other.name != group_ref.name {
                            format!("  {}", other.name)
                        } else {
                            String::new()
                        }
                    );
                    if ui.button(label).clicked() {
                        actions.push(BrowserAction::ChangePrint(*print));
                        ui.close();
                    }
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::catalog::fixtures::dump_catalog;
    use crate::deck::catalog::Source;
    use agni_importers::riftbound::catalog::CatalogCard;

    fn ctx<'a>(
        identity: &'a [String],
        champion_tags: &'a [String],
        copies: &'a BTreeMap<String, u32>,
        class: ViewportClass,
    ) -> BrowserContext<'a> {
        BrowserContext {
            identity,
            champion_tags,
            copies,
            champion_empty: true,
            class,
            input: InputKind::Pointer,
        }
    }

    fn group_named<'a>(catalog: &'a Catalog, name: &str) -> &'a Group {
        &catalog.groups[catalog.find_name(name).expect(name)]
    }

    #[test]
    fn a_tap_routes_by_kind_and_refuses_the_fourth_copy() {
        let catalog = dump_catalog();
        let identity = ["Calm".to_string(), "Mind".to_string()];
        let champion_tags = ["Lillia".to_string()];
        let mut copies = BTreeMap::new();
        copies.insert("Lonely Poro".to_string(), 3);
        copies.insert("Seat of Power".to_string(), 1);
        let ctx = ctx(&identity, &champion_tags, &copies, ViewportClass::Desktop);
        let legend = group_named(&catalog, "Lillia - Bashful Bloom");
        assert_eq!(
            tap_action(legend, &ctx),
            Ok(BrowserAction::SetLegend(legend.default_print))
        );
        assert_eq!(caption_text(legend, &ctx), CAPTION_LEGEND);
        let fawn = group_named(&catalog, "Lillia - Fae Fawn");
        assert!(champion_cue(fawn, &ctx));
        assert_eq!(caption_text(fawn, &ctx), CAPTION_CHAMPION);
        assert_eq!(
            tap_action(fawn, &ctx),
            Ok(BrowserAction::SetChampion(fawn.default_print))
        );
        assert_eq!(add_label(fawn, &ctx), "set as champion");
        let poro = group_named(&catalog, "Lonely Poro");
        assert_eq!(tap_action(poro, &ctx), Err(COPY_LIMIT_NOTE.to_string()));
        assert!(capped(poro, 3));
        assert_eq!(badge_text(poro, 3).as_deref(), Some("3/3"));
        assert_eq!(badge_text(poro, 0), None);
        let seat = group_named(&catalog, "Seat of Power");
        assert!(tap_action(seat, &ctx).is_err());
        assert_eq!(badge_text(seat, 1).as_deref(), Some("1"));
        let rune = group_named(&catalog, "Calm Rune");
        assert_eq!(
            tap_action(rune, &ctx),
            Ok(BrowserAction::Add(rune.default_print))
        );
        assert_eq!(badge_text(rune, 7).as_deref(), Some("7"));
        assert!(!capped(rune, 12));
        let other_champion = group_named(&catalog, "Poppy - Paragon");
        assert!(
            !champion_cue(other_champion, &ctx),
            "a Poppy unit shares no tag with Lillia"
        );
        assert_eq!(
            tap_action(other_champion, &ctx),
            Ok(BrowserAction::Add(other_champion.default_print))
        );
        assert_eq!(caption_text(other_champion, &ctx), "Poppy - Paragon");
        let no_champion = BrowserContext {
            champion_empty: false,
            ..ctx
        };
        assert!(!champion_cue(fawn, &no_champion));
        assert!(!champion_offer(fawn, &no_champion));
        assert_eq!(add_label(fawn, &no_champion), "add");
        assert!(
            champion_offer(other_champion, &ctx),
            "a champion unit outside the legend's tag is still offered by hand"
        );
        assert!(
            !champion_offer(fawn, &ctx),
            "the cued card needs no second button"
        );
        assert!(!champion_offer(poro, &ctx));
    }

    #[test]
    fn without_a_legend_every_champion_unit_is_cued_and_a_capped_one_still_promotes() {
        let catalog = dump_catalog();
        let identity: [String; 0] = [];
        let champion_tags: [String; 0] = [];
        let mut copies = BTreeMap::new();
        copies.insert("Lillia - Fae Fawn".to_string(), 3);
        let ctx = ctx(&identity, &champion_tags, &copies, ViewportClass::Desktop);
        let fawn = group_named(&catalog, "Lillia - Fae Fawn");
        assert!(champion_cue(fawn, &ctx));
        assert!(capped(fawn, 3));
        assert_eq!(
            tap_action(fawn, &ctx),
            Ok(BrowserAction::SetChampion(fawn.default_print)),
            "three copies in main do not stop the champion pick"
        );
        let poppy = group_named(&catalog, "Poppy - Paragon");
        assert!(
            champion_cue(poppy, &ctx),
            "no legend, so any champion is cued"
        );
        let poro = group_named(&catalog, "Lonely Poro");
        assert!(!champion_cue(poro, &ctx));
        assert_eq!(
            tap_action(poro, &ctx),
            Ok(BrowserAction::Add(poro.default_print))
        );
    }

    #[test]
    fn columns_follow_the_available_width() {
        assert_eq!(tile_columns_for(400.0, 6.0, TILE_MIN_W_CARD), 4);
        assert_eq!(tile_columns_for(360.0, 8.0, TILE_MIN_W_CARD), 3);
        assert_eq!(tile_columns_for(328.0, 8.0, TILE_MIN_W_CARD), 3);
        assert_eq!(tile_columns_for(200.0, 8.0, TILE_MIN_W_CARD), 2);
        assert_eq!(tile_columns_for(50.0, 8.0, TILE_MIN_W_CARD), 1);
        let size = tile_size_for(400.0, 6.0, 4);
        assert!((size.x - 95.5).abs() < 1e-3);
        assert!((size.y - 95.5 * CARD_ASPECT).abs() < 1e-3);
        assert_eq!(tile_size_for(2000.0, 6.0, 1).x, TILE_MAX_W_CARD);
        assert!(row_height(size, ViewportClass::Desktop) > size.y + CAPTION_H);
        assert_eq!(
            row_height(size, ViewportClass::PhonePortrait)
                - row_height(size, ViewportClass::Desktop),
            CAPTION_H_PHONE - CAPTION_H,
            "phones get a 48 dp caption band"
        );
        assert!(chips_wrap(ViewportClass::Desktop));
        assert!(chips_wrap(ViewportClass::Tablet));
        assert!(!chips_wrap(ViewportClass::PhonePortrait));
        let with_art = detail_art_size(800.0, true, false);
        assert!((with_art.y - DETAIL_W * CARD_ASPECT).abs() < 1e-3);
        let without = detail_art_size(800.0, false, false);
        assert_eq!(without.y, DETAIL_PLACEHOLDER_H);
        let short = detail_art_size(360.0, true, false);
        assert!((short.y - 360.0 * DETAIL_ART_SHARE).abs() < 1e-3);
    }

    #[test]
    fn the_result_cache_follows_filter_search_identity_and_generation() {
        let mut catalog = dump_catalog();
        let mut state = BrowserState::default();
        let identity = ["Fury".to_string()];
        let all = state.results(&catalog, &[]).len();
        assert_eq!(all, catalog.groups.iter().filter(|g| !g.hidden).count());
        state.filter.toggle_kind(CardKind::Legend);
        let legends = state.results(&catalog, &[]).len();
        assert!(legends < all);
        state.arm_identity(&identity);
        assert!(
            state.filter.fits_identity,
            "the first legend arms the identity chip"
        );
        let fitting = state.results(&catalog, &identity).len();
        assert!(fitting < legends);
        state.filter.fits_identity = false;
        state.arm_identity(&identity);
        assert!(
            !state.filter.fits_identity,
            "clearing the chip sticks while the legend stays"
        );
        state.arm_identity(&[]);
        state.arm_identity(&identity);
        assert!(state.filter.fits_identity, "a new legend arms it again");
        state.search = "lillia".into();
        let searched = state.results(&catalog, &identity).len();
        assert!(searched <= fitting);
        catalog.generation += 1;
        let again = state.results(&catalog, &identity).len();
        assert_eq!(again, searched);
        state.open_with(Filter::only_kind(CardKind::Rune), "");
        let runes = state.results(&catalog, &[]).to_vec();
        assert!(runes
            .iter()
            .all(|g| catalog.groups[*g].kind == CardKind::Rune));
        state.reset_filters();
        assert!(state.filter.is_default());
    }

    #[test]
    fn a_note_lives_a_moment_and_the_detail_remembers_its_anchor() {
        let mut state = BrowserState::default();
        state.set_note("+1 Lonely Poro (2/3)", 10.0);
        assert_eq!(state.live_note(10.5), Some("+1 Lonely Poro (2/3)"));
        assert_eq!(state.live_note(12.0), None);
        assert!(state.note.is_none());
        let rect = egui::Rect::from_min_size(egui::pos2(1.0, 2.0), egui::vec2(3.0, 4.0));
        state.open_detail(4, Some(rect));
        assert_eq!(state.detail, Some(4));
        state.close_detail();
        assert_eq!(state.detail, None);
        assert!(!shares_tag(&["Vi".into()], &["Lillia".into()]));
        assert!(shares_tag(
            &["Yordle".into(), "Poppy".into()],
            &["poppy".into()]
        ));
    }

    #[test]
    fn print_labels_read_set_and_number_and_flag_a_reprint_set() {
        assert_eq!(print_label("unl-116a-219", Some("UNL")), "UNL · 116a");
        assert_eq!(print_label("ogn-042-298", Some("OGN")), "OGN · 042");
        assert_eq!(print_label("opp-183-298", Some("OPP")), "OPP · 183");
        assert_eq!(print_label("ven-r01", None), "VEN · R01");
        assert_eq!(print_label("nonsense", None), "nonsense");
        let group = Group {
            name: "Ember".into(),
            prints: vec![0],
            default_print: 0,
            kind: CardKind::Unit,
            domain: vec!["Fury".into()],
            champion: false,
            signature: false,
            tags: Vec::new(),
            energy: Some(3),
            might: Some(2),
            power: Some(1),
            text_lower: String::new(),
            name_lower: "ember".into(),
            hidden: false,
        };
        assert_eq!(stats_line(&group), "3 energy · 1 power · 2 might");
    }

    fn run_pane(
        width: f32,
        height: f32,
        class: ViewportClass,
        click: Option<egui::Pos2>,
        state: &mut BrowserState,
        catalog: &Catalog,
        copies: &BTreeMap<String, u32>,
    ) -> (Vec<BrowserAction>, egui::FullOutput) {
        let context = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(width, height));
        let mut actions = Vec::new();
        let mut output = None;
        let identity: Vec<String> = Vec::new();
        let tags: Vec<String> = Vec::new();
        let mut thumbs = Thumbs::default();
        let mut art = ArtCache::default();
        for pass in 0..3 {
            let mut input = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            if pass == 2 {
                if let Some(at) = click {
                    input.events.push(egui::Event::PointerMoved(at));
                    input.events.push(egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    });
                    input.events.push(egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
            }
            let mut full = context.run_ui(input, |ui| {
                ui.set_min_size(screen.size());
                ui.set_max_size(screen.size());
                ui.set_clip_rect(screen);
                let ctx = BrowserContext {
                    identity: &identity,
                    champion_tags: &tags,
                    copies,
                    champion_empty: true,
                    class,
                    input: InputKind::Pointer,
                };
                actions = browser_pane(ui, state, catalog, &mut thumbs, &mut art, &ctx);
            });
            full.textures_delta.clear();
            output = Some(full);
        }
        (actions, output.expect("three passes ran"))
    }

    fn small_catalog() -> Catalog {
        let card = |name: &str, id: &str, kind: CardKind| CatalogCard {
            name: name.into(),
            riftbound_id: id.into(),
            kind,
            domain: vec!["Calm".into()],
            energy: Some(2),
            text: Some("text".into()),
            ..Default::default()
        };
        let mut cards = Vec::new();
        for index in 0..30 {
            cards.push(card(
                &format!("Unit {index:02}"),
                &format!("ogn-{:03}-298", index + 1),
                CardKind::Unit,
            ));
        }
        Catalog::from_cards(cards, Source::Store(0))
    }

    fn tile_rects(output: &egui::FullOutput, size: egui::Vec2) -> Vec<egui::Rect> {
        let mut rects: Vec<egui::Rect> = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Rect(rect)
                    if (rect.rect.width() - size.x).abs() < 0.5
                        && (rect.rect.height() - size.y).abs() < 0.5 =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
            .collect();
        rects.sort_by(|a, b| {
            a.min
                .y
                .partial_cmp(&b.min.y)
                .unwrap()
                .then(a.min.x.partial_cmp(&b.min.x).unwrap())
        });
        rects.dedup();
        rects
    }

    #[test]
    fn the_grid_lays_out_three_columns_at_360_and_four_at_400_with_the_desktop_gap() {
        let catalog = small_catalog();
        let copies = BTreeMap::new();
        let mut state = BrowserState::default();
        let (actions, output) = run_pane(
            360.0,
            640.0,
            ViewportClass::PhonePortrait,
            None,
            &mut state,
            &catalog,
            &copies,
        );
        assert!(actions.is_empty());
        assert!(
            !state.visible.is_empty(),
            "visible rows are recorded for staging"
        );
        assert_eq!(
            state.visible[0], catalog.groups[0].default_print,
            "the first visible tile is the first group"
        );
        let gap = egui::Style::default().spacing.item_spacing.x;
        let columns = tile_columns_for(360.0, gap, TILE_MIN_W_CARD);
        assert_eq!(columns, 3);
        let rects = tile_rects(&output, tile_size_for(360.0, gap, columns));
        assert!(
            rects.len() >= 3,
            "three tiles paint on the first row: {rects:?}"
        );
        let first_row = rects[0].min.y;
        assert_eq!(
            rects
                .iter()
                .filter(|r| (r.min.y - first_row).abs() < 0.5)
                .count(),
            3
        );
        let (_, output) = run_pane(
            400.0,
            600.0,
            ViewportClass::Desktop,
            None,
            &mut state,
            &catalog,
            &copies,
        );
        let columns = tile_columns_for(400.0, gap, TILE_MIN_W_CARD);
        assert_eq!(columns, 4);
        let rects = tile_rects(&output, tile_size_for(400.0, gap, columns));
        let first_row = rects[0].min.y;
        assert_eq!(
            rects
                .iter()
                .filter(|r| (r.min.y - first_row).abs() < 0.5)
                .count(),
            4
        );
        let clips: Vec<egui::Rect> = output
            .shapes
            .iter()
            .filter(|clipped| matches!(clipped.shape, egui::Shape::Rect(_)))
            .map(|clipped| clipped.clip_rect)
            .collect();
        assert!(
            clips.iter().all(|r| r.max.x <= 400.5 && r.max.y <= 600.5),
            "no tile paints outside the pane: {clips:?}"
        );
    }

    #[test]
    fn a_tap_on_the_first_tile_returns_add_and_the_visible_prints_get_requested() {
        let catalog = small_catalog();
        let copies = BTreeMap::new();
        let mut state = BrowserState::default();
        let (_, output) = run_pane(
            400.0,
            700.0,
            ViewportClass::Desktop,
            None,
            &mut state,
            &catalog,
            &copies,
        );
        let requested = state.requested.len();
        assert!(
            requested > 0 && requested <= ART_REQUESTS_PER_FRAME * 3,
            "art is requested for visible tiles only, capped per frame: {requested}"
        );
        assert!(
            requested < catalog.cards.len(),
            "off-screen tiles never request art"
        );
        let gap = egui::Style::default().spacing.item_spacing.x;
        let size = tile_size_for(400.0, gap, tile_columns_for(400.0, gap, TILE_MIN_W_CARD));
        let rects = tile_rects(&output, size);
        let first = rects.first().expect("a first tile");
        let (actions, _) = run_pane(
            400.0,
            700.0,
            ViewportClass::Desktop,
            Some(first.center()),
            &mut state,
            &catalog,
            &copies,
        );
        assert_eq!(
            actions,
            vec![BrowserAction::Add(catalog.groups[0].default_print)],
            "a tap on the first tile adds one copy of it"
        );
        assert!(
            state.detail.is_none(),
            "a tap on the art never opens the detail"
        );
        let mut full = BTreeMap::new();
        full.insert(catalog.groups[0].name.clone(), COPY_LIMIT);
        let (actions, _) = run_pane(
            400.0,
            700.0,
            ViewportClass::Desktop,
            Some(first.center()),
            &mut state,
            &catalog,
            &full,
        );
        assert!(
            actions.is_empty(),
            "the fourth copy is refused: {actions:?}"
        );
        assert!(
            state
                .note
                .as_ref()
                .is_some_and(|(text, _)| text == COPY_LIMIT_NOTE),
            "the refusal leaves the amber note"
        );
    }
}
