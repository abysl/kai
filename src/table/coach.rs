use super::highlight::{acting, rim_color_in, strongest, Palette, RimKind, Rims};
use super::*;
use agni_sim::wire::PluginView;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const CALLOUT_W: f32 = 240.0;
pub const CALLOUT_GAP: f32 = 8.0;
pub const CALLOUT_GUESS_H: f32 = 96.0;
pub const IDLE_SECS: f64 = 6.0;
pub const HINT_SECS: f64 = 8.0;
pub const HINT_TIMES: u8 = 2;
pub const TAG_SECS: f64 = 3.0;
pub const TAG_PT: f32 = 12.0;
pub const TAG_GAP: f32 = 4.0;
pub const TIP_SECS: f64 = 1.5;
pub const KEEP_OUT: f32 = 0.5;
pub const GOT_IT: &str = "got it";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Mark {
    Deal,
    Prompt,
    Pass,
}

impl Mark {
    pub const ALL: [Mark; 3] = [Mark::Deal, Mark::Prompt, Mark::Pass];

    pub fn text(self) -> &'static str {
        match self {
            Mark::Deal => "lit cards can be played — drag one to a lit zone",
            Mark::Prompt => "pulsing cards answer the question — tap one",
            Mark::Pass => "this button always says what it does",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Seen {
    pub marks: BTreeSet<Mark>,
    pub hints: BTreeMap<String, u8>,
}

impl Seen {
    pub fn saw(&self, mark: Mark) -> bool {
        self.marks.contains(&mark)
    }

    pub fn see(&mut self, mark: Mark) -> bool {
        self.marks.insert(mark)
    }

    pub fn hint_allowed(&self, key: &str) -> bool {
        self.hints.get(key).copied().unwrap_or(0) < HINT_TIMES
    }

    pub fn count_hint(&mut self, key: &str) {
        *self.hints.entry(key.to_string()).or_insert(0) += 1;
    }

    pub fn reset(&mut self) {
        self.marks.clear();
        self.hints.clear();
    }
}

pub fn colour_word(kind: RimKind, palette: Palette) -> &'static str {
    match (kind, palette) {
        (RimKind::Play | RimKind::Hide, Palette::Standard) => "green",
        (RimKind::Play | RimKind::Hide, Palette::ColourBlind) => "blue",
        (RimKind::March, _) => "cyan",
        (RimKind::Activate, Palette::Standard) => "orange",
        (RimKind::Activate, Palette::ColourBlind) => "yellow",
        (RimKind::React, _) => "violet",
        (RimKind::Answer, _) => "pale",
        (RimKind::Enemy, Palette::Standard) => "pink",
        (RimKind::Enemy, Palette::ColourBlind) => "orange",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint {
    pub kind: Option<RimKind>,
    pub primary: Option<String>,
}

impl Hint {
    pub fn key(&self) -> String {
        match self.kind {
            Some(kind) => kind.legend().to_string(),
            None => "primary".to_string(),
        }
    }

    pub fn text(&self, palette: Palette) -> String {
        let or_press = self
            .primary
            .as_ref()
            .map(|label| format!(", or press {label}"))
            .unwrap_or_default();
        match self.kind {
            Some(RimKind::Play) => format!(
                "drag a {} card to play it{or_press}",
                colour_word(RimKind::Play, palette)
            ),
            Some(RimKind::March) => format!(
                "drag a {} unit to a battlefield to attack{or_press}",
                colour_word(RimKind::March, palette)
            ),
            Some(RimKind::Activate) => format!(
                "a {} tag marks an ability — select the card for its chips",
                colour_word(RimKind::Activate, palette)
            ),
            Some(RimKind::React) => format!(
                "respond with a {} card, or pass",
                colour_word(RimKind::React, palette)
            ),
            Some(RimKind::Answer) => "tap a pulsing card to answer".to_string(),
            Some(RimKind::Hide) => {
                "drag a dotted card to a battlefield to hide it there".to_string()
            }
            Some(RimKind::Enemy) => "a ringed card is being targeted".to_string(),
            None => match &self.primary {
                Some(label) => format!("nothing to play — press {label}"),
                None => String::new(),
            },
        }
    }
}

pub fn idle_hint(view: &PluginView) -> Option<Hint> {
    if !acting(view) || view.prompt.is_some() {
        return None;
    }
    let primary = primary::primary_of(view)
        .filter(primary::Primary::enabled)
        .map(|primary| primary.label);
    let kind = view
        .legal
        .iter()
        .filter_map(|row| strongest(&row.kinds))
        .min();
    if kind.is_none() && primary.is_none() {
        return None;
    }
    Some(Hint { kind, primary })
}

pub fn next_mark(view: &PluginView, me: u8, rims: &Rims, seen: &Seen) -> Option<Mark> {
    let prompt_mine = view
        .prompt
        .as_ref()
        .is_some_and(|summary| summary.seat == me);
    if prompt_mine && !seen.saw(Mark::Prompt) {
        return Some(Mark::Prompt);
    }
    if !rims.lift.is_empty() && !seen.saw(Mark::Deal) {
        return Some(Mark::Deal);
    }
    let pass = acting(view) && primary::pass_index(view).is_some();
    if pass && !seen.saw(Mark::Pass) {
        return Some(Mark::Pass);
    }
    None
}

pub fn primary_bytes(view: &PluginView) -> Option<Vec<u8>> {
    let index = primary::primary_of(view).and_then(|primary| primary.affordance)?;
    Some(view.affordances[index].data.to_vec())
}

pub fn primary_fired(before: Option<&[u8]>, view: &PluginView, fired: &[Vec<u8>]) -> bool {
    let now = primary_bytes(view);
    fired.iter().any(|bytes| {
        before.is_some_and(|data| bytes.as_slice() == data)
            || now.as_deref().is_some_and(|data| bytes.as_slice() == data)
    })
}

pub fn done(mark: Mark, dragged: bool, any_fired: bool, primary_fired: bool) -> bool {
    match mark {
        Mark::Deal => dragged,
        Mark::Prompt => any_fired,
        Mark::Pass => primary_fired,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shown {
    pub mark: Mark,
    pub since: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Idle {
    pub hint: Hint,
    pub since: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tag {
    pub kind: RimKind,
    pub card: u32,
    pub at: f64,
}

#[derive(Resource, Debug, Default)]
pub struct Coach {
    pub shown: Option<Shown>,
    pub idle: Option<Idle>,
    pub last_change: f64,
    pub idle_checked: Option<f64>,
    pub tags: Vec<Tag>,
    pub tagged: BTreeSet<RimKind>,
    pub active: bool,
    pub got_it: Option<Mark>,
}

impl Coach {
    pub fn end_session(&mut self) {
        *self = Coach::default();
    }

    pub fn tag(&mut self, kind: RimKind, card: u32, now: f64) {
        if self.tagged.insert(kind) {
            self.tags.push(Tag {
                kind,
                card,
                at: now,
            });
        }
    }

    pub fn expire_tags(&mut self, now: f64) {
        self.tags.retain(|tag| now - tag.at < TAG_SECS);
    }
}

pub fn keep_out(stage: egui::Rect) -> egui::Rect {
    egui::Rect::from_center_size(stage.center(), stage.size() * KEEP_OUT)
}

pub fn clamp_into(rect: egui::Rect, bounds: egui::Rect) -> egui::Rect {
    let mut rect = rect;
    if rect.max.x > bounds.max.x {
        rect = rect.translate(egui::vec2(bounds.max.x - rect.max.x, 0.0));
    }
    if rect.min.x < bounds.min.x {
        rect = rect.translate(egui::vec2(bounds.min.x - rect.min.x, 0.0));
    }
    if rect.max.y > bounds.max.y {
        rect = rect.translate(egui::vec2(0.0, bounds.max.y - rect.max.y));
    }
    if rect.min.y < bounds.min.y {
        rect = rect.translate(egui::vec2(0.0, bounds.min.y - rect.min.y));
    }
    rect
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Above,
    Below,
    LeftOf,
}

pub fn lifted_off(rect: egui::Rect, avoid: &[egui::Rect], safe: egui::Rect) -> egui::Rect {
    let mut rect = rect;
    for _ in 0..avoid.len() {
        let Some(blocking) = avoid
            .iter()
            .filter(|slot| slot.width() > 0.0 && slot.height() > 0.0 && rect.intersects(**slot))
            .min_by(|a, b| a.min.y.total_cmp(&b.min.y))
        else {
            break;
        };
        rect = clamp_into(
            egui::Rect::from_min_size(
                egui::pos2(rect.min.x, blocking.min.y - CALLOUT_GAP - rect.height()),
                rect.size(),
            ),
            safe,
        );
    }
    rect
}

pub fn place_callout(
    anchor: egui::Rect,
    side: Anchor,
    size: egui::Vec2,
    safe: egui::Rect,
    stage: egui::Rect,
    avoid: &[egui::Rect],
) -> egui::Rect {
    let rect = match side {
        Anchor::Above => egui::Rect::from_min_size(
            egui::pos2(
                anchor.center().x - size.x / 2.0,
                anchor.min.y - CALLOUT_GAP - size.y,
            ),
            size,
        ),
        Anchor::Below => egui::Rect::from_min_size(
            egui::pos2(anchor.center().x - size.x / 2.0, anchor.max.y + CALLOUT_GAP),
            size,
        ),
        Anchor::LeftOf => egui::Rect::from_min_size(
            egui::pos2(anchor.min.x - CALLOUT_GAP - size.x, anchor.max.y - size.y),
            size,
        ),
    };
    let mut rect = clamp_into(rect, safe);
    rect = lifted_off(rect, avoid, safe);
    let zone = keep_out(stage);
    if rect.intersects(zone) {
        let right = clamp_into(
            egui::Rect::from_min_size(egui::pos2(zone.max.x + CALLOUT_GAP, rect.min.y), size),
            safe,
        );
        let left = clamp_into(
            egui::Rect::from_min_size(
                egui::pos2(zone.min.x - CALLOUT_GAP - size.x, rect.min.y),
                size,
            ),
            safe,
        );
        let below = clamp_into(
            egui::Rect::from_min_size(egui::pos2(rect.min.x, zone.max.y + CALLOUT_GAP), size),
            safe,
        );
        let above = clamp_into(
            egui::Rect::from_min_size(
                egui::pos2(rect.min.x, zone.min.y - CALLOUT_GAP - size.y),
                size,
            ),
            safe,
        );
        rect = [right, left, below, above]
            .into_iter()
            .map(|candidate| lifted_off(candidate, avoid, safe))
            .find(|candidate| !candidate.intersects(zone))
            .unwrap_or(rect);
    }
    rect
}

pub fn callout_avoids(hud: &hud::HudRects, mark: Mark) -> Vec<egui::Rect> {
    let mut out = vec![hud.secondary];
    if mark != Mark::Pass {
        out.push(hud.primary);
    }
    out.extend(hud.bottom_left);
    out.push(hud.hand);
    out
}

pub fn anchor_side(mark: Mark, phone: bool) -> Anchor {
    match mark {
        Mark::Deal => Anchor::Above,
        Mark::Prompt => Anchor::Above,
        Mark::Pass if phone => Anchor::Above,
        Mark::Pass => Anchor::LeftOf,
    }
}

pub fn touch_tip(response: egui::Response, label: &str) -> egui::Response {
    let context = response.ctx.clone();
    let now = context.input(|input| input.time);
    let id = response.id.with("touch tip");
    if response.long_touched() {
        context.data_mut(|data| data.insert_temp(id, now));
    }
    let since: Option<f64> = context.data(|data| data.get_temp(id));
    if since.is_some_and(|since| now - since < TIP_SECS) {
        response.show_tooltip_text(label);
        context.request_repaint();
        response
    } else {
        response.on_hover_text(label)
    }
}

pub fn refresh_coach(
    time: Res<Time>,
    menu: Res<crate::menu::Menu>,
    info: Res<SessionInfo>,
    my_seat: Res<MySeat>,
    panel: Res<plugin_ui::PluginPanel>,
    rims: Res<Rims>,
    held: Res<Held>,
    mut tuning: ResMut<Tuning>,
    mut coach: ResMut<Coach>,
    mut requests: MessageReader<plugin_ui::PluginActionRequested>,
    mut drops: MessageReader<CardDropped>,
    mut before: Local<Option<Vec<u8>>>,
) {
    let now = time.elapsed_secs_f64();
    let fired: Vec<Vec<u8>> = requests.read().map(|request| request.0.clone()).collect();
    let dropped = drops.read().count() > 0;
    let fired_primary = primary_fired(before.as_deref(), &panel.view, &fired);
    *before = primary_bytes(&panel.view);
    if !menu.at_table() || !info.active() {
        if coach.active {
            coach.end_session();
        }
        return;
    }
    if !coach.active {
        coach.active = true;
        coach.last_change = now;
    }
    if panel.is_changed() {
        coach.last_change = now;
        coach.idle = None;
        coach.idle_checked = None;
    }
    let mut seen = tuning.coach.clone();
    if let Some(got_it) = coach.got_it.take() {
        seen.see(got_it);
    }
    if let Some(shown) = coach.shown {
        let dragged = dropped || held.card.is_some();
        if done(shown.mark, dragged, !fired.is_empty(), fired_primary) {
            seen.see(shown.mark);
        }
        if seen.saw(shown.mark) {
            coach.shown = None;
        }
    }
    if coach.shown.is_none() {
        if let Some(mark) = next_mark(&panel.view, my_seat.0 .0, &rims, &seen) {
            coach.shown = Some(Shown { mark, since: now });
        }
    }
    if coach.shown.is_none() && held.card.is_none() {
        let expired = coach
            .idle
            .as_ref()
            .is_some_and(|idle| now - idle.since > HINT_SECS);
        if expired {
            coach.idle = None;
        } else if coach.idle.is_none()
            && now - coach.last_change >= IDLE_SECS
            && coach.idle_checked != Some(coach.last_change)
        {
            coach.idle_checked = Some(coach.last_change);
            if let Some(hint) = idle_hint(&panel.view) {
                let key = hint.key();
                if seen.hint_allowed(&key) {
                    seen.count_hint(&key);
                    coach.idle = Some(Idle { hint, since: now });
                }
            }
        }
    } else if coach.idle.is_some() {
        coach.idle = None;
    }
    let legal: Vec<(u32, RimKind)> = rims
        .legal
        .iter()
        .map(|(card, kind)| (*card, *kind))
        .collect();
    for (card, kind) in legal {
        coach.tag(kind, card, now);
    }
    let enemies: Vec<u32> = rims.enemy.iter().copied().collect();
    for card in enemies {
        coach.tag(RimKind::Enemy, card, now);
    }
    coach.expire_tags(now);
    if seen != tuning.coach {
        tuning.coach = seen;
    }
}

fn card_rect(
    cards: &Query<(&CardView, &GlobalTransform, &ViewVisibility, Has<Landscape>)>,
    camera: &Camera,
    camera_transform: &GlobalTransform,
    card: u32,
) -> Option<egui::Rect> {
    cards
        .iter()
        .find(|(view, _, visible, _)| view.0 .0 == card && visible.get())
        .and_then(|(_, transform, _, landscape)| {
            chips::screen_rect(camera, camera_transform, transform, landscape)
        })
}

#[allow(clippy::too_many_arguments)]
pub fn coach_ui(
    mut contexts: EguiContexts,
    time: Res<Time>,
    hud: Res<hud::Hud>,
    viewport: Res<crate::viewport::Viewport>,
    menu: Res<crate::menu::Menu>,
    settings: Res<crate::settings::Settings>,
    table_menu: Res<hud::TableMenu>,
    seats: hud::Seats,
    panel: Res<plugin_ui::PluginPanel>,
    refusals: Res<toast::Refusals>,
    rims: Res<Rims>,
    tuning: Res<Tuning>,
    mut coach: ResMut<Coach>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    cards: Query<(&CardView, &GlobalTransform, &ViewVisibility, Has<Landscape>)>,
    mut measured: Local<f32>,
) -> Result {
    if !menu.at_table() || !coach.active || !seats.info.active() {
        return Ok(());
    }
    let now = time.elapsed_secs_f64();
    let context = contexts.ctx_mut()?.clone();
    let Ok((camera, camera_transform)) = camera.single() else {
        return Ok(());
    };
    let palette = Palette::of(&tuning);
    let phone = viewport.class.is_phone();
    let painter = context.layer_painter(egui::LayerId::background());
    for tag in &coach.tags {
        let Some(rect) = card_rect(&cards, camera, camera_transform, tag.card) else {
            continue;
        };
        let colour = rim_color_in(tag.kind, palette);
        let at = egui::pos2(rect.max.x + TAG_GAP, rect.min.y);
        let galley = painter.layout_no_wrap(
            tag.kind.legend().to_string(),
            egui::FontId::proportional(TAG_PT),
            colour,
        );
        let back = egui::Rect::from_min_size(at, galley.size() + egui::vec2(6.0, 2.0));
        painter.rect_filled(back, 3.0, hud::SURFACE);
        painter.galley(at + egui::vec2(3.0, 1.0), galley, colour);
    }
    let covered = settings.open || table_menu.open || menu.sheet.is_some();
    if covered {
        return Ok(());
    }
    if let Some(idle) = &coach.idle {
        let rect = if phone {
            hud.0
                .ticker
                .filter(|rect| rect.height() >= hud::TICKER_H * 0.5)
        } else {
            let free = hud::strip_idle(&panel.view, seats.me().0, &seats.info, seats.players.0)
                && refusals.on_strip(now).is_none();
            free.then_some(hud.0.strip)
        };
        if let Some(rect) = rect {
            let text = idle.hint.text(palette);
            hud::slot(&context, "idle hint", rect, |ui| {
                hud::panel_frame(ui.style()).show(ui, |ui| {
                    ui.set_min_width(rect.width() - 16.0);
                    ui.set_max_width(rect.width() - 16.0);
                    ui.horizontal(|ui| {
                        crate::theme::icon(ui, "drag", 16.0, hud::INK_WEAK);
                        ui.label(egui::RichText::new(text).color(hud::INK));
                    });
                });
            });
        }
    }
    let Some(shown) = coach.shown else {
        return Ok(());
    };
    let anchor = match shown.mark {
        Mark::Deal => rims
            .lift
            .iter()
            .find_map(|card| card_rect(&cards, camera, camera_transform, *card)),
        Mark::Prompt => rims
            .pulse
            .iter()
            .find_map(|card| card_rect(&cards, camera, camera_transform, *card))
            .or(Some(hud.0.strip)),
        Mark::Pass => Some(hud.0.primary),
    };
    let Some(anchor) = anchor else {
        return Ok(());
    };
    let side = match shown.mark {
        Mark::Prompt if rims.pulse.is_empty() => Anchor::Below,
        mark => anchor_side(mark, phone),
    };
    let guess = if *measured > 0.0 {
        *measured
    } else {
        CALLOUT_GUESS_H
    };
    let rect = place_callout(
        anchor,
        side,
        egui::vec2(CALLOUT_W, guess),
        hud.0.safe,
        hud.0.stage,
        &callout_avoids(&hud.0, shown.mark),
    );
    let touch = phone || viewport.class == crate::viewport::ViewportClass::Tablet;
    let mut got_it = false;
    let response = egui::Area::new(egui::Id::new("coach mark"))
        .fixed_pos(rect.min)
        .order(egui::Order::Foreground)
        .show(&context, |ui| {
            ui.set_max_width(CALLOUT_W);
            hud::panel_frame(ui.style())
                .stroke(egui::Stroke::new(2.0, hud::AMBER))
                .show(ui, |ui| {
                    ui.set_min_width(CALLOUT_W - 16.0);
                    ui.set_max_width(CALLOUT_W - 16.0);
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    ui.label(egui::RichText::new(shown.mark.text()).color(hud::INK));
                    let size = egui::vec2(
                        ui.available_width(),
                        if touch { hud::TOUCH_MIN } else { 28.0 },
                    );
                    if ui.add_sized(size, egui::Button::new(GOT_IT)).clicked() {
                        got_it = true;
                    }
                });
        });
    *measured = response.response.rect.height();
    let from = match side {
        Anchor::Above => response.response.rect.center_bottom(),
        Anchor::Below => response.response.rect.center_top(),
        Anchor::LeftOf => response.response.rect.right_center(),
    };
    let to = match side {
        Anchor::Above => anchor.center_top(),
        Anchor::Below => anchor.center_bottom(),
        Anchor::LeftOf => anchor.left_center(),
    };
    context
        .layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("coach line"),
        ))
        .line_segment([from, to], egui::Stroke::new(2.0, hud::AMBER));
    if got_it {
        coach.got_it = Some(shown.mark);
        coach.shown = None;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agni_sim::wire::{Affordance, AffordanceKind, Legal, LegalKind, PromptSummary};
    use serde_bytes::ByteBuf;

    fn offer(label: &str, hotkey: &str, data: u8) -> Affordance {
        Affordance {
            label: label.into(),
            hotkey: Some(hotkey.into()),
            enabled: true,
            kind: AffordanceKind::Plain,
            data: ByteBuf::from(vec![data]),
            card: None,
        }
    }

    fn acting_view(kinds: Vec<LegalKind>) -> PluginView {
        PluginView {
            status: vec!["turn 1 · {seat 0} · action phase · rules enforced".into()],
            affordances: vec![offer("end turn", "space", 9)],
            legal: if kinds.is_empty() {
                Vec::new()
            } else {
                vec![Legal {
                    card: 7,
                    kinds,
                    zones: vec![3],
                    hidden: Vec::new(),
                }]
            },
            ..Default::default()
        }
    }

    #[test]
    fn the_idle_hint_names_the_strongest_rim_in_the_palettes_words() {
        let cases = [
            (
                LegalKind::Play { accelerate: false },
                RimKind::Play,
                "green",
                "blue",
            ),
            (LegalKind::March, RimKind::March, "cyan", "cyan"),
            (
                LegalKind::Activate { ability: 0 },
                RimKind::Activate,
                "orange",
                "yellow",
            ),
            (LegalKind::React, RimKind::React, "violet", "violet"),
            (LegalKind::Answer, RimKind::Answer, "pulsing", "pulsing"),
            (LegalKind::Hide, RimKind::Hide, "dotted", "dotted"),
        ];
        for (kind, rim, standard, colour_blind) in cases {
            let hint = idle_hint(&acting_view(vec![kind])).expect("a hint");
            assert_eq!(hint.kind, Some(rim));
            assert_eq!(hint.key(), rim.legend());
            assert!(
                hint.text(Palette::Standard).contains(standard),
                "{rim:?}: {}",
                hint.text(Palette::Standard)
            );
            assert!(hint.text(Palette::ColourBlind).contains(colour_blind));
        }
        let both = idle_hint(&acting_view(vec![LegalKind::March, LegalKind::React])).unwrap();
        assert_eq!(both.kind, Some(RimKind::React), "the strongest rim wins");
        let play = idle_hint(&acting_view(vec![LegalKind::Play { accelerate: true }])).unwrap();
        assert_eq!(
            play.text(Palette::Standard),
            "drag a green card to play it, or press end turn"
        );
        let nothing = idle_hint(&acting_view(Vec::new())).unwrap();
        assert_eq!(nothing.kind, None);
        assert_eq!(nothing.key(), "primary");
        assert_eq!(
            nothing.text(Palette::Standard),
            "nothing to play — press end turn"
        );
    }

    #[test]
    fn no_hint_while_a_prompt_is_open_or_it_is_not_my_move() {
        let mut asked = acting_view(vec![LegalKind::Play { accelerate: false }]);
        asked.prompt = Some(PromptSummary {
            seat: 0,
            why: "choose".into(),
            min: 1,
            max: 1,
            picked: 0,
            optional: false,
        });
        assert_eq!(idle_hint(&asked), None);
        let theirs = PluginView {
            status: vec!["turn 1 · {seat 1} · action phase · rules enforced".into()],
            legal: acting_view(vec![LegalKind::React]).legal,
            ..Default::default()
        };
        assert_eq!(idle_hint(&theirs), None);
        assert_eq!(idle_hint(&PluginView::default()), None);
    }

    #[test]
    fn the_seen_set_round_trips_with_tuning_and_caps_each_hint() {
        let mut seen = Seen::default();
        assert!(!seen.saw(Mark::Deal));
        assert!(seen.see(Mark::Deal));
        assert!(!seen.see(Mark::Deal));
        assert!(seen.hint_allowed("playable"));
        seen.count_hint("playable");
        assert!(seen.hint_allowed("playable"));
        seen.count_hint("playable");
        assert!(!seen.hint_allowed("playable"));
        assert!(seen.hint_allowed("respond"));
        let tuned = Tuning {
            coach: seen.clone(),
            ..Tuning::default()
        };
        let json = serde_json::to_string(&tuned).unwrap();
        let back: Tuning = serde_json::from_str(&json).unwrap();
        assert_eq!(back.coach, seen);
        let old: Tuning = serde_json::from_str(r#"{"zoom": 1.0, "view_version": 3}"#).unwrap();
        assert_eq!(old.coach, Seen::default());
        seen.reset();
        assert_eq!(seen, Seen::default());
        let partial: Seen = serde_json::from_str(r#"{"marks": ["Pass"]}"#).unwrap();
        assert!(partial.saw(Mark::Pass) && partial.hints.is_empty());
    }

    #[test]
    fn marks_come_one_at_a_time_in_the_order_the_player_meets_them() {
        let view = acting_view(vec![LegalKind::Play { accelerate: false }]);
        let mut rims = Rims::default();
        rims.lift.insert(7);
        let mut seen = Seen::default();
        assert_eq!(next_mark(&view, 0, &rims, &seen), Some(Mark::Deal));
        seen.see(Mark::Deal);
        assert_eq!(
            next_mark(&view, 0, &rims, &seen),
            None,
            "end turn on turn 1 is not the first pass"
        );
        let mut window = view.clone();
        window.affordances = vec![offer("pass", "w", 9)];
        assert_eq!(next_mark(&window, 0, &rims, &seen), Some(Mark::Pass));
        seen.see(Mark::Pass);
        assert_eq!(next_mark(&window, 0, &rims, &seen), None);
        let mut asked = view.clone();
        asked.prompt = Some(PromptSummary {
            seat: 0,
            why: "choose".into(),
            min: 1,
            max: 1,
            picked: 0,
            optional: false,
        });
        assert_eq!(next_mark(&asked, 0, &rims, &seen), Some(Mark::Prompt));
        assert_eq!(
            next_mark(&asked, 1, &rims, &seen),
            None,
            "another seat's question is not my mark"
        );
        assert!(done(Mark::Deal, true, false, false));
        assert!(!done(Mark::Deal, false, true, true));
        assert!(done(Mark::Prompt, false, true, false));
        assert!(!done(Mark::Pass, true, true, false));
        assert!(done(Mark::Pass, false, true, true));
        assert!(primary_fired(None, &view, &[vec![9]]));
        assert!(!primary_fired(None, &view, &[vec![1]]));
        let mut after = view.clone();
        after.affordances = vec![offer("pass", "w", 4)];
        assert!(
            primary_fired(Some(&[9]), &after, &[vec![9]]),
            "the bytes fired against the view before the refresh still count"
        );
        assert!(!primary_fired(Some(&[9]), &after, &[vec![1]]));
    }

    #[test]
    fn the_callout_stays_inside_the_safe_rect_and_off_the_felts_centre() {
        let safe = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 800.0));
        let stage = egui::Rect::from_min_size(egui::pos2(220.0, 80.0), egui::vec2(840.0, 500.0));
        let size = egui::vec2(CALLOUT_W, CALLOUT_GUESS_H);
        let hand_card =
            egui::Rect::from_min_size(egui::pos2(600.0, 700.0), egui::vec2(80.0, 112.0));
        let above = place_callout(hand_card, Anchor::Above, size, safe, stage, &[]);
        assert!(safe.contains_rect(above));
        assert!(!above.intersects(keep_out(stage)));
        assert!(above.max.y <= hand_card.min.y);
        let centre_card = egui::Rect::from_center_size(stage.center(), egui::vec2(80.0, 112.0));
        let dodged = place_callout(centre_card, Anchor::Above, size, safe, stage, &[]);
        assert!(safe.contains_rect(dodged));
        assert!(!dodged.intersects(keep_out(stage)));
        let primary = egui::Rect::from_min_size(egui::pos2(1100.0, 640.0), egui::vec2(168.0, 56.0));
        let beside = place_callout(primary, Anchor::LeftOf, size, safe, stage, &[]);
        assert!(safe.contains_rect(beside));
        assert!(beside.max.x <= primary.min.x);
        let phone_safe = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(360.0, 800.0));
        let phone_stage =
            egui::Rect::from_min_size(egui::pos2(0.0, 240.0), egui::vec2(360.0, 420.0));
        let phone_primary =
            egui::Rect::from_min_size(egui::pos2(200.0, 660.0), egui::vec2(152.0, 56.0));
        let up = place_callout(
            phone_primary,
            Anchor::Above,
            size,
            phone_safe,
            phone_stage,
            &[],
        );
        assert!(phone_safe.contains_rect(up));
        let drawer_card =
            egui::Rect::from_min_size(egui::pos2(120.0, 740.0), egui::vec2(96.0, 134.0));
        let bottom_left =
            egui::Rect::from_min_size(egui::pos2(8.0, 668.0), egui::vec2(184.0, 48.0));
        let over_the_row = place_callout(
            drawer_card,
            Anchor::Above,
            size,
            phone_safe,
            phone_stage,
            &[phone_primary, bottom_left],
        );
        assert!(
            !over_the_row.intersects(phone_primary) && !over_the_row.intersects(bottom_left),
            "a mark on a drawer card never covers the primary row: {over_the_row:?}"
        );
        assert!(phone_safe.contains_rect(over_the_row));
        assert_eq!(anchor_side(Mark::Pass, true), Anchor::Above);
        assert_eq!(anchor_side(Mark::Pass, false), Anchor::LeftOf);
    }
}
