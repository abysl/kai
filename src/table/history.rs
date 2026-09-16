use super::*;
use plate::{classify_status, StatusLine};

pub const KEEP: usize = 200;
pub const TOASTS_KEPT: usize = toast::LOG_KEEP;
pub const TILE_W: f32 = hud::HISTORY_W;
pub const TILE_PITCH: f32 = hud::HISTORY_TILE_H;
pub const TILE_GAP: f32 = 4.0;
pub const TILE_H: f32 = TILE_PITCH - TILE_GAP;
pub const TILE_BORDER: f32 = 2.0;
pub const GLYPH: f32 = 14.0;
pub const SENTENCE_W: f32 = 240.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventClass {
    Play,
    Death,
    Conquer,
    Attach,
    Draw,
    Hide,
    Reveal,
    Trigger,
    Pass,
    Win,
    Note,
    Toast,
}

impl EventClass {
    pub fn on_rail(self) -> bool {
        !matches!(
            self,
            EventClass::Pass | EventClass::Note | EventClass::Toast
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            EventClass::Play => "play",
            EventClass::Death => "death",
            EventClass::Conquer => "conquer",
            EventClass::Attach => "attach",
            EventClass::Draw => "draw",
            EventClass::Hide => "hide",
            EventClass::Reveal => "reveal",
            EventClass::Trigger => "trigger",
            EventClass::Pass => "pass",
            EventClass::Win => "win",
            EventClass::Note => "note",
            EventClass::Toast => "refused",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub class: EventClass,
    pub seat: Option<u8>,
    pub card: Option<u32>,
    pub zone: Option<u16>,
    pub text: String,
    pub at: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    Seat(u8),
    Card(u32),
    Zone(u16),
}

pub fn tokens(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            break;
        };
        let token = &after[..end];
        if let Some((kind, index)) = token.split_once(' ') {
            let parsed = match kind {
                "seat" => index.parse().ok().map(Token::Seat),
                "card" => index.parse().ok().map(Token::Card),
                "zone" => index.parse().ok().map(Token::Zone),
                _ => None,
            };
            if let Some(parsed) = parsed {
                out.push(parsed);
            }
        }
        rest = &after[end + 1..];
    }
    out
}

pub fn verb(line: &str) -> &str {
    let rest = match line.find('}') {
        Some(end) if line.starts_with('{') => line[end + 1..].trim_start(),
        _ => line,
    };
    rest.split(|c: char| c.is_whitespace() || c == '·' || c == ':')
        .next()
        .unwrap_or("")
}

pub fn classify(line: &str) -> EventClass {
    let starts_with_seat = line.starts_with("{seat ");
    let starts_with_card = line.starts_with("{card ");
    let verb = verb(line);
    if starts_with_seat {
        return match verb {
            "plays" | "takes" => EventClass::Play,
            "activates" => EventClass::Trigger,
            "conquers" => EventClass::Conquer,
            "holds" | "keeps" | "wins" if line.contains("{zone ") => EventClass::Conquer,
            "wins" => EventClass::Win,
            "passes" => EventClass::Pass,
            "hides" => EventClass::Hide,
            "reveals" | "looks" => EventClass::Reveal,
            "draws" | "burns" | "channels" | "recycles" => EventClass::Draw,
            _ => EventClass::Note,
        };
    }
    if starts_with_card {
        return match verb {
            "dies" => EventClass::Death,
            "is" if line.contains("attached") || line.contains("recalled") => EventClass::Attach,
            "detaches" => EventClass::Attach,
            "triggers" => EventClass::Trigger,
            _ => EventClass::Note,
        };
    }
    EventClass::Note
}

pub fn event_of(line: &str, at: f64) -> Event {
    let class = classify(line);
    let found = tokens(line);
    let seat = found.iter().find_map(|token| match token {
        Token::Seat(seat) => Some(*seat),
        _ => None,
    });
    let card = found.iter().find_map(|token| match token {
        Token::Card(card) => Some(*card),
        _ => None,
    });
    let zone = found.iter().find_map(|token| match token {
        Token::Zone(zone) => Some(*zone),
        _ => None,
    });
    Event {
        class,
        seat,
        card,
        zone,
        text: line.to_string(),
        at,
    }
}

pub fn narration(view: &agni_sim::wire::PluginView) -> Vec<String> {
    if !view.narration.is_empty() {
        return view.narration.clone();
    }
    narration_lines(&view.status)
}

pub fn narration_lines(status: &[String]) -> Vec<String> {
    status
        .iter()
        .filter_map(|line| match classify_status(line) {
            StatusLine::Narration(text) => Some(text),
            _ => None,
        })
        .collect()
}

pub fn fresh<'a>(previous: &[String], window: &'a [String]) -> &'a [String] {
    let most = previous.len().min(window.len());
    for overlap in (1..=most).rev() {
        if previous[previous.len() - overlap..] == window[..overlap] {
            return &window[overlap..];
        }
    }
    window
}

#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct History {
    pub events: Vec<Event>,
    pub window: Vec<String>,
}

impl History {
    pub fn absorb(&mut self, window: &[String], at: f64) -> usize {
        let new: Vec<String> = fresh(&self.window, window).to_vec();
        for line in &new {
            self.events.push(event_of(line, at));
        }
        self.window = window.to_vec();
        self.trim();
        new.len()
    }

    pub fn toast(&mut self, text: &str, card: Option<u32>, at: f64) {
        self.events.push(Event {
            class: EventClass::Toast,
            seat: None,
            card,
            zone: None,
            text: text.to_string(),
            at,
        });
        self.trim();
    }

    fn trim(&mut self) {
        let toasts = self
            .events
            .iter()
            .filter(|event| event.class == EventClass::Toast)
            .count();
        if toasts > TOASTS_KEPT {
            let mut drop = toasts - TOASTS_KEPT;
            self.events.retain(|event| {
                if drop > 0 && event.class == EventClass::Toast {
                    drop -= 1;
                    false
                } else {
                    true
                }
            });
        }
        if self.events.len() > KEEP {
            let extra = self.events.len() - KEEP;
            self.events.drain(..extra);
        }
    }

    pub fn clear(&mut self) {
        self.events.clear();
        self.window.clear();
    }

    pub fn rail(&self, tiles: usize) -> Vec<&Event> {
        self.events
            .iter()
            .rev()
            .filter(|event| event.class.on_rail())
            .take(tiles)
            .collect()
    }

    pub fn toasts(&self) -> Vec<&Event> {
        self.events
            .iter()
            .filter(|event| event.class == EventClass::Toast)
            .collect()
    }
}

#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct HistoryHover(pub Option<Event>);

pub fn refresh_history(
    time: Res<Time>,
    panel: Res<plugin_ui::PluginPanel>,
    refusals: Res<toast::Refusals>,
    info: Res<SessionInfo>,
    mut history: ResMut<History>,
    mut last_toast: Local<Option<f64>>,
    mut last_role: Local<Option<SessionRole>>,
) {
    let now = time.elapsed_secs_f64();
    if *last_role != Some(info.role) {
        if last_role.is_some() && !matches!(info.role, SessionRole::Host | SessionRole::Client) {
            history.clear();
        }
        *last_role = Some(info.role);
    }
    if panel.is_changed() {
        let window = narration(&panel.view);
        if window != history.window {
            history.absorb(&window, now);
        }
    }
    if let Some(current) = &refusals.current {
        if *last_toast != Some(current.at) {
            *last_toast = Some(current.at);
            history.toast(&current.text, current.card, current.at);
        }
    }
}

pub fn paint_glyph(
    painter: &egui::Painter,
    rect: egui::Rect,
    class: EventClass,
    ink: egui::Color32,
) {
    let c = rect.center();
    let r = rect.width().min(rect.height()) / 2.0;
    let stroke = egui::Stroke::new(1.8, ink);
    let filled =
        |points: Vec<egui::Pos2>| egui::Shape::convex_polygon(points, ink, egui::Stroke::NONE);
    match class {
        EventClass::Play => {
            painter.add(filled(vec![
                egui::pos2(c.x - r * 0.7, c.y - r),
                egui::pos2(c.x + r, c.y),
                egui::pos2(c.x - r * 0.7, c.y + r),
            ]));
        }
        EventClass::Death => {
            painter.line_segment(
                [egui::pos2(c.x - r, c.y - r), egui::pos2(c.x + r, c.y + r)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(c.x - r, c.y + r), egui::pos2(c.x + r, c.y - r)],
                stroke,
            );
        }
        EventClass::Conquer => {
            painter.line_segment(
                [
                    egui::pos2(c.x - r * 0.7, c.y - r),
                    egui::pos2(c.x - r * 0.7, c.y + r),
                ],
                stroke,
            );
            painter.add(filled(vec![
                egui::pos2(c.x - r * 0.7, c.y - r),
                egui::pos2(c.x + r, c.y - r * 0.5),
                egui::pos2(c.x - r * 0.7, c.y),
            ]));
        }
        EventClass::Attach => {
            painter.circle_stroke(egui::pos2(c.x - r * 0.4, c.y), r * 0.55, stroke);
            painter.circle_stroke(egui::pos2(c.x + r * 0.4, c.y), r * 0.55, stroke);
        }
        EventClass::Draw => {
            painter.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(r * 1.3, r * 1.8)),
                2.0,
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        EventClass::Hide => {
            painter.rect_filled(
                egui::Rect::from_center_size(c, egui::vec2(r * 1.3, r * 1.8)),
                2.0,
                ink,
            );
        }
        EventClass::Reveal => {
            painter.circle_stroke(c, r * 0.9, stroke);
            painter.circle_filled(c, r * 0.35, ink);
        }
        EventClass::Trigger => {
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(c.x + r * 0.3, c.y - r),
                    egui::pos2(c.x - r * 0.5, c.y + r * 0.1),
                    egui::pos2(c.x + r * 0.2, c.y + r * 0.1),
                    egui::pos2(c.x - r * 0.3, c.y + r),
                ],
                stroke,
            ));
        }
        EventClass::Pass => {
            painter.line_segment([egui::pos2(c.x - r, c.y), egui::pos2(c.x + r, c.y)], stroke);
        }
        EventClass::Win => {
            painter.add(egui::Shape::Path(egui::epaint::PathShape {
                points: colors::glyph_points(colors::Glyph::Star, rect),
                closed: true,
                fill: ink,
                stroke: egui::epaint::PathStroke::NONE,
            }));
        }
        EventClass::Note => {
            painter.circle_filled(c, r * 0.4, ink);
        }
        EventClass::Toast => {
            painter.line_segment(
                [egui::pos2(c.x, c.y - r), egui::pos2(c.x, c.y + r * 0.3)],
                egui::Stroke::new(2.2, ink),
            );
            painter.circle_filled(egui::pos2(c.x, c.y + r * 0.8), r * 0.22, ink);
        }
    }
}

pub fn expand(event: &Event, seats: &hud::Seats) -> String {
    let table = &seats.table.0;
    let view = &seats.mirror.view;
    let me = seats.me();
    plugin_ui::expand(
        &event.text,
        &|seat| seats.name(seat),
        &|zone| plugin_ui::zone_label(&view.zones, zone),
        &|card| plugin_ui::card_label(table, view, me, card),
    )
}

pub fn tiles_in(rect: egui::Rect) -> usize {
    (rect.height() / TILE_PITCH).floor().max(0.0) as usize
}

#[allow(clippy::too_many_arguments)]
pub(super) fn history_ui(
    mut contexts: EguiContexts,
    hud: Res<hud::Hud>,
    history: Res<History>,
    seats: hud::Seats,
    menu: Res<crate::menu::Menu>,
    tuning: Res<Tuning>,
    mut art: hud::Art,
    mut hover: ResMut<HistoryHover>,
    mut selected: ResMut<Selected>,
    cards: Query<(Entity, &CardView)>,
) -> Result {
    let Some(rect) = hud.0.history else {
        if hover.0.is_some() {
            hover.0 = None;
        }
        return Ok(());
    };
    if !menu.at_table() {
        return Ok(());
    }
    let shown: Vec<Event> = history.rail(tiles_in(rect)).into_iter().cloned().collect();
    if shown.is_empty() {
        if hover.0.is_some() {
            hover.0 = None;
        }
        return Ok(());
    }
    let mut textures: Vec<Option<egui::TextureId>> = Vec::with_capacity(shown.len());
    for event in &shown {
        let name = event
            .card
            .and_then(|card| seats.table.0.get(CardId(card)))
            .filter(|card| !card.face.is_hidden())
            .map(|card| card.face.name.clone());
        textures.push(name.and_then(|name| art.texture(&mut contexts, &name)));
    }
    let context = contexts.ctx_mut()?.clone();
    let colour_blind = tuning.colour_blind;
    let mut hovered: Option<(usize, egui::Rect)> = None;
    let mut clicked: Option<u32> = None;
    egui::Area::new(egui::Id::new("history rail"))
        .fixed_pos(rect.min)
        .order(egui::Order::Middle)
        .show(&context, |ui| {
            ui.set_max_size(rect.size());
            for (index, event) in shown.iter().enumerate() {
                let tile = egui::Rect::from_min_size(
                    egui::pos2(rect.min.x, rect.min.y + index as f32 * TILE_PITCH),
                    egui::vec2(TILE_W, TILE_H),
                );
                let response = ui.interact(
                    tile,
                    egui::Id::new(("history tile", index)),
                    egui::Sense::click(),
                );
                let seat_rgb = event
                    .seat
                    .map(|seat| seats.label(PlayerId(seat)).1)
                    .unwrap_or([120, 120, 130]);
                let [r, g, b] = seat_rgb;
                let border = egui::Color32::from_rgb(r, g, b);
                let painter = ui.painter();
                match textures[index] {
                    Some(texture) => {
                        painter.image(
                            texture,
                            tile,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                    }
                    None => {
                        painter.rect_filled(tile, 4.0, hud::SURFACE_2);
                        let swatch = egui::Rect::from_center_size(
                            tile.center() - egui::vec2(0.0, 4.0),
                            egui::vec2(16.0, 16.0),
                        );
                        painter.rect_filled(swatch, 4.0, border);
                        if colour_blind {
                            if let Some(glyph) = colors::glyph_of_rgb(seat_rgb) {
                                colors::paint_glyph(painter, swatch.shrink(3.0), glyph, hud::INK);
                            }
                        }
                    }
                }
                painter.rect_stroke(
                    tile,
                    4.0,
                    egui::Stroke::new(TILE_BORDER, border),
                    egui::StrokeKind::Inside,
                );
                let badge = egui::Rect::from_min_size(
                    egui::pos2(tile.max.x - GLYPH - 2.0, tile.max.y - GLYPH - 2.0),
                    egui::vec2(GLYPH, GLYPH),
                );
                painter.circle_filled(badge.center(), GLYPH / 2.0 + 1.0, hud::SURFACE);
                paint_glyph(painter, badge.shrink(3.0), event.class, hud::INK);
                if response.hovered() {
                    hovered = Some((index, tile));
                }
                if response.clicked() {
                    clicked = event.card;
                }
            }
            if let Some((index, tile)) = hovered {
                let event = &shown[index];
                let sentence = expand(event, &seats);
                let painter = ui.painter();
                let galley = painter.layout(
                    sentence,
                    egui::FontId::proportional(12.0),
                    hud::INK,
                    SENTENCE_W,
                );
                let size = galley.size() + egui::vec2(16.0, 10.0);
                let at = egui::pos2(tile.max.x + hud::GAP, tile.min.y);
                let panel = egui::Rect::from_min_size(at, size);
                painter.rect_filled(panel, 6.0, hud::SURFACE);
                painter.rect_stroke(
                    panel,
                    6.0,
                    egui::Stroke::new(1.0, hud::HAIRLINE),
                    egui::StrokeKind::Inside,
                );
                painter.galley(at + egui::vec2(8.0, 5.0), galley, hud::INK);
            }
        });
    let next = hovered.map(|(index, _)| shown[index].clone());
    if hover.0 != next {
        hover.0 = next;
    }
    if let Some(card) = clicked {
        if let Some((entity, _)) = cards.iter().find(|(_, view)| view.0 .0 == card) {
            selected.0 = Some(entity);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(items: &[&str]) -> Vec<String> {
        items.iter().map(|line| (*line).to_string()).collect()
    }

    #[test]
    fn a_narration_line_is_classified_by_its_first_token_and_verb() {
        let table = [
            (
                "{seat 0} plays {card 12}",
                EventClass::Play,
                Some(0),
                Some(12),
                None,
            ),
            ("{card 12} dies", EventClass::Death, None, Some(12), None),
            (
                "{seat 1} conquers {zone 9}",
                EventClass::Conquer,
                Some(1),
                None,
                Some(9),
            ),
            (
                "{seat 1} holds {zone 9}",
                EventClass::Conquer,
                Some(1),
                None,
                Some(9),
            ),
            (
                "{seat 0} wins the combat at {zone 10}",
                EventClass::Conquer,
                Some(0),
                None,
                Some(10),
            ),
            (
                "{card 3} is attached to {card 4}",
                EventClass::Attach,
                None,
                Some(3),
                None,
            ),
            (
                "{card 3} detaches from {card 4}",
                EventClass::Attach,
                None,
                Some(3),
                None,
            ),
            (
                "{card 3} is recalled to base",
                EventClass::Attach,
                None,
                Some(3),
                None,
            ),
            ("{seat 0} draws 2", EventClass::Draw, Some(0), None, None),
            (
                "{seat 0} channels 2 runes exhausted",
                EventClass::Draw,
                Some(0),
                None,
                None,
            ),
            (
                "{seat 0} hides a card at {zone 9}",
                EventClass::Hide,
                Some(0),
                None,
                Some(9),
            ),
            (
                "{seat 0} reveals {card 5}",
                EventClass::Reveal,
                Some(0),
                Some(5),
                None,
            ),
            (
                "{card 5} triggers",
                EventClass::Trigger,
                None,
                Some(5),
                None,
            ),
            (
                "{seat 1} activates {card 6}",
                EventClass::Trigger,
                Some(1),
                Some(6),
                None,
            ),
            ("{seat 1} passes", EventClass::Pass, Some(1), None, None),
            (
                "{seat 1} passes · nothing to play",
                EventClass::Pass,
                Some(1),
                None,
                None,
            ),
            (
                "{seat 1} wins with 8 points",
                EventClass::Win,
                Some(1),
                None,
                None,
            ),
            (
                "{seat 0} spends 1 XP",
                EventClass::Note,
                Some(0),
                None,
                None,
            ),
            (
                "{seat 0} keeps their hand",
                EventClass::Note,
                Some(0),
                None,
                None,
            ),
            (
                "{card 7} trigger fizzles · no target",
                EventClass::Note,
                None,
                Some(7),
                None,
            ),
            ("no damage is dealt", EventClass::Note, None, None, None),
            (
                "{zone 9} is left empty",
                EventClass::Note,
                None,
                None,
                Some(9),
            ),
        ];
        for (line, class, seat, card, zone) in table {
            let event = event_of(line, 1.0);
            assert_eq!(event.class, class, "{line}");
            assert_eq!(event.seat, seat, "{line}");
            assert_eq!(event.card, card, "{line}");
            assert_eq!(event.zone, zone, "{line}");
            assert_eq!(event.text, line);
        }
        assert_eq!(verb("{seat 0} plays {card 1}"), "plays");
        assert_eq!(verb("plain words"), "plain");
        assert_eq!(
            tokens("{seat 0} takes {card 3} to {zone 4} {bogus}"),
            vec![Token::Seat(0), Token::Card(3), Token::Zone(4)]
        );
        assert!(EventClass::Play.on_rail());
        assert!(!EventClass::Pass.on_rail());
        assert!(!EventClass::Toast.on_rail());
        assert_eq!(EventClass::Toast.label(), "refused");
    }

    #[test]
    fn a_sliding_narration_window_adds_only_its_new_lines() {
        let mut history = History::default();
        assert_eq!(history.absorb(&lines(&["a", "b", "c"]), 1.0), 3);
        assert_eq!(history.absorb(&lines(&["a", "b", "c"]), 2.0), 0);
        assert_eq!(history.absorb(&lines(&["b", "c", "d", "e"]), 3.0), 2);
        assert_eq!(
            history.absorb(&lines(&["e", "e"]), 4.0),
            1,
            "a repeated line stays distinct"
        );
        assert_eq!(
            history.absorb(&lines(&["x", "y"]), 5.0),
            2,
            "an unrelated window is all new"
        );
        let texts: Vec<&str> = history
            .events
            .iter()
            .map(|event| event.text.as_str())
            .collect();
        assert_eq!(texts, ["a", "b", "c", "d", "e", "e", "x", "y"]);
        assert_eq!(history.window, lines(&["x", "y"]));
        assert!(fresh(&[], &lines(&["q"])).len() == 1);
        assert!(history.absorb(&[], 6.0) == 0);
        history.clear();
        assert!(history.events.is_empty() && history.window.is_empty());
    }

    #[test]
    fn the_log_keeps_the_last_five_toasts() {
        let mut history = History::default();
        history.absorb(&lines(&["{seat 0} plays {card 1}"]), 0.5);
        for index in 0..7 {
            history.toast(
                &format!("refused {index}"),
                Some(index),
                1.0 + f64::from(index),
            );
        }
        let toasts: Vec<&str> = history
            .toasts()
            .iter()
            .map(|event| event.text.as_str())
            .collect();
        assert_eq!(
            toasts,
            [
                "refused 2",
                "refused 3",
                "refused 4",
                "refused 5",
                "refused 6"
            ]
        );
        assert_eq!(TOASTS_KEPT, 5);
        assert_eq!(
            history.events[0].class,
            EventClass::Play,
            "narration is not evicted by toasts"
        );
        assert_eq!(history.events.len(), 6);
        assert!(history
            .rail(8)
            .iter()
            .all(|event| event.class != EventClass::Toast));
    }

    #[test]
    fn the_rail_lists_the_newest_board_events_first_and_skips_passes() {
        let mut history = History::default();
        history.absorb(
            &lines(&[
                "{seat 0} plays {card 1}",
                "{seat 1} passes",
                "{card 1} dies",
                "{seat 0} conquers {zone 9}",
            ]),
            1.0,
        );
        let rail: Vec<EventClass> = history.rail(8).iter().map(|event| event.class).collect();
        assert_eq!(
            rail,
            [EventClass::Conquer, EventClass::Death, EventClass::Play]
        );
        assert_eq!(history.rail(1).len(), 1);
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(40.0, 8.0 * 56.0));
        assert_eq!(tiles_in(rect), 8);
        assert_eq!(
            tiles_in(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(40.0, 100.0)
            )),
            1
        );
        for _ in 0..KEEP {
            history.absorb(&lines(&["{seat 0} passes", "{seat 1} passes"]), 2.0);
            history.window.clear();
        }
        assert_eq!(history.events.len(), KEEP);
    }

    #[test]
    fn only_the_narration_lines_of_a_status_feed_the_history() {
        let status = lines(&[
            "turn 3 · {seat 0} · action phase · rules enforced",
            "points · {seat 0} 0 · {seat 1} 0",
            "{seat 0} plays {card 4}",
            "{card 4} dies",
        ]);
        assert_eq!(
            narration_lines(&status),
            lines(&["{seat 0} plays {card 4}", "{card 4} dies"])
        );
        let parsed = agni_sim::wire::PluginView {
            status: status.clone(),
            ..Default::default()
        };
        assert_eq!(
            narration(&parsed),
            lines(&["{seat 0} plays {card 4}", "{card 4} dies"]),
            "without the field the classifier is the source"
        );
        let told = agni_sim::wire::PluginView {
            status,
            narration: lines(&[
                "{seat 0} plays {card 4}",
                "{card 4} dies",
                "{seat 1} passes",
            ]),
            ..Default::default()
        };
        assert_eq!(
            narration(&told),
            told.narration,
            "the structured narration wins over the parsed lines"
        );
    }
}
