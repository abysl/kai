use super::*;
use crate::viewport::{Viewport, ViewportClass};
use bevy_egui::egui::{pos2, vec2, Rect};

pub const GUTTER: f32 = 12.0;
pub const GAP: f32 = 8.0;
pub const MENU_BUTTON: f32 = 40.0;
pub const TURN_PLATE_W: f32 = 200.0;
pub const TURN_PLATE_H: f32 = 48.0;
pub const PHASE_BAR_H: f32 = 20.0;
pub const STRIP_MAX_W: f32 = 560.0;
pub const STRIP_ROW_H: f32 = 32.0;
pub const TOAST_H: f32 = 32.0;
pub const COLUMN_DESKTOP: f32 = 220.0;
pub const COLUMN_TABLET: f32 = 200.0;
pub const CHAIN_HEADER_H: f32 = 28.0;
pub const CHAIN_ROW_H: f32 = 80.0;
pub const CHAIN_MORE_H: f32 = 20.0;
pub const CHAIN_PAD: f32 = 16.0;
pub const CHAIN_FULL_ROWS: usize = 3;
pub const PLATE_H: f32 = 64.0;
pub const PLATE_COMPACT_H: f32 = 40.0;
pub const HISTORY_W: f32 = 40.0;
pub const HISTORY_TILE_H: f32 = 56.0;
pub const HISTORY_MAX_TILES: usize = 8;
pub const INSPECTOR_W: f32 = 198.0;
pub const INSPECTOR_CAPTION_H: f32 = 56.0;
pub const PRIMARY_W: f32 = 168.0;
pub const PRIMARY_H: f32 = 56.0;
pub const SECONDARY_H: f32 = 36.0;
pub const TAB_W: f32 = 40.0;
pub const TAB_H: f32 = 28.0;
pub const DRAWER_W: f32 = 300.0;
pub const HAND_MARGIN: f32 = 192.0;
pub const HAND_FRACTION: f32 = 0.28;
pub const HAND_MIN_W: f32 = 120.0;
pub const BANNER_MAX_W: f32 = 480.0;
pub const BANNER_MAX_H: f32 = 220.0;
pub const TOUCH_MIN: f32 = 48.0;
pub const SHEET_W: f32 = 380.0;
pub const WIDE_SHEET_W_DESKTOP: f32 = 760.0;
pub const WIDE_SHEET_W_TABLET: f32 = 640.0;

#[derive(bevy::ecs::system::SystemParam)]
pub struct Seats<'w> {
    pub table: Res<'w, GameTable>,
    pub mirror: Res<'w, Mirror>,
    pub info: Res<'w, SessionInfo>,
    pub my_seat: Res<'w, MySeat>,
    pub players: Res<'w, PlayerCount>,
    pub colors: Res<'w, colors::SeatColors>,
}

impl Seats<'_> {
    pub fn label(&self, seat: PlayerId) -> (String, [u8; 3], bool) {
        colors::seat_label(&self.info.roster, &self.colors, self.my_seat.0, seat)
    }

    pub fn name(&self, seat: u8) -> String {
        self.label(PlayerId(seat)).0
    }

    pub fn me(&self) -> PlayerId {
        self.my_seat.0
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct Art<'w> {
    pub cache: ResMut<'w, art::ArtCache>,
    pub images: ResMut<'w, Assets<Image>>,
    pub registry: ResMut<'w, crate::render::egui_art::EguiArt>,
}

impl Art<'_> {
    pub fn texture(&mut self, contexts: &mut EguiContexts, name: &str) -> Option<egui::TextureId> {
        let handle = self.cache.image(name, &mut self.images)?;
        Some(self.registry.texture(contexts, &handle))
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct Sender<'w> {
    pub secrets: ResMut<'w, plugin_ui::RollSecrets>,
    pub requests: MessageWriter<'w, plugin_ui::PluginActionRequested>,
    pub activity: ResMut<'w, super::primary::TurnActivity>,
}

impl Sender<'_> {
    pub fn fire(&mut self, affordance: &agni_sim::wire::Affordance) {
        if let Some(bytes) =
            plugin_ui::action_bytes(affordance, &mut self.secrets, &crate::os::entropy::secret)
        {
            self.activity.note_action(affordance);
            self.requests
                .write(plugin_ui::PluginActionRequested(bytes, affordance.card));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DrawerState {
    #[default]
    Tucked,
    Raised,
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct Insets {
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HudRects {
    pub class: ViewportClass,
    pub safe: Rect,
    pub menu_button: Rect,
    pub bug_button: Rect,
    pub turn_plate: Rect,
    pub phase_bar: Option<Rect>,
    pub strip: Rect,
    pub toast: Rect,
    pub chain: Rect,
    pub chain_rows: usize,
    pub seats: Rect,
    pub plate_h: f32,
    pub history: Option<Rect>,
    pub inspector: Option<Rect>,
    pub secondary: Rect,
    pub primary: Rect,
    pub drawer_tabs: Option<Rect>,
    pub drawer: Option<Rect>,
    pub hand: Rect,
    pub banner: Rect,
    pub stage: Rect,
    pub frame: Rect,
    pub top_bar: Option<Rect>,
    pub opp_strip: Option<Rect>,
    pub ticker: Option<Rect>,
    pub chain_rail: Option<Rect>,
    pub bottom_left: Option<Rect>,
    pub drawer_state: DrawerState,
}

impl HudRects {
    pub fn placed(&self) -> Vec<(&'static str, Rect)> {
        let mut out = vec![
            ("menu_button", self.menu_button),
            ("bug_button", self.bug_button),
            ("turn_plate", self.turn_plate),
            ("strip", self.strip),
            ("seats", self.seats),
            ("secondary", self.secondary),
            ("primary", self.primary),
            ("hand", self.hand),
        ];
        if !self.class.is_phone() {
            out.push(("toast", self.toast));
            out.push(("chain", self.chain));
        }
        if let Some(rect) = self.opp_strip {
            out.push(("opp_strip", rect));
        }
        if let Some(rect) = self.ticker.filter(|rect| rect.height() > 0.0) {
            out.push(("ticker", rect));
        }
        if let Some(rect) = self.chain_rail.filter(|rect| rect.height() > 0.0) {
            out.push(("chain_rail", rect));
        }
        if let Some(rect) = self.bottom_left {
            out.push(("bottom_left", rect));
        }
        if let Some(rect) = self.phase_bar {
            out.push(("phase_bar", rect));
        }
        if let Some(rect) = self.history {
            out.push(("history", rect));
        }
        if let Some(rect) = self.inspector {
            out.push(("inspector", rect));
        }
        if let Some(rect) = self.drawer_tabs {
            out.push(("drawer_tabs", rect));
        }
        if let Some(rect) = self.drawer {
            out.push(("drawer", rect));
        }
        out
    }

    pub fn touch(&self) -> Vec<(&'static str, Rect)> {
        let mut out = vec![
            ("menu_button", self.menu_button),
            ("bug_button", self.bug_button),
            ("primary", self.primary),
        ];
        if let Some(rect) = self.drawer_tabs {
            out.push(("drawer_tabs", rect));
        }
        out
    }

    pub fn column_x(&self) -> f32 {
        self.chain.min.x
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Metrics {
    gutter: f32,
    menu: f32,
    turn_plate: egui::Vec2,
    phase_bar: bool,
    strip_rows: usize,
    strip_below_top_row: bool,
    column: f32,
    plate_h: f32,
    history: bool,
    inspector: bool,
    inspector_cap: f32,
    tabs: bool,
    hand_margin: f32,
    hand_fraction: f32,
    primary_above_hand: bool,
    stage_min_fraction: f32,
}

impl Metrics {
    fn of(class: ViewportClass) -> Self {
        match class {
            ViewportClass::Desktop => Self {
                gutter: GUTTER,
                menu: MENU_BUTTON,
                turn_plate: vec2(TURN_PLATE_W, TURN_PLATE_H),
                phase_bar: true,
                strip_rows: 3,
                strip_below_top_row: false,
                column: COLUMN_DESKTOP,
                plate_h: PLATE_H,
                history: true,
                inspector: true,
                inspector_cap: 0.18,
                tabs: true,
                hand_margin: HAND_MARGIN,
                hand_fraction: HAND_FRACTION,
                primary_above_hand: false,
                stage_min_fraction: 0.0,
            },
            ViewportClass::Tablet => Self {
                column: COLUMN_TABLET,
                inspector_cap: 0.16,
                ..Self::of(ViewportClass::Desktop)
            },
            ViewportClass::PhoneLandscape => Self::compact(1, false),
            ViewportClass::PhonePortrait => Self::compact(2, true),
        }
    }

    fn compact(strip_rows: usize, portrait: bool) -> Self {
        Self {
            gutter: GAP,
            menu: TOUCH_MIN,
            turn_plate: vec2(120.0, 44.0),
            phase_bar: false,
            strip_rows,
            strip_below_top_row: portrait,
            column: 150.0,
            plate_h: PLATE_COMPACT_H,
            history: false,
            inspector: false,
            inspector_cap: 0.0,
            tabs: false,
            hand_margin: if portrait { 54.0 } else { 192.0 },
            hand_fraction: if portrait { 0.2 } else { 0.16 },
            primary_above_hand: portrait,
            stage_min_fraction: 0.42,
        }
    }
}

pub fn chain_height(shown_rows: usize, len: usize) -> f32 {
    let more = if len > shown_rows { CHAIN_MORE_H } else { 0.0 };
    CHAIN_HEADER_H + shown_rows as f32 * CHAIN_ROW_H + more + CHAIN_PAD
}

pub fn chain_rows_shown(len: usize) -> usize {
    len.min(CHAIN_FULL_ROWS)
}

fn safe_rect(screen: Rect, insets: Insets) -> Rect {
    Rect::from_min_max(
        pos2(screen.min.x + insets.left, screen.min.y + insets.top),
        pos2(screen.max.x - insets.right, screen.max.y - insets.bottom),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StripFit {
    Idle,
    Collapsed,
    Rows(usize),
    #[default]
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Fit {
    pub strip: StripFit,
    pub hand_left: bool,
}

impl Fit {
    pub fn pending(pending: bool) -> Self {
        Self {
            strip: if pending {
                StripFit::Full
            } else {
                StripFit::Idle
            },
            hand_left: false,
        }
    }
}

pub fn layout(
    class: ViewportClass,
    screen: Rect,
    insets: Insets,
    drawer: DrawerState,
    chain_len: usize,
    seats: usize,
) -> HudRects {
    layout_pending(
        class,
        screen,
        insets,
        drawer,
        chain_len,
        seats,
        Fit::default(),
    )
}

pub fn layout_pending(
    class: ViewportClass,
    screen: Rect,
    insets: Insets,
    drawer: DrawerState,
    chain_len: usize,
    seats: usize,
    fit: Fit,
) -> HudRects {
    match class {
        ViewportClass::PhonePortrait => {
            return layout_phone_portrait(screen, insets, drawer, chain_len, seats, fit)
        }
        ViewportClass::PhoneLandscape => {
            return layout_phone_landscape(screen, insets, drawer, chain_len, seats, fit)
        }
        ViewportClass::Tablet | ViewportClass::Desktop => {}
    }
    let drawer_state = drawer;
    let m = Metrics::of(class);
    let g = m.gutter;
    let safe = safe_rect(screen, insets);
    let seats = seats.max(1);

    let menu_button =
        Rect::from_min_size(pos2(safe.min.x + g, safe.min.y + g), vec2(m.menu, m.menu));

    let hand_margin = m
        .hand_margin
        .min((safe.width() - HAND_MIN_W) / 2.0)
        .max(0.0);
    let hand_h = (safe.height() * m.hand_fraction).round();
    let hand = Rect::from_min_max(
        pos2(safe.min.x + hand_margin, safe.max.y - hand_h),
        pos2(safe.max.x - hand_margin, safe.max.y),
    );

    let right = safe.max.x - g;
    let bottom = safe.max.y - g;
    let drawer_tabs = m.tabs.then(|| {
        Rect::from_min_size(
            pos2(right - PRIMARY_W, bottom - TAB_H),
            vec2(PRIMARY_W, TAB_H),
        )
    });
    let primary_bottom = if m.primary_above_hand {
        hand.min.y - GAP
    } else {
        drawer_tabs.map_or(bottom, |tabs| tabs.min.y - 4.0)
    };
    let primary = Rect::from_min_size(
        pos2(right - PRIMARY_W, primary_bottom - PRIMARY_H),
        vec2(PRIMARY_W, PRIMARY_H),
    );
    let secondary = Rect::from_min_size(
        pos2(right - PRIMARY_W, primary.min.y - GAP - SECONDARY_H),
        vec2(PRIMARY_W, SECONDARY_H),
    );

    let drawer = (drawer == DrawerState::Raised && !class.is_phone()).then(|| {
        Rect::from_min_max(
            pos2((safe.max.x - DRAWER_W).max(safe.min.x), safe.min.y + g),
            pos2(safe.max.x, secondary.min.y.min(hand.min.y) - GAP),
        )
    });
    let column_right = drawer.map_or(right, |drawer| drawer.min.x - GAP);
    let column_x = column_right - m.column;

    let bug_button = bug_rect(menu_button);
    let turn_x = bug_button.max.x + GAP;
    let turn_w = if m.strip_below_top_row {
        m.turn_plate.x.min(safe.max.x - g - turn_x)
    } else {
        m.turn_plate.x.min(column_x - GAP - turn_x)
    }
    .max(TOUCH_MIN);
    let turn_plate =
        Rect::from_min_size(pos2(turn_x, safe.min.y + g), vec2(turn_w, m.turn_plate.y));
    let phase_bar = m.phase_bar.then(|| {
        Rect::from_min_size(
            pos2(turn_x, turn_plate.max.y + GAP),
            vec2(turn_w, PHASE_BAR_H),
        )
    });
    let top_row_bottom = phase_bar
        .map_or(turn_plate.max.y, |bar| bar.max.y)
        .max(menu_button.max.y);

    let strip_h = m.strip_rows as f32 * STRIP_ROW_H;
    let strip = if m.strip_below_top_row {
        Rect::from_min_size(
            pos2(safe.min.x + g, top_row_bottom + GAP),
            vec2(safe.width() - 2.0 * g, strip_h),
        )
    } else {
        let span_min = turn_plate.max.x + GAP;
        let span_max = column_x - GAP;
        let w = STRIP_MAX_W.min(span_max - span_min).max(0.0);
        let x = (safe.center().x - w / 2.0).clamp(span_min, (span_max - w).max(span_min));
        Rect::from_min_size(pos2(x, safe.min.y + g), vec2(w, strip_h))
    };
    let toast = Rect::from_min_size(
        pos2(strip.min.x, strip.max.y + GAP),
        vec2(strip.width(), TOAST_H),
    );

    let column_top = if m.strip_below_top_row {
        toast.max.y + GAP
    } else {
        safe.min.y + g
    };
    let column_limit = secondary.min.y.min(hand.min.y) - GAP;
    let budget = column_limit - column_top;
    let mut chain_rows = chain_rows_shown(chain_len);
    let mut plate_h = m.plate_h;
    loop {
        let need = chain_height(chain_rows, chain_len) + GAP + plate_h * seats as f32;
        if need <= budget {
            break;
        }
        if chain_rows > 1 {
            chain_rows = 1;
        } else if chain_rows == 1 {
            chain_rows = 0;
        } else if plate_h > PLATE_COMPACT_H {
            plate_h = PLATE_COMPACT_H;
        } else {
            break;
        }
    }
    let chain_h = chain_height(chain_rows, chain_len).min(budget.max(CHAIN_HEADER_H));
    let chain = Rect::from_min_size(pos2(column_x, column_top), vec2(m.column, chain_h));
    let seats_h = (plate_h * seats as f32).min((column_limit - chain.max.y - GAP).max(0.0));
    let seats_rect =
        Rect::from_min_size(pos2(column_x, chain.max.y + GAP), vec2(m.column, seats_h));

    let inspector = m.inspector.then(|| {
        let w = INSPECTOR_W
            .min(safe.width() * m.inspector_cap)
            .min(hand.min.x - safe.min.x - 2.0 * g)
            .max(TOUCH_MIN);
        let h = (w * dim::CARD_H / dim::CARD_W + INSPECTOR_CAPTION_H).min(safe.height() * 0.42);
        Rect::from_min_size(pos2(safe.min.x + g, bottom - h), vec2(w, h))
    });
    let history = m
        .history
        .then(|| {
            let top = top_row_bottom + GAP;
            let floor = inspector.map_or(bottom, |rect| rect.min.y - GAP);
            let tiles = ((floor - top) / HISTORY_TILE_H).floor().max(0.0) as usize;
            let tiles = tiles.min(HISTORY_MAX_TILES);
            (tiles > 0).then(|| {
                Rect::from_min_size(
                    pos2(safe.min.x + g, top),
                    vec2(HISTORY_W, tiles as f32 * HISTORY_TILE_H),
                )
            })
        })
        .flatten();

    let stage = Rect::from_min_max(
        pos2(safe.min.x, toast.max.y + GAP),
        pos2(safe.max.x, hand.min.y - GAP),
    );
    let banner_w = BANNER_MAX_W.min(stage.width() - 2.0 * g).max(0.0);
    let banner_h = BANNER_MAX_H.min(stage.height() * 0.5).max(0.0);
    let banner = Rect::from_center_size(stage.center(), vec2(banner_w, banner_h));

    HudRects {
        class,
        safe,
        menu_button,
        bug_button,
        turn_plate,
        phase_bar,
        strip,
        toast,
        chain,
        chain_rows,
        seats: seats_rect,
        plate_h,
        history,
        inspector,
        secondary,
        primary,
        drawer_tabs,
        drawer,
        hand,
        banner,
        stage,
        frame: stage,
        top_bar: None,
        opp_strip: None,
        ticker: None,
        chain_rail: None,
        bottom_left: None,
        drawer_state,
    }
}

pub const PHONE_TOP_BAR_H: f32 = 56.0;
pub const LANDSCAPE_TOP_BAR_H: f32 = 56.0;
pub const OPP_STRIP_H: f32 = 64.0;
pub const PHONE_BANNER_ROWS: usize = 3;
pub const LANDSCAPE_BANNER_ROWS: usize = 3;
pub const TICKER_H: f32 = 32.0;
pub const CHAIN_RAIL_H: f32 = 56.0;
pub const DRAWER_TUCKED_PORTRAIT: f32 = 72.0;
pub const DRAWER_RAISED_PORTRAIT: f32 = 188.0;
pub const DRAWER_TUCKED_LANDSCAPE: f32 = 56.0;
pub const DRAWER_RAISED_LANDSCAPE: f32 = 168.0;
pub const PHONE_PRIMARY_W: f32 = 152.0;
pub const LANDSCAPE_PRIMARY_W: f32 = 120.0;
pub const LANDSCAPE_PRIMARY_H: f32 = 48.0;
pub const PHONE_SECONDARY_H: f32 = 40.0;
pub const LANDSCAPE_SECONDARY_W: f32 = 96.0;
pub const BOTTOM_LEFT_H: f32 = 48.0;
pub const BOTTOM_LEFT_W: f32 = 160.0;
pub const PHONE_SEATS_W: f32 = 148.0;
pub const PORTRAIT_SEATS_W: f32 = 168.0;
pub const OPP_THUMBS_MIN_W: f32 = 150.0;
pub const LANDSCAPE_RAIL_W: f32 = 160.0;
pub const PLATE_PHONE_MIN_H: f32 = 20.0;
pub const LANDSCAPE_BANNER_MARGIN: f32 = 176.0;
pub const LANDSCAPE_TURN_PLATE_W: f32 = 200.0;
pub const BANNER_COLLAPSED_H: f32 = 28.0;

pub fn drawer_height(class: ViewportClass, state: DrawerState) -> f32 {
    match (class, state) {
        (ViewportClass::PhonePortrait, DrawerState::Tucked) => DRAWER_TUCKED_PORTRAIT,
        (ViewportClass::PhonePortrait, DrawerState::Raised) => DRAWER_RAISED_PORTRAIT,
        (ViewportClass::PhoneLandscape, DrawerState::Tucked) => DRAWER_TUCKED_LANDSCAPE,
        (ViewportClass::PhoneLandscape, DrawerState::Raised) => DRAWER_RAISED_LANDSCAPE,
        _ => 0.0,
    }
}

fn phone_plate_h(bar_h: f32, seats: usize) -> f32 {
    (bar_h / seats as f32).clamp(PLATE_PHONE_MIN_H, PLATE_COMPACT_H)
}

fn banner_in(stage: Rect, g: f32) -> Rect {
    let banner_w = BANNER_MAX_W.min(stage.width() - 2.0 * g).max(0.0);
    let banner_h = BANNER_MAX_H.min(stage.height() * 0.5).max(0.0);
    Rect::from_center_size(stage.center(), vec2(banner_w, banner_h))
}

fn strip_height(fit: StripFit, rows: usize) -> f32 {
    match fit {
        StripFit::Idle => 0.0,
        StripFit::Collapsed => BANNER_COLLAPSED_H,
        StripFit::Rows(needed) => needed.clamp(1, rows) as f32 * STRIP_ROW_H,
        StripFit::Full => rows as f32 * STRIP_ROW_H,
    }
}

pub fn rail_height(chain_len: usize) -> f32 {
    if chain_len == 0 {
        0.0
    } else {
        CHAIN_RAIL_H
    }
}

fn mirrored(rect: Rect, safe: Rect) -> Rect {
    Rect::from_min_max(
        pos2(safe.min.x + (safe.max.x - rect.max.x), rect.min.y),
        pos2(safe.max.x - (rect.min.x - safe.min.x), rect.max.y),
    )
}

fn handed(rects: [Rect; 3], safe: Rect, hand_left: bool) -> [Rect; 3] {
    if hand_left {
        rects.map(|rect| mirrored(rect, safe))
    } else {
        rects
    }
}

fn layout_phone_portrait(
    screen: Rect,
    insets: Insets,
    drawer: DrawerState,
    chain_len: usize,
    seats: usize,
    fit: Fit,
) -> HudRects {
    let class = ViewportClass::PhonePortrait;
    let g = GAP;
    let safe = safe_rect(screen, insets);
    let seats = seats.max(1);
    let inner_w = (safe.width() - 2.0 * g).max(0.0);

    let top_bar = Rect::from_min_size(safe.min, vec2(safe.width(), PHONE_TOP_BAR_H));
    let menu_button = Rect::from_min_size(
        pos2(safe.min.x + 4.0, safe.min.y + 4.0),
        vec2(TOUCH_MIN, TOUCH_MIN),
    );
    let bug_button = bug_rect(menu_button);
    let turn_x = bug_button.max.x + g;
    let turn_plate = Rect::from_min_max(
        pos2(turn_x, safe.min.y + 4.0),
        pos2(
            (safe.max.x - g).max(turn_x + TOUCH_MIN),
            safe.min.y + 4.0 + TOUCH_MIN,
        ),
    );
    let seats_w = PORTRAIT_SEATS_W.min(safe.width() * 0.6);
    let plate_h = phone_plate_h(OPP_STRIP_H, seats);
    let seats_rect = Rect::from_min_size(
        pos2(safe.max.x - g - seats_w, top_bar.max.y),
        vec2(seats_w, (plate_h * seats as f32).min(OPP_STRIP_H)),
    );
    let opp_strip = Rect::from_min_max(
        pos2(safe.min.x + g, top_bar.max.y),
        pos2(
            (seats_rect.min.x - g).max(safe.min.x + g + TOUCH_MIN),
            top_bar.max.y + OPP_STRIP_H,
        ),
    );
    let strip = Rect::from_min_size(
        pos2(safe.min.x + g, opp_strip.max.y),
        vec2(inner_w, strip_height(fit.strip, PHONE_BANNER_ROWS)),
    );
    let ticker = Rect::from_min_size(pos2(safe.min.x + g, strip.max.y), vec2(inner_w, TICKER_H));
    let chain = Rect::from_min_size(
        pos2(safe.min.x + g, ticker.max.y),
        vec2(inner_w, rail_height(chain_len)),
    );

    let row_for = |state: DrawerState| {
        let hand = Rect::from_min_max(
            pos2(safe.min.x, safe.max.y - drawer_height(class, state)),
            safe.max,
        );
        let primary_w = PHONE_PRIMARY_W.min(safe.width() / 2.0 - g).max(TOUCH_MIN);
        let primary = Rect::from_min_size(
            pos2(safe.max.x - g - primary_w, hand.min.y - g - PRIMARY_H),
            vec2(primary_w, PRIMARY_H),
        );
        let secondary = Rect::from_min_size(
            pos2(primary.min.x, primary.min.y - g - PHONE_SECONDARY_H),
            vec2(primary_w, PHONE_SECONDARY_H),
        );
        let bottom_left = Rect::from_min_max(
            pos2(safe.min.x + g, primary.max.y - BOTTOM_LEFT_H),
            pos2(primary.min.x - g, primary.max.y),
        );
        (hand, primary, secondary, bottom_left)
    };
    let (hand, primary, secondary, bottom_left) = row_for(drawer);
    let [primary, secondary, bottom_left] =
        handed([primary, secondary, bottom_left], safe, fit.hand_left);
    let stage_top = chain.max.y + g;
    let stage = Rect::from_min_max(
        pos2(safe.min.x, stage_top),
        pos2(safe.max.x, primary.min.y - g),
    );
    let (_, tucked_primary, _, _) = row_for(DrawerState::Tucked);
    let frame = Rect::from_min_max(
        pos2(safe.min.x, stage_top),
        pos2(safe.max.x, tucked_primary.min.y - g),
    );
    let banner = banner_in(stage, g);

    HudRects {
        class,
        safe,
        menu_button,
        bug_button,
        turn_plate,
        phase_bar: None,
        strip,
        toast: ticker,
        chain,
        chain_rows: 0,
        seats: seats_rect,
        plate_h,
        history: None,
        inspector: None,
        secondary,
        primary,
        drawer_tabs: None,
        drawer: None,
        hand,
        banner,
        stage,
        frame,
        top_bar: Some(top_bar),
        opp_strip: Some(opp_strip),
        ticker: Some(ticker),
        chain_rail: Some(chain),
        bottom_left: Some(bottom_left),
        drawer_state: drawer,
    }
}

fn layout_phone_landscape(
    screen: Rect,
    insets: Insets,
    drawer: DrawerState,
    chain_len: usize,
    seats: usize,
    fit: Fit,
) -> HudRects {
    let class = ViewportClass::PhoneLandscape;
    let g = GAP;
    let safe = safe_rect(screen, insets);
    let seats = seats.max(1);

    let top_bar = Rect::from_min_size(safe.min, vec2(safe.width(), LANDSCAPE_TOP_BAR_H));
    let menu_button = Rect::from_min_size(
        pos2(safe.min.x + 4.0, safe.min.y + 4.0),
        vec2(TOUCH_MIN, TOUCH_MIN),
    );
    let seats_w = PHONE_SEATS_W.min(safe.width() * 0.25);
    let plate_h = phone_plate_h(LANDSCAPE_TOP_BAR_H, seats);
    let seats_rect = Rect::from_min_size(
        pos2(safe.max.x - g - seats_w, safe.min.y),
        vec2(seats_w, (plate_h * seats as f32).min(LANDSCAPE_TOP_BAR_H)),
    );
    let bug_button = bug_rect(menu_button);
    let turn_x = bug_button.max.x + g;
    let turn_w = LANDSCAPE_TURN_PLATE_W
        .min((seats_rect.min.x - g - turn_x) / 2.0)
        .max(TOUCH_MIN);
    let turn_plate = Rect::from_min_size(pos2(turn_x, safe.min.y + 4.0), vec2(turn_w, TOUCH_MIN));
    let opp_strip = Rect::from_min_max(
        pos2(turn_plate.max.x + g, safe.min.y + 4.0),
        pos2(
            (seats_rect.min.x - g).max(turn_plate.max.x + g),
            safe.min.y + LANDSCAPE_TOP_BAR_H - 4.0,
        ),
    );

    let banner_w = BANNER_MAX_W
        .min(safe.width() - 2.0 * LANDSCAPE_BANNER_MARGIN)
        .max((safe.width() * 0.4).min(safe.width()));
    let banner_x = safe.center().x - banner_w / 2.0;
    let row_for = |state: DrawerState| {
        let hand = Rect::from_min_max(
            pos2(safe.min.x, safe.max.y - drawer_height(class, state)),
            safe.max,
        );
        let primary = Rect::from_min_size(
            pos2(
                safe.max.x - g - LANDSCAPE_PRIMARY_W,
                hand.min.y - g - LANDSCAPE_PRIMARY_H,
            ),
            vec2(LANDSCAPE_PRIMARY_W, LANDSCAPE_PRIMARY_H),
        );
        let secondary = Rect::from_min_size(
            pos2(
                primary.max.x - LANDSCAPE_SECONDARY_W,
                primary.min.y - g - PHONE_SECONDARY_H,
            ),
            vec2(LANDSCAPE_SECONDARY_W, PHONE_SECONDARY_H),
        );
        let bottom_left = Rect::from_min_max(
            pos2(safe.min.x + g, primary.max.y - BOTTOM_LEFT_H),
            pos2(
                (safe.min.x + g + BOTTOM_LEFT_W).min(primary.min.x - g),
                primary.max.y,
            ),
        );
        (hand, primary, secondary, bottom_left)
    };
    let (hand, primary, secondary, bottom_left) = row_for(drawer);
    let [primary, secondary, bottom_left] =
        handed([primary, secondary, bottom_left], safe, fit.hand_left);
    let strip = Rect::from_min_size(
        pos2(banner_x, top_bar.max.y + 4.0),
        vec2(banner_w, strip_height(fit.strip, LANDSCAPE_BANNER_ROWS)),
    );
    let ticker_h = match drawer {
        DrawerState::Tucked => TICKER_H,
        DrawerState::Raised => 0.0,
    };
    let ticker = Rect::from_min_size(pos2(banner_x, strip.max.y), vec2(banner_w, ticker_h));
    let rail_w = LANDSCAPE_RAIL_W.min((banner_x - 2.0 * g).max(TOUCH_MIN));
    let rail_h = rail_height(chain_len).min((hand.min.y - g - top_bar.max.y - 4.0).max(0.0));
    let chain = Rect::from_min_size(
        pos2(safe.min.x + g, top_bar.max.y + 4.0),
        vec2(rail_w, rail_h),
    );
    let stage = Rect::from_min_max(
        pos2(safe.min.x, top_bar.max.y),
        pos2(safe.max.x, primary.min.y - g),
    );
    let (_, tucked_primary, _, _) = row_for(DrawerState::Tucked);
    let frame = Rect::from_min_max(
        pos2(safe.min.x, top_bar.max.y),
        pos2(safe.max.x, tucked_primary.min.y - g),
    );
    let banner = banner_in(stage, g);

    HudRects {
        class,
        safe,
        menu_button,
        bug_button,
        turn_plate,
        phase_bar: None,
        strip,
        toast: ticker,
        chain,
        chain_rows: 0,
        seats: seats_rect,
        plate_h,
        history: None,
        inspector: None,
        secondary,
        primary,
        drawer_tabs: None,
        drawer: None,
        hand,
        banner,
        stage,
        frame,
        top_bar: Some(top_bar),
        opp_strip: Some(opp_strip),
        ticker: Some(ticker),
        chain_rail: Some(chain),
        bottom_left: Some(bottom_left),
        drawer_state: drawer,
    }
}

pub const THUMB_W: f32 = 32.0;
pub const THUMB_H: f32 = 45.0;
pub const BACK_W: f32 = 10.0;
pub const BACK_H: f32 = 14.0;
pub const MAX_BACKS: usize = 10;

pub fn chip(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(13.0)
            .color(INK)
            .background_color(SURFACE_2),
    );
}

pub fn opp_line(seat: &sync::OppSeat) -> String {
    format!(
        "hand {} · deck {} · runes {}/{} · trash {}",
        seat.hand, seat.deck, seat.runes_ready, seat.runes_total, seat.trash
    )
}

pub fn opp_chips(seat: &sync::OppSeat, backs_shown: bool) -> Vec<String> {
    let mut out = Vec::new();
    if !backs_shown || seat.hand > MAX_BACKS {
        out.push(format!("hand {}", seat.hand));
    }
    out.push(format!("deck {}", seat.deck));
    out.push(format!("runes {}/{}", seat.runes_ready, seat.runes_total));
    out.push(format!("trash {}", seat.trash));
    out
}

pub fn opp_strip_ui(
    mut contexts: EguiContexts,
    hud: Res<Hud>,
    strip: Res<sync::OppStrip>,
    mirror: Res<Mirror>,
    menu: Res<crate::menu::Menu>,
    mut art: Art,
) -> Result {
    if !menu.at_table() || mirror.view.zones.is_empty() || !hud.0.class.is_phone() {
        return Ok(());
    }
    let Some(rect) = hud.0.opp_strip else {
        return Ok(());
    };
    if strip.seats.is_empty() || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return Ok(());
    }
    let portrait = hud.0.class == ViewportClass::PhonePortrait;
    let back_texture = crate::net::game_of_zones(&mirror.view.zones)
        .art_game()
        .map(|game| {
            let handle = art.cache.back_image(game, &mut art.images);
            art.registry.texture(&mut contexts, &handle)
        });
    let mut thumbs: Vec<(u8, Option<egui::TextureId>, Option<egui::TextureId>)> = Vec::new();
    if portrait {
        for seat in &strip.seats {
            let legend = seat
                .legend
                .as_deref()
                .and_then(|name| art.texture(&mut contexts, name));
            let champion = seat
                .champion
                .as_deref()
                .and_then(|name| art.texture(&mut contexts, name));
            thumbs.push((seat.seat, legend, champion));
        }
    }
    let context = contexts.ctx_mut()?.clone();
    slot(&context, "opp strip", rect, |ui| {
        ui.set_min_width(rect.width());
        ui.set_max_width(rect.width());
        ui.set_clip_rect(rect);
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        for seat in &strip.seats {
            let color = egui::Color32::from_rgb(seat.color[0], seat.color[1], seat.color[2]);
            ui.spacing_mut().item_spacing = vec2(6.0, 2.0);
            if portrait {
                let (_, legend, champion) = thumbs
                    .iter()
                    .find(|(id, _, _)| *id == seat.seat)
                    .cloned()
                    .unwrap_or((seat.seat, None, None));
                let wide = rect.width() >= OPP_THUMBS_MIN_W;
                let faces: &[Option<egui::TextureId>] =
                    if wide { &[legend, champion] } else { &[] };
                ui.horizontal(|ui| {
                    ui.set_max_width(rect.width());
                    for texture in faces.iter().copied() {
                        let (tile, _) =
                            ui.allocate_exact_size(vec2(THUMB_W, THUMB_H), egui::Sense::hover());
                        match texture {
                            Some(texture) => {
                                ui.painter().image(
                                    texture,
                                    tile,
                                    egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                    egui::Color32::WHITE,
                                );
                            }
                            None => {
                                ui.painter().rect_filled(tile, 3.0, SURFACE_2);
                            }
                        }
                        ui.painter().rect_stroke(
                            tile,
                            3.0,
                            egui::Stroke::new(1.0, color),
                            egui::StrokeKind::Inside,
                        );
                    }
                    let shown = if wide { seat.hand.min(MAX_BACKS) } else { 0 };
                    for _ in 0..shown {
                        let (back, _) =
                            ui.allocate_exact_size(vec2(BACK_W, BACK_H), egui::Sense::hover());
                        ui.painter().rect_filled(back, 2.0, CARD_BACK_INK);
                        if let Some(texture) = back_texture {
                            ui.painter().image(
                                texture,
                                back,
                                egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                egui::Color32::WHITE,
                            );
                        }
                        ui.painter().rect_stroke(
                            back,
                            2.0,
                            egui::Stroke::new(1.0, HAIRLINE),
                            egui::StrokeKind::Inside,
                        );
                    }
                    if !wide {
                        chip(ui, &format!("hand {}", seat.hand));
                    }
                });
                ui.horizontal(|ui| {
                    ui.set_max_width(rect.width());
                    for text in opp_chips(seat, true) {
                        chip(ui, &text);
                    }
                });
            } else {
                ui.horizontal(|ui| {
                    ui.set_max_width(rect.width());
                    let (swatch, _) =
                        ui.allocate_exact_size(vec2(10.0, 10.0), egui::Sense::hover());
                    ui.painter().rect_filled(swatch, 3.0, color);
                    chip(ui, &opp_line(seat));
                });
            }
        }
    });
    Ok(())
}

pub const CARD_BACK_INK: egui::Color32 = egui::Color32::from_rgb(82, 36, 26);

pub fn my_runes(table: &Table, mirror: &Mirror, me: PlayerId) -> Option<(usize, usize)> {
    let pool = mirror
        .view
        .zones
        .iter()
        .find(|decl| decl.name == agni_riftbound::ZONE_NAME_RUNE_POOL)?;
    let ids: Vec<u32> = table
        .in_area(me, Zone::Plugin(pool.id))
        .map(|card| card.id.0)
        .collect();
    let ready = ids.iter().filter(|card| !mirror.rotated(**card)).count();
    Some((ready, ids.len()))
}

pub fn bottom_left_ui(
    mut contexts: EguiContexts,
    hud: Res<Hud>,
    table: Res<GameTable>,
    mirror: Res<Mirror>,
    my_seat: Res<MySeat>,
    menu: Res<crate::menu::Menu>,
) -> Result {
    if !menu.at_table() || !hud.0.class.is_phone() {
        return Ok(());
    }
    let Some(rect) = hud.0.bottom_left else {
        return Ok(());
    };
    let Some((ready, total)) = my_runes(&table, &mirror, my_seat.0) else {
        return Ok(());
    };
    let context = contexts.ctx_mut()?.clone();
    slot(&context, "bottom left", rect, |ui| {
        ui.with_layout(egui::Layout::left_to_right(egui::Align::BOTTOM), |ui| {
            ui.set_min_height(rect.height());
            ui.label(
                egui::RichText::new(format!("runes {ready}/{total}"))
                    .size(15.0)
                    .strong()
                    .color(INK)
                    .background_color(SURFACE),
            );
        });
    });
    Ok(())
}

pub fn drawer_ui(
    mut contexts: EguiContexts,
    hud: Res<Hud>,
    menu: Res<crate::menu::Menu>,
    table: Res<GameTable>,
    mirror: Res<Mirror>,
    my_seat: Res<MySeat>,
    mut drawer: ResMut<layout::HandDrawer>,
    mut scroll: ResMut<layout::DrawerScroll>,
) -> Result {
    if !menu.at_table() || !hud.0.class.is_phone() {
        return Ok(());
    }
    let band = hud.0.hand;
    if band.height() <= 0.0 {
        return Ok(());
    }
    let state = hud.0.drawer_state;
    let hand = my_hand_ids(&table, &mirror, my_seat.0).len();
    let pitch = layout::card_pitch(hud.0.class, state, band.width());
    let visible = layout::hand_visible(hud.0.class, band.width(), pitch);
    let context = contexts.ctx_mut()?.clone();
    let grab = match state {
        DrawerState::Tucked => band,
        DrawerState::Raised => Rect::from_min_max(
            pos2(band.min.x, band.max.y - layout::DRAWER_GRAB_H),
            band.max,
        ),
    };
    let mut event = None;
    let mut swipe_x = 0.0;
    egui::Area::new(egui::Id::new("hand drawer"))
        .fixed_pos(grab.min)
        .order(egui::Order::Middle)
        .show(&context, |ui| {
            ui.set_min_size(grab.size());
            ui.set_max_size(grab.size());
            let painter = ui.painter();
            painter.line_segment(
                [pos2(band.min.x, band.min.y), pos2(band.max.x, band.min.y)],
                egui::Stroke::new(1.0, HAIRLINE),
            );
            let handle_y = match state {
                DrawerState::Tucked => band.min.y + 4.0,
                DrawerState::Raised => band.max.y - layout::DRAWER_GRAB_H / 2.0,
            };
            painter.rect_filled(
                Rect::from_center_size(pos2(band.center().x, handle_y), vec2(40.0, 4.0)),
                2.0,
                INK_WEAK,
            );
            if hand > visible {
                let first = scroll.0.round() as usize + 1;
                let last = (first + visible - 1).min(hand);
                let text = format!("{first}–{last} of {hand}");
                painter.text(
                    pos2(band.max.x - GAP, handle_y),
                    egui::Align2::RIGHT_CENTER,
                    text,
                    egui::FontId::proportional(12.0),
                    INK_WEAK,
                );
            }
            let response = ui.interact(
                grab,
                egui::Id::new("hand drawer grab"),
                egui::Sense::click_and_drag(),
            );
            if response.clicked() {
                event = Some(match state {
                    DrawerState::Tucked => layout::DrawerEvent::TapDrawer,
                    DrawerState::Raised => layout::DrawerEvent::SwipeDown,
                });
            }
            if response.dragged() {
                swipe_x = response.drag_delta().x;
            }
            if response.drag_stopped() {
                let travel = ui.input(|input| {
                    let origin = input.pointer.press_origin();
                    let latest = input.pointer.latest_pos();
                    match (origin, latest) {
                        (Some(origin), Some(latest)) => latest - origin,
                        _ => egui::Vec2::ZERO,
                    }
                });
                event = layout::swipe_event(Vec2::new(travel.x, travel.y), layout::SWIPE_TUCK);
            }
        });
    if swipe_x != 0.0 {
        let next = layout::swipe_scroll(scroll.0, swipe_x, pitch, hand, visible);
        if next != scroll.0 {
            scroll.0 = next;
        }
    }
    let clamped = layout::scroll_clamp(scroll.0, hand, visible);
    if clamped != scroll.0 {
        scroll.0 = clamped;
    }
    if let Some(event) = event {
        let next = layout::drawer_next(drawer.state, event);
        if next != drawer.state {
            drawer.state = next;
        }
    }
    Ok(())
}

pub struct ResponsivePlugin;

impl Plugin for ResponsivePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<layout::HandDrawer>()
            .init_resource::<layout::DrawerScroll>()
            .init_resource::<layout::HandPlane>()
            .init_resource::<sync::OppStrip>()
            .init_resource::<crate::viewport::SafeInsets>()
            .init_resource::<crate::viewport::UiZoom>()
            .init_resource::<crate::viewport::BackKey>()
            .add_observer(layout::on_tap_felt)
            .add_systems(
                PreUpdate,
                (
                    crate::viewport::read_back_key,
                    crate::viewport::platform_insets,
                    crate::viewport::sync_insets,
                )
                    .chain()
                    .after(crate::viewport::classify),
            )
            .add_systems(
                PreUpdate,
                crate::viewport::feed_egui_insets
                    .after(bevy_egui::EguiPreUpdateSet::ProcessInput)
                    .before(bevy_egui::EguiPreUpdateSet::BeginPass),
            )
            .add_systems(
                Update,
                (
                    sync::refresh_opp_strip,
                    layout::auto_drawer,
                    layout::tuck_on_drag_out,
                    scene::touch_camera,
                    crate::net::rejoin_on_resume,
                )
                    .before(refresh_hud),
            )
            .add_systems(
                EguiPrimaryContextPass,
                (
                    crate::viewport::apply_ui_style,
                    crate::os::ime::scroll_focused_above_ime,
                )
                    .before(super::zone_overlay_ui),
            )
            .add_systems(
                EguiPrimaryContextPass,
                (opp_strip_ui, bottom_left_ui, drawer_ui)
                    .chain()
                    .after(primary::primary_ui)
                    .before(menu_button_ui),
            );
    }
}

pub fn stage_min_fraction(class: ViewportClass) -> f32 {
    Metrics::of(class).stage_min_fraction
}

#[derive(Resource, Debug, Clone, PartialEq)]
pub struct Hud(pub HudRects);

impl Default for Hud {
    fn default() -> Self {
        let viewport = Viewport::default();
        Self(layout(
            viewport.class,
            Rect::from_min_size(pos2(0.0, 0.0), vec2(viewport.logical.x, viewport.logical.y)),
            Insets::default(),
            DrawerState::Tucked,
            0,
            SOLO_SEATS,
        ))
    }
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Drawer(pub DrawerState);

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Banner {
    pub collapsed: bool,
}

pub fn strip_fit(idle: bool, phone: bool, collapsed: bool, rows: usize) -> StripFit {
    if idle {
        StripFit::Idle
    } else if phone && collapsed {
        StripFit::Collapsed
    } else if phone {
        StripFit::Rows(rows)
    } else {
        StripFit::Full
    }
}

pub fn strip_rows_needed(
    view: &agni_sim::wire::PluginView,
    me: u8,
    refusal: bool,
    leftover: bool,
) -> usize {
    let state = plugin_ui::strip_state(view, me).map_or(1, |state| state.rows());
    state + usize::from(refusal) + usize::from(leftover)
}

#[allow(clippy::too_many_arguments)]
pub fn refresh_hud(
    viewport: Res<Viewport>,
    insets: Res<Insets>,
    drawer: Res<Drawer>,
    hand_drawer: Res<layout::HandDrawer>,
    banner: Res<Banner>,
    tuning: Res<Tuning>,
    time: Res<Time>,
    refusals: Res<toast::Refusals>,
    panel: Res<plugin_ui::PluginPanel>,
    table: Res<GameTable>,
    mirror: Res<Mirror>,
    players: Res<PlayerCount>,
    info: Res<SessionInfo>,
    my_seat: Res<MySeat>,
    mut hud: ResMut<Hud>,
) {
    let phone = viewport.class.is_phone();
    let state = if phone { hand_drawer.state } else { drawer.0 };
    let idle = strip_idle(&panel.view, my_seat.0 .0, &info, players.0);
    let rows = strip_rows_needed(
        &panel.view,
        my_seat.0 .0,
        refusals.on_strip(time.elapsed_secs_f64()).is_some(),
        !plugin_ui::strip_chips(&panel.view).is_empty(),
    );
    let next = layout_pending(
        viewport.class,
        Rect::from_min_size(pos2(0.0, 0.0), vec2(viewport.logical.x, viewport.logical.y)),
        *insets,
        state,
        chain::len(&panel.view, &table, &mirror),
        players.0,
        Fit {
            strip: strip_fit(idle, phone, banner.collapsed, rows),
            hand_left: phone && tuning.hand_left,
        },
    );
    if hud.0 != next {
        hud.0 = next;
    }
}

pub fn strip_idle(
    view: &agni_sim::wire::PluginView,
    me: u8,
    info: &SessionInfo,
    players: usize,
) -> bool {
    let empty_seat =
        info.role == SessionRole::Host && view.status.is_empty() && info.roster.len() < players;
    plugin_ui::strip_state(view, me).is_none() && !empty_seat
}

pub fn slot<R>(
    context: &egui::Context,
    id: &str,
    rect: Rect,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    egui::Area::new(egui::Id::new(id))
        .fixed_pos(rect.min)
        .order(egui::Order::Middle)
        .show(context, |ui| {
            ui.set_max_size(rect.size());
            body(ui)
        })
}

pub fn panel_frame(style: &egui::Style) -> egui::Frame {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(egui::Stroke::new(1.0, HAIRLINE))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::same(8))
        .shadow(style.visuals.window_shadow)
}

pub const SURFACE: egui::Color32 = egui::Color32::from_rgba_premultiplied(26, 26, 32, 235);
pub const SURFACE_2: egui::Color32 = egui::Color32::from_rgb(42, 42, 51);
pub const HAIRLINE: egui::Color32 = egui::Color32::from_rgba_premultiplied(255, 255, 255, 28);
pub const INK: egui::Color32 = egui::Color32::from_rgb(242, 242, 245);
pub const INK_WEAK: egui::Color32 = egui::Color32::from_rgb(169, 169, 181);
pub const GREEN: egui::Color32 = egui::Color32::from_rgb(56, 160, 92);
pub const AMBER: egui::Color32 = egui::Color32::from_rgb(240, 178, 50);
pub const GREY: egui::Color32 = egui::Color32::from_rgb(70, 70, 76);
pub const DANGER: egui::Color32 = egui::Color32::from_rgb(255, 107, 107);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SheetWidth {
    #[default]
    Standard,
    Wide,
}

impl SheetWidth {
    pub fn points(self, class: ViewportClass) -> f32 {
        match (self, class) {
            (Self::Standard, _) => SHEET_W,
            (Self::Wide, ViewportClass::Desktop) => WIDE_SHEET_W_DESKTOP,
            (Self::Wide, _) => WIDE_SHEET_W_TABLET,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SheetLayout {
    pub width: SheetWidth,
    pub body_scroll: bool,
}

impl Default for SheetLayout {
    fn default() -> Self {
        Self {
            width: SheetWidth::Standard,
            body_scroll: true,
        }
    }
}

pub fn sheet_rect(class: ViewportClass, screen: Rect, side: Side) -> Rect {
    sheet_rect_with(class, screen, side, SheetWidth::Standard)
}

pub fn sheet_rect_with(class: ViewportClass, screen: Rect, side: Side, width: SheetWidth) -> Rect {
    if class.is_phone() {
        return screen;
    }
    let w = width.points(class).min(screen.width());
    match side {
        Side::Left => Rect::from_min_size(screen.min, vec2(w, screen.height())),
        Side::Right => Rect::from_min_size(
            pos2(screen.max.x - w, screen.min.y),
            vec2(w, screen.height()),
        ),
    }
}

pub fn sheet(
    context: &egui::Context,
    id: &str,
    class: ViewportClass,
    side: Side,
    title: &str,
    open: &mut bool,
    body: impl FnOnce(&mut egui::Ui),
) {
    sheet_with(
        context,
        id,
        class,
        side,
        title,
        open,
        SheetLayout::default(),
        body,
    );
}

pub fn sheet_with(
    context: &egui::Context,
    id: &str,
    class: ViewportClass,
    side: Side,
    title: &str,
    open: &mut bool,
    layout: SheetLayout,
    body: impl FnOnce(&mut egui::Ui),
) {
    if !*open {
        return;
    }
    let screen = context.content_rect();
    let rect = sheet_rect_with(class, screen, side, layout.width);
    let layer = egui::LayerId::new(egui::Order::Middle, egui::Id::new(id));
    context.move_to_top(layer);
    let mut scrim_clicked = false;
    let mut close = false;
    egui::Area::new(egui::Id::new(id))
        .fixed_pos(screen.min)
        .order(egui::Order::Middle)
        .show(context, |ui| {
            ui.set_min_size(screen.size());
            ui.set_max_size(screen.size());
            ui.set_clip_rect(screen);
            ui.painter()
                .rect_filled(screen, 0.0, crate::theme::tokens(ui.ctx()).scrim);
            let scrim = ui.interact(screen, egui::Id::new((id, "scrim")), egui::Sense::click());
            if scrim.clicked()
                && scrim
                    .interact_pointer_pos()
                    .is_some_and(|at| !rect.contains(at))
            {
                scrim_clicked = true;
            }
            let mut panel = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            panel.set_min_size(rect.size());
            panel.set_max_size(rect.size());
            let tokens = crate::theme::dress(&mut panel);
            crate::viewport::dress_sheet(&mut panel);
            egui::Frame::new()
                .fill(tokens.surface_opaque())
                .stroke(egui::Stroke::new(1.0, tokens.hairline))
                .corner_radius(if class.is_phone() { 0.0 } else { 12.0 })
                .inner_margin(egui::Margin::same(16))
                .show(&mut panel, |ui| {
                    ui.set_min_size(rect.size() - vec2(32.0, 32.0));
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(title)
                                .size(18.0)
                                .strong()
                                .color(tokens.ink),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let button = egui::Button::new(egui::RichText::new("×").size(20.0))
                                .min_size(vec2(TOUCH_MIN, TOUCH_MIN));
                            if ui.add(button).clicked() {
                                close = true;
                            }
                        });
                    });
                    ui.separator();
                    if layout.body_scroll {
                        egui::ScrollArea::vertical()
                            .id_salt((id, "scroll"))
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                                body(ui);
                            });
                    } else {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                        body(ui);
                    }
                });
        });
    if close || scrim_clicked {
        *open = false;
    }
}

#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TableMenu {
    pub open: bool,
    pub confirm_leave: bool,
    pub confirm_offer: bool,
    pub manual: bool,
}

pub fn free_table_offer(view: &agni_sim::wire::PluginView) -> Option<usize> {
    if let Some(index) = view
        .affordances
        .iter()
        .position(|offer| offer.enabled && offer.label == agni_plugin_sdk::manual::DISABLE)
    {
        return Some(index);
    }
    view.affordances.iter().position(|affordance| {
        affordance.enabled
            && is_free_table_offer(&affordance.label)
            && affordance.label != agni_plugin_sdk::manual::CONTROLS
    })
}

pub const MODE_SWITCH_LABELS: [&str; 2] = ["switch to free table", "switch to rules enforced"];

pub fn is_free_table_offer(label: &str) -> bool {
    matches!(
        label,
        "free table"
            | "confirm free table"
            | "withdraw free table"
            | agni_plugin_sdk::manual::DISABLE
            | agni_plugin_sdk::manual::CONTROLS
    ) || MODE_SWITCH_LABELS.contains(&label)
}

pub fn free_table_label(label: &str) -> &'static str {
    match label {
        agni_plugin_sdk::manual::DISABLE => "disable rules enforcement",
        "confirm free table" => "confirm free table",
        "withdraw free table" => "withdraw the free table proposal",
        "switch to free table" => "switch to free table",
        "switch to rules enforced" => "switch to rules enforced",
        _ => "propose a free table",
    }
}

pub fn offer_needs_confirm(label: &str) -> bool {
    label != "withdraw free table"
}

pub fn leave_note(role: SessionRole) -> &'static str {
    match role {
        SessionRole::Host => "this closes the table",
        SessionRole::Client => "the table stays open for the others",
        _ => "back to the lobby",
    }
}

pub fn between_games(info: &SessionInfo, view: &agni_sim::wire::PluginView) -> bool {
    !info.active() || view.winner.is_some()
}

pub fn menu_button_ui(
    mut contexts: EguiContexts,
    hud: Res<Hud>,
    menu: Res<crate::menu::Menu>,
    settings: Res<crate::settings::Settings>,
    mut table_menu: ResMut<TableMenu>,
) -> Result {
    if !menu.at_table() || settings.open {
        return Ok(());
    }
    let context = contexts.ctx_mut()?.clone();
    let rect = hud.0.menu_button;
    slot(&context, "menu button", rect, |ui| {
        let button = egui::Button::new("").min_size(rect.size());
        let response = coach::touch_tip(ui.add(button), "table menu (esc)");
        let center = response.rect.center();
        for step in [-1.0, 0.0, 1.0] {
            let y = center.y + step * 6.0;
            ui.painter().line_segment(
                [egui::pos2(center.x - 9.0, y), egui::pos2(center.x + 9.0, y)],
                egui::Stroke::new(2.0, INK),
            );
        }
        if response.clicked() {
            table_menu.open = !table_menu.open;
        }
    });
    slot(&context, "report bug button", hud.0.bug_button, |ui| {
        let response = ui
            .add_sized(hud.0.bug_button.size(), egui::Button::new(""))
            .on_hover_text("report a bug");
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "report a bug")
        });
        let center = response.rect.center();
        let stroke = egui::Stroke::new(1.7, INK);
        let painter = ui.painter();
        painter.circle_stroke(center, 7.0, stroke);
        painter.circle_stroke(center - vec2(0.0, 9.0), 3.0, stroke);
        painter.line_segment([center - vec2(0.0, 6.0), center + vec2(0.0, 6.0)], stroke);
        for side in [-1.0, 1.0] {
            for offset in [-5.0, 0.0, 5.0] {
                painter.line_segment(
                    [
                        center + vec2(side * 6.0, offset),
                        center + vec2(side * 12.0, offset * 1.5),
                    ],
                    stroke,
                );
            }
        }
        if response.clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(BUG_REPORT_URL));
        }
    });
    Ok(())
}

pub const BUG_REPORT_URL: &str = "https://github.com/abysl/kai/issues/new";

fn bug_rect(menu: Rect) -> Rect {
    Rect::from_min_size(pos2(menu.max.x + GAP, menu.min.y), menu.size())
}

#[allow(clippy::too_many_arguments)]
pub fn table_menu_ui(
    mut contexts: EguiContexts,
    viewport: Res<Viewport>,
    mut table_menu: ResMut<TableMenu>,
    mut menu: ResMut<crate::menu::Menu>,
    mut settings: ResMut<crate::settings::Settings>,
    panel: Res<plugin_ui::PluginPanel>,
    mut info: ResMut<SessionInfo>,
    mut host: ResMut<net::HostState>,
    mut client: ResMut<net::ClientState>,
    mut help: ResMut<crate::help::HelpSheet>,
    mut sender: Sender,
    mut manual: super::manual::Inputs,
) -> Result {
    if !menu.at_table() || settings.open {
        if table_menu.open {
            table_menu.open = false;
        }
        return Ok(());
    }
    if !table_menu.open {
        table_menu.confirm_leave = false;
        table_menu.confirm_offer = false;
        table_menu.manual = false;
        return Ok(());
    }
    let context = contexts.ctx_mut()?.clone();
    let mut open = true;
    let mut action = None;
    let offer = free_table_offer(&panel.view).map(|index| {
        let label = &panel.view.affordances[index].label;
        (
            index,
            if table_menu.confirm_offer && offer_needs_confirm(label) {
                format!("{} · are you sure?", free_table_label(label))
            } else {
                free_table_label(label).to_string()
            },
            offer_needs_confirm(label),
        )
    });
    let deck_change = between_games(&info, &panel.view);
    let role = info.role;
    let confirm_leave = table_menu.confirm_leave;
    let manual_available = super::manual::available(&panel.view, info.role);
    let manual_open = table_menu.manual && manual_available;
    sheet(
        &context,
        "table menu",
        viewport.class,
        Side::Left,
        "table",
        &mut open,
        |ui| {
            if manual_open {
                if ui.button("‹ table menu").clicked() {
                    action = Some(MenuAction::Back);
                }
                for intent in super::manual::body(
                    ui,
                    &mut manual.panel,
                    &manual.mirror.view,
                    &manual.table.0,
                    &panel.view,
                    manual.me.0 .0,
                    &info.roster,
                ) {
                    manual.requests.write(super::manual::Requested(intent));
                }
                if ui.button("create tokens").clicked() {
                    action = Some(MenuAction::Tokens);
                }
                ui.hyperlink_to("report a bug", BUG_REPORT_URL);
                return;
            }
            let wide = vec2(ui.available_width(), TOUCH_MIN);
            if ui.add_sized(wide, egui::Button::new("resume")).clicked() {
                action = Some(MenuAction::Resume);
            }
            if let Some((index, label, confirm)) = &offer {
                if ui.add_sized(wide, egui::Button::new(label)).clicked() {
                    action = Some(MenuAction::FreeTable(*index, *confirm));
                }
                if panel.view.affordances[*index].label == agni_plugin_sdk::manual::DISABLE {
                    ui.label("Keeps this match and turns off all automatic rules for everyone until the next game.");
                }
            }
            if manual_available
                && ui
                    .add_sized(wide, egui::Button::new("manual controls"))
                    .clicked()
            {
                action = Some(MenuAction::Manual);
            }
            ui.hyperlink_to("report a bug", BUG_REPORT_URL);
            if deck_change
                && ui
                    .add_sized(wide, egui::Button::new("change deck"))
                    .clicked()
            {
                action = Some(MenuAction::ChangeDeck);
            }
            let leave = if confirm_leave {
                "leave · are you sure?"
            } else {
                "leave table"
            };
            if ui.add_sized(wide, egui::Button::new(leave)).clicked() {
                action = Some(MenuAction::Leave);
            }
            ui.label(egui::RichText::new(leave_note(role)).weak());
            ui.add_space(GAP);
            if ui.add_sized(wide, egui::Button::new("settings")).clicked() {
                action = Some(MenuAction::Settings);
            }
            if ui.add_sized(wide, egui::Button::new("help")).clicked() {
                action = Some(MenuAction::Help);
            }
        },
    );
    if !open {
        table_menu.open = false;
    }
    match action {
        Some(MenuAction::Manual) => table_menu.manual = true,
        Some(MenuAction::Back) => table_menu.manual = false,
        Some(MenuAction::Tokens) => {
            manual.tokens.open = true;
            table_menu.open = false;
        }
        Some(MenuAction::Resume) => table_menu.open = false,
        Some(MenuAction::FreeTable(index, confirm)) => {
            if !confirm || table_menu.confirm_offer {
                sender.fire(&panel.view.affordances[index]);
                table_menu.open = false;
            } else {
                table_menu.confirm_offer = true;
            }
        }
        Some(MenuAction::ChangeDeck) => {
            match menu.last_game {
                Some(game) => menu.open_lobby(game),
                None => menu.screen = crate::menu::Screen::Games,
            }
            table_menu.open = false;
        }
        Some(MenuAction::Leave) => {
            if table_menu.confirm_leave {
                net::leave_session(&mut info, &mut host, &mut client);
                menu.screen = crate::menu::Screen::Games;
                table_menu.open = false;
            } else {
                table_menu.confirm_leave = true;
            }
        }
        Some(MenuAction::Settings) => {
            settings.open = true;
            table_menu.open = false;
        }
        Some(MenuAction::Help) => {
            help.open = true;
            table_menu.open = false;
        }
        None => {}
    }
    Ok(())
}

enum MenuAction {
    Manual,
    Back,
    Tokens,
    Resume,
    FreeTable(usize, bool),
    ChangeDeck,
    Leave,
    Settings,
    Help,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZES: [(f32, f32); 4] = [
        (1280.0, 800.0),
        (1024.0, 768.0),
        (800.0, 360.0),
        (360.0, 800.0),
    ];

    #[test]
    fn the_bug_icon_is_a_direct_hud_target_beside_the_menu_at_every_size() {
        assert_eq!(BUG_REPORT_URL, "https://github.com/abysl/kai/issues/new");
        for (w, h) in SIZES {
            let class = crate::viewport::viewport_class(Vec2::new(w, h));
            let rects = layout(
                class,
                screen(w, h),
                Insets::default(),
                DrawerState::Tucked,
                0,
                2,
            );
            assert!(rects.safe.contains_rect(rects.bug_button));
            assert!(rects.bug_button.min.x >= rects.menu_button.max.x);
            assert!(rects.bug_button.max.x < rects.turn_plate.min.x);
            if class.is_phone() {
                assert_eq!(rects.bug_button.size(), vec2(TOUCH_MIN, TOUCH_MIN));
            }
        }
    }

    fn screen(w: f32, h: f32) -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(w, h))
    }

    fn intersects(a: Rect, b: Rect) -> bool {
        a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y
    }

    #[test]
    fn the_reference_sizes_map_to_their_classes() {
        assert_eq!(
            crate::viewport::viewport_class(Vec2::new(1280.0, 800.0)),
            ViewportClass::Desktop
        );
        assert_eq!(
            crate::viewport::viewport_class(Vec2::new(1024.0, 768.0)),
            ViewportClass::Tablet
        );
    }

    #[test]
    fn no_placed_slot_overlaps_another_at_any_size_chain_length_or_drawer_state() {
        for (w, h) in SIZES {
            let class = crate::viewport::viewport_class(Vec2::new(w, h));
            for chain_len in [0usize, 1, 5] {
                for drawer in [DrawerState::Tucked, DrawerState::Raised] {
                    for seats in [2usize, 4] {
                        let rects = layout(
                            class,
                            screen(w, h),
                            Insets::default(),
                            drawer,
                            chain_len,
                            seats,
                        );
                        let placed = rects.placed();
                        for (index, (name, rect)) in placed.iter().enumerate() {
                            assert!(
                                rect.width() >= 0.0 && rect.height() >= 0.0,
                                "{name} at {w}x{h} chain {chain_len} {drawer:?} seats {seats} is inverted: {rect:?}"
                            );
                            for (other_name, other) in placed.iter().skip(index + 1) {
                                assert!(
                                    !intersects(*rect, *other),
                                    "{name} {rect:?} overlaps {other_name} {other:?} at {w}x{h} chain {chain_len} {drawer:?} seats {seats}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn every_slot_stays_inside_the_safe_rect() {
        let insets = Insets {
            top: 24.0,
            bottom: 16.0,
            left: 0.0,
            right: 8.0,
        };
        for (w, h) in SIZES {
            let class = crate::viewport::viewport_class(Vec2::new(w, h));
            for chain_len in [0usize, 1, 5] {
                let rects = layout(
                    class,
                    screen(w, h),
                    insets,
                    DrawerState::Tucked,
                    chain_len,
                    2,
                );
                let safe = rects.safe;
                assert_eq!(
                    safe,
                    Rect::from_min_max(pos2(0.0, 24.0), pos2(w - 8.0, h - 16.0))
                );
                for (name, rect) in rects
                    .placed()
                    .into_iter()
                    .chain([("banner", rects.banner), ("stage", rects.stage)])
                {
                    assert!(
                        safe.contains_rect(rect.shrink(0.01)),
                        "{name} {rect:?} leaves the safe rect {safe:?} at {w}x{h} chain {chain_len}"
                    );
                }
            }
        }
    }

    #[test]
    fn touch_slots_are_at_least_48_on_their_short_side_and_phones_keep_half_the_height_for_the_stage(
    ) {
        for (w, h) in SIZES {
            let class = crate::viewport::viewport_class(Vec2::new(w, h));
            for chain_len in [0usize, 5] {
                let rects = layout(
                    class,
                    screen(w, h),
                    Insets::default(),
                    DrawerState::Tucked,
                    chain_len,
                    2,
                );
                assert!(rects.primary.height() >= TOUCH_MIN);
                if class.is_phone() {
                    for (name, rect) in rects.touch() {
                        let short = rect.width().min(rect.height());
                        assert!(short >= TOUCH_MIN, "{name} is {short} short at {w}x{h}");
                    }
                    assert!(rects.drawer_tabs.is_none());
                    assert!(
                        rects.frame.height() >= rects.safe.height() * stage_min_fraction(class),
                        "the frame is {} of {} at {w}x{h} chain {chain_len}",
                        rects.frame.height(),
                        rects.safe.height()
                    );
                }
            }
        }
    }

    #[test]
    fn the_desktop_row_matches_the_design_table() {
        let rects = layout(
            ViewportClass::Desktop,
            screen(1280.0, 800.0),
            Insets::default(),
            DrawerState::Tucked,
            2,
            2,
        );
        assert_eq!(
            rects.menu_button,
            Rect::from_min_size(pos2(12.0, 12.0), vec2(40.0, 40.0))
        );
        assert_eq!(
            rects.turn_plate,
            Rect::from_min_size(pos2(108.0, 12.0), vec2(200.0, 48.0))
        );
        assert_eq!(
            rects.phase_bar,
            Some(Rect::from_min_size(pos2(108.0, 68.0), vec2(200.0, 20.0)))
        );
        assert_eq!(
            rects.strip,
            Rect::from_min_size(pos2(360.0, 12.0), vec2(560.0, 96.0))
        );
        assert_eq!(
            rects.toast,
            Rect::from_min_size(pos2(360.0, 116.0), vec2(560.0, 32.0))
        );
        assert_eq!(rects.chain.min, pos2(1048.0, 12.0));
        assert_eq!(rects.chain.width(), 220.0);
        assert_eq!(rects.chain.height(), chain_height(2, 2));
        assert_eq!(rects.seats.min.y, rects.chain.max.y + GAP);
        assert_eq!(rects.seats.height(), 2.0 * PLATE_H);
        assert_eq!(rects.primary.size(), vec2(168.0, 56.0));
        assert_eq!(rects.primary.max.x, 1268.0);
        assert_eq!(rects.drawer_tabs.map(|tabs| tabs.max.y), Some(788.0));
        assert_eq!(rects.secondary.max.y, rects.primary.min.y - GAP);
        assert_eq!(
            rects.hand,
            Rect::from_min_max(pos2(192.0, 576.0), pos2(1088.0, 800.0))
        );
        let inspector = rects.inspector.unwrap();
        assert_eq!(inspector.min.x, 12.0);
        assert!(inspector.max.x <= rects.hand.min.x);
        assert_eq!(inspector.max.y, 788.0);
        assert!(inspector.height() <= 0.42 * 800.0);
        let history = rects.history.unwrap();
        assert_eq!(history.min, pos2(12.0, 96.0));
        assert!(history.max.y <= inspector.min.y - GAP);
        assert_eq!(history.height() % HISTORY_TILE_H, 0.0);
        assert!(rects.drawer.is_none());
    }

    #[test]
    fn the_tablet_row_narrows_the_columns_and_the_drawer_pushes_them_left() {
        let tucked = layout(
            ViewportClass::Tablet,
            screen(1024.0, 768.0),
            Insets::default(),
            DrawerState::Tucked,
            5,
            2,
        );
        assert_eq!(tucked.chain.width(), 200.0);
        assert_eq!(tucked.chain_rows, 3);
        assert_eq!(tucked.chain.height(), chain_height(3, 5));
        assert!(tucked.inspector.unwrap().width() <= 0.16 * 1024.0);
        let raised = layout(
            ViewportClass::Tablet,
            screen(1024.0, 768.0),
            Insets::default(),
            DrawerState::Raised,
            5,
            2,
        );
        let drawer = raised.drawer.unwrap();
        assert_eq!(drawer.width(), DRAWER_W);
        assert!(raised.chain.max.x <= drawer.min.x);
        assert!(raised.strip.max.x <= raised.chain.min.x);
        assert!(raised.strip.width() > 0.0);
    }

    #[test]
    fn a_short_column_shows_fewer_full_chain_rows_before_it_squeezes_the_plates() {
        let landscape = layout(
            ViewportClass::PhoneLandscape,
            screen(800.0, 360.0),
            Insets::default(),
            DrawerState::Tucked,
            5,
            2,
        );
        assert!(landscape.chain_rows < 3);
        assert!(landscape.seats.max.y <= landscape.secondary.min.y - GAP);
        assert_eq!(
            chain_height(0, 5),
            CHAIN_HEADER_H + CHAIN_MORE_H + CHAIN_PAD
        );
        assert_eq!(
            chain_height(3, 3),
            CHAIN_HEADER_H + 3.0 * CHAIN_ROW_H + CHAIN_PAD
        );
        assert_eq!(chain_rows_shown(9), 3);
        assert_eq!(chain_rows_shown(1), 1);
    }

    #[test]
    fn the_sheet_fills_a_phone_and_sits_on_one_side_elsewhere() {
        let phone = sheet_rect(
            ViewportClass::PhonePortrait,
            screen(360.0, 800.0),
            Side::Right,
        );
        assert_eq!(phone, screen(360.0, 800.0));
        let right = sheet_rect(ViewportClass::Desktop, screen(1280.0, 800.0), Side::Right);
        assert_eq!(
            right,
            Rect::from_min_max(pos2(900.0, 0.0), pos2(1280.0, 800.0))
        );
        let left = sheet_rect(ViewportClass::Tablet, screen(1024.0, 768.0), Side::Left);
        assert_eq!(left, Rect::from_min_max(pos2(0.0, 0.0), pos2(380.0, 768.0)));
    }

    #[test]
    fn a_wide_sheet_takes_760_on_a_desktop_640_on_a_tablet_and_the_whole_phone() {
        let wide =
            |class, w, h| sheet_rect_with(class, screen(w, h), Side::Right, SheetWidth::Wide);
        assert_eq!(
            wide(ViewportClass::Desktop, 1280.0, 800.0),
            Rect::from_min_max(pos2(520.0, 0.0), pos2(1280.0, 800.0))
        );
        assert_eq!(
            wide(ViewportClass::Tablet, 1024.0, 768.0),
            Rect::from_min_max(pos2(384.0, 0.0), pos2(1024.0, 768.0))
        );
        assert_eq!(
            wide(ViewportClass::Tablet, 600.0, 900.0),
            screen(600.0, 900.0),
            "a narrow tablet caps the wide sheet at the screen"
        );
        assert_eq!(
            wide(ViewportClass::PhonePortrait, 360.0, 640.0),
            screen(360.0, 640.0)
        );
        assert_eq!(
            wide(ViewportClass::PhoneLandscape, 800.0, 480.0),
            screen(800.0, 480.0)
        );
        assert_eq!(SheetLayout::default().width, SheetWidth::Standard);
        assert!(SheetLayout::default().body_scroll);
        assert_eq!(
            sheet_rect_with(
                ViewportClass::Desktop,
                screen(1280.0, 800.0),
                Side::Right,
                SheetWidth::Standard
            ),
            sheet_rect(ViewportClass::Desktop, screen(1280.0, 800.0), Side::Right),
            "every other sheet keeps its 380 pt"
        );
    }

    #[test]
    fn the_free_table_offer_lives_in_the_menu_and_the_leave_note_names_the_consequence() {
        use agni_sim::wire::{Affordance, AffordanceKind, PluginView};
        use serde_bytes::ByteBuf;
        let offer = |label: &str| Affordance {
            label: label.into(),
            hotkey: None,
            enabled: true,
            kind: AffordanceKind::Plain,
            data: ByteBuf::from(vec![13]),
            card: None,
        };
        let proposing = PluginView {
            affordances: vec![offer("end turn"), offer("free table")],
            ..Default::default()
        };
        assert_eq!(free_table_offer(&proposing), Some(1));
        assert_eq!(free_table_label("free table"), "propose a free table");
        let confirming = PluginView {
            affordances: vec![offer("confirm free table")],
            ..Default::default()
        };
        assert_eq!(free_table_offer(&confirming), Some(0));
        assert_eq!(free_table_label("confirm free table"), "confirm free table");
        assert!(is_free_table_offer("withdraw free table"));
        assert_eq!(
            free_table_label("withdraw free table"),
            "withdraw the free table proposal"
        );
        assert_eq!(free_table_offer(&PluginView::default()), None);
        assert_eq!(leave_note(SessionRole::Host), "this closes the table");
        assert_eq!(
            leave_note(SessionRole::Client),
            "the table stays open for the others"
        );
        let mut info = SessionInfo::default();
        assert!(between_games(&info, &PluginView::default()));
        info.role = SessionRole::Host;
        assert!(!between_games(&info, &PluginView::default()));
        let won = PluginView {
            winner: Some(1),
            ..Default::default()
        };
        assert!(between_games(&info, &won));
    }
}

#[cfg(test)]
mod phone_tests {
    use super::*;

    fn screen(w: f32, h: f32) -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(w, h))
    }

    #[test]
    fn the_portrait_row_matches_the_design_table() {
        let rects = layout(
            ViewportClass::PhonePortrait,
            screen(360.0, 800.0),
            Insets::default(),
            DrawerState::Tucked,
            1,
            2,
        );
        assert_eq!(
            rects.top_bar,
            Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(360.0, 56.0)))
        );
        assert_eq!(rects.menu_button.size(), vec2(48.0, 48.0));
        assert_eq!(
            rects.turn_plate,
            Rect::from_min_max(pos2(116.0, 4.0), pos2(352.0, 52.0))
        );
        assert_eq!(
            rects.opp_strip,
            Some(Rect::from_min_max(pos2(8.0, 56.0), pos2(176.0, 120.0)))
        );
        assert!(
            rects.opp_strip.unwrap().width() >= OPP_THUMBS_MIN_W,
            "the strip keeps its thumbnails and two chip rows at 360 wide"
        );
        assert_eq!(
            rects.seats,
            Rect::from_min_max(pos2(184.0, 56.0), pos2(352.0, 120.0))
        );
        assert_eq!(rects.plate_h, 32.0);
        assert_eq!((rects.strip.min.y, rects.strip.max.y), (120.0, 216.0));
        assert_eq!(
            rects.ticker.map(|r| (r.min.y, r.max.y)),
            Some((216.0, 248.0))
        );
        assert_eq!(rects.toast, rects.ticker.unwrap());
        assert_eq!(
            rects.chain_rail.map(|r| (r.min.y, r.max.y)),
            Some((248.0, 304.0))
        );
        assert_eq!(rects.chain, rects.chain_rail.unwrap());
        assert_eq!(rects.chain_rows, 0);
        assert_eq!(
            rects.hand,
            Rect::from_min_max(pos2(0.0, 728.0), pos2(360.0, 800.0))
        );
        assert_eq!(
            rects.stage,
            Rect::from_min_max(pos2(0.0, 312.0), pos2(360.0, 656.0))
        );
        assert_eq!(rects.frame, rects.stage);
        assert!(rects.stage.height() >= 340.0);
        assert_eq!(rects.primary.max, pos2(352.0, 720.0));
        assert_eq!(rects.primary.size(), vec2(152.0, 56.0));
        assert_eq!(rects.secondary.max.y, rects.primary.min.y - GAP);
        let chips = rects.bottom_left.unwrap();
        assert_eq!(chips.max.y, rects.primary.max.y);
        assert_eq!(chips.max.x, rects.primary.min.x - GAP);
        assert_eq!(chips.height(), BOTTOM_LEFT_H);
        assert!(rects.inspector.is_none() && rects.history.is_none() && rects.phase_bar.is_none());
        assert!(rects.drawer.is_none() && rects.drawer_tabs.is_none());
        assert_eq!(rects.drawer_state, DrawerState::Tucked);
    }

    #[test]
    fn the_portrait_drawer_raises_and_the_primary_rides_on_it() {
        let raised = layout(
            ViewportClass::PhonePortrait,
            screen(360.0, 800.0),
            Insets::default(),
            DrawerState::Raised,
            0,
            2,
        );
        assert_eq!(raised.hand.min.y, 612.0);
        assert_eq!(raised.primary.max.y, 604.0);
        assert_eq!(raised.stage.max.y, 540.0);
        assert_eq!(raised.frame.max.y, 656.0);
        assert_eq!(
            drawer_height(ViewportClass::PhonePortrait, DrawerState::Raised),
            188.0
        );
        assert_eq!(
            drawer_height(ViewportClass::PhoneLandscape, DrawerState::Tucked),
            56.0
        );
        assert_eq!(
            drawer_height(ViewportClass::Desktop, DrawerState::Raised),
            0.0
        );
    }

    #[test]
    fn an_idle_strip_collapses_and_the_stage_grows() {
        let idle = layout_pending(
            ViewportClass::PhonePortrait,
            screen(360.0, 800.0),
            Insets::default(),
            DrawerState::Tucked,
            0,
            2,
            Fit::pending(false),
        );
        assert_eq!(idle.strip.height(), 0.0);
        assert_eq!(idle.chain.height(), 0.0, "an empty chain has no rail");
        assert_eq!(idle.stage.min.y, 160.0);
        assert_eq!(
            idle.stage.max.y,
            idle.primary.min.y - GAP,
            "the stage ends above the primary row"
        );
        let with_rail = layout(
            ViewportClass::PhonePortrait,
            screen(360.0, 800.0),
            Insets::default(),
            DrawerState::Tucked,
            1,
            2,
        );
        assert_eq!(with_rail.chain.height(), CHAIN_RAIL_H);
        let collapsed = layout_pending(
            ViewportClass::PhonePortrait,
            screen(360.0, 800.0),
            Insets::default(),
            DrawerState::Tucked,
            0,
            2,
            Fit {
                strip: StripFit::Collapsed,
                hand_left: false,
            },
        );
        assert_eq!(collapsed.strip.height(), BANNER_COLLAPSED_H);
        let pending = layout_pending(
            ViewportClass::PhoneLandscape,
            screen(800.0, 360.0),
            Insets::default(),
            DrawerState::Tucked,
            0,
            2,
            Fit::pending(true),
        );
        assert_eq!(pending.strip.height(), 96.0);
        let idle = layout_pending(
            ViewportClass::PhoneLandscape,
            screen(800.0, 360.0),
            Insets::default(),
            DrawerState::Tucked,
            0,
            2,
            Fit::pending(false),
        );
        assert_eq!(idle.strip.height(), 0.0);
        assert_eq!(idle.stage, pending.stage);
        assert_eq!(strip_fit(true, true, true, 2), StripFit::Idle);
        assert_eq!(strip_fit(false, true, true, 2), StripFit::Collapsed);
        assert_eq!(strip_fit(false, false, true, 2), StripFit::Full);
        assert_eq!(strip_fit(false, true, false, 1), StripFit::Rows(1));
        let one_row = layout_pending(
            ViewportClass::PhonePortrait,
            screen(360.0, 800.0),
            Insets::default(),
            DrawerState::Tucked,
            0,
            2,
            Fit {
                strip: StripFit::Rows(1),
                hand_left: false,
            },
        );
        assert_eq!(
            one_row.strip.height(),
            STRIP_ROW_H,
            "a one-line banner takes one row"
        );
        assert_eq!(
            layout_pending(
                ViewportClass::PhonePortrait,
                screen(360.0, 800.0),
                Insets::default(),
                DrawerState::Tucked,
                0,
                2,
                Fit {
                    strip: StripFit::Rows(9),
                    hand_left: false,
                },
            )
            .strip
            .height(),
            PHONE_BANNER_ROWS as f32 * STRIP_ROW_H
        );
    }

    #[test]
    fn the_frame_is_the_tucked_stage_so_raising_the_drawer_never_moves_the_camera() {
        for (class, w, h) in [
            (ViewportClass::PhonePortrait, 360.0, 800.0),
            (ViewportClass::PhoneLandscape, 800.0, 360.0),
        ] {
            let tucked = layout(
                class,
                screen(w, h),
                Insets::default(),
                DrawerState::Tucked,
                0,
                2,
            );
            let raised = layout(
                class,
                screen(w, h),
                Insets::default(),
                DrawerState::Raised,
                0,
                2,
            );
            assert_eq!(tucked.frame, tucked.stage, "{class:?}");
            assert_eq!(raised.frame, tucked.frame, "{class:?}");
            assert!(raised.stage.max.y < tucked.stage.max.y, "{class:?}");
            assert_eq!(raised.stage.max.y, raised.primary.min.y - GAP, "{class:?}");
            assert!(!raised.stage.intersects(raised.primary), "{class:?}");
            assert!(!raised.stage.intersects(raised.hand), "{class:?}");
        }
    }

    #[test]
    fn hand_on_the_left_mirrors_the_primary_row_on_phones() {
        let right = layout(
            ViewportClass::PhonePortrait,
            screen(360.0, 800.0),
            Insets::default(),
            DrawerState::Tucked,
            0,
            2,
        );
        let left = layout_pending(
            ViewportClass::PhonePortrait,
            screen(360.0, 800.0),
            Insets::default(),
            DrawerState::Tucked,
            0,
            2,
            Fit {
                strip: StripFit::Full,
                hand_left: true,
            },
        );
        assert_eq!(left.primary.min.x, GAP);
        assert_eq!(left.primary.width(), right.primary.width());
        assert_eq!(left.primary.min.y, right.primary.min.y);
        assert_eq!(left.secondary.min.x, GAP);
        assert_eq!(left.bottom_left.unwrap().max.x, 360.0 - GAP);
        assert!(!left.bottom_left.unwrap().intersects(left.primary));
    }

    #[test]
    fn the_landscape_row_matches_the_design_table() {
        let rects = layout(
            ViewportClass::PhoneLandscape,
            screen(800.0, 360.0),
            Insets::default(),
            DrawerState::Tucked,
            1,
            2,
        );
        assert_eq!(
            rects.top_bar,
            Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 56.0)))
        );
        assert_eq!(
            rects.menu_button,
            Rect::from_min_size(pos2(4.0, 4.0), vec2(48.0, 48.0))
        );
        assert_eq!(rects.turn_plate.height(), 48.0);
        assert!(rects.turn_plate.max.y <= 56.0);
        assert_eq!(rects.plate_h, 28.0);
        assert!(rects.seats.max.y <= 56.0);
        let opp = rects.opp_strip.unwrap();
        assert!(opp.min.x >= rects.turn_plate.max.x && opp.max.x <= rects.seats.min.x);
        assert_eq!(
            rects.strip,
            Rect::from_min_size(pos2(176.0, 60.0), vec2(448.0, 96.0))
        );
        assert_eq!(
            rects.chain,
            Rect::from_min_max(pos2(8.0, 60.0), pos2(168.0, 116.0))
        );
        assert!(rects.chain.max.x <= rects.strip.min.x - GAP);
        assert!(rects.chain.max.y <= rects.hand.min.y - GAP);
        assert_eq!(
            rects.hand,
            Rect::from_min_max(pos2(0.0, 304.0), pos2(800.0, 360.0))
        );
        assert_eq!(
            rects.stage,
            Rect::from_min_max(pos2(0.0, 56.0), pos2(800.0, 240.0))
        );
        assert_eq!(
            rects.primary,
            Rect::from_min_max(pos2(672.0, 248.0), pos2(792.0, 296.0))
        );
        assert_eq!(rects.secondary.size(), vec2(96.0, 40.0));
        assert_eq!(rects.secondary.max.x, rects.primary.max.x);
        assert_eq!(
            rects.bottom_left,
            Some(Rect::from_min_max(pos2(8.0, 248.0), pos2(168.0, 296.0)))
        );
        let raised = layout(
            ViewportClass::PhoneLandscape,
            screen(800.0, 360.0),
            Insets::default(),
            DrawerState::Raised,
            5,
            2,
        );
        assert_eq!(raised.hand.min.y, 192.0);
        assert_eq!(raised.ticker.unwrap().height(), 0.0);
        assert!(raised.chain.max.y <= raised.hand.min.y - GAP);
        assert_eq!(raised.primary.max.y, 184.0);
    }

    #[test]
    fn insets_shrink_the_phone_rows_from_every_edge() {
        let insets = Insets {
            top: 32.0,
            bottom: 24.0,
            left: 0.0,
            right: 44.0,
        };
        let rects = layout(
            ViewportClass::PhoneLandscape,
            screen(800.0, 360.0),
            insets,
            DrawerState::Tucked,
            0,
            2,
        );
        assert_eq!(
            rects.safe,
            Rect::from_min_max(pos2(0.0, 32.0), pos2(756.0, 336.0))
        );
        assert_eq!(rects.top_bar.unwrap().min.y, 32.0);
        assert_eq!(rects.hand.max.y, 336.0);
        assert_eq!(rects.primary.max.x, 748.0);
        let portrait = layout(
            ViewportClass::PhonePortrait,
            screen(360.0, 800.0),
            Insets {
                top: 24.0,
                bottom: 48.0,
                left: 0.0,
                right: 0.0,
            },
            DrawerState::Raised,
            0,
            2,
        );
        assert_eq!(
            portrait.hand,
            Rect::from_min_max(pos2(0.0, 564.0), pos2(360.0, 752.0))
        );
        assert_eq!(portrait.menu_button.min.y, 28.0);
        for (name, rect) in portrait.placed() {
            assert!(
                portrait.safe.contains_rect(rect.shrink(0.01)),
                "{name} leaves the safe rect"
            );
        }
    }

    #[test]
    fn four_seats_still_fit_their_phone_band() {
        for (class, w, h) in [
            (ViewportClass::PhonePortrait, 360.0, 800.0),
            (ViewportClass::PhoneLandscape, 800.0, 360.0),
        ] {
            let rects = layout(
                class,
                screen(w, h),
                Insets::default(),
                DrawerState::Tucked,
                2,
                4,
            );
            let band = match class {
                ViewportClass::PhonePortrait => rects.opp_strip.unwrap().max.y,
                _ => rects.top_bar.unwrap().max.y,
            };
            assert!(rects.seats.max.y <= band);
            assert!(rects.plate_h >= PLATE_PHONE_MIN_H);
            assert!(rects.turn_plate.width() >= TOUCH_MIN);
        }
    }

    #[test]
    fn the_strip_is_idle_only_when_nothing_is_pending() {
        use agni_sim::wire::{PluginView, PromptSummary};
        let info = SessionInfo::default();
        assert!(strip_idle(&PluginView::default(), 0, &info, 2));
        let prompt = PluginView {
            prompt: Some(PromptSummary {
                seat: 0,
                why: "choose".into(),
                min: 1,
                max: 1,
                picked: 0,
                optional: false,
            }),
            ..Default::default()
        };
        assert!(!strip_idle(&prompt, 0, &info, 2));
        let hosting = SessionInfo {
            role: SessionRole::Host,
            ..Default::default()
        };
        assert!(!strip_idle(&PluginView::default(), 0, &hosting, 2));
        let seat = sync::OppSeat {
            seat: 1,
            name: "claude".into(),
            color: [200, 60, 60],
            hand: 5,
            deck: 24,
            trash: 2,
            runes_ready: 4,
            runes_total: 6,
            legend: None,
            champion: None,
        };
        assert_eq!(opp_line(&seat), "hand 5 · deck 24 · runes 4/6 · trash 2");
        assert_eq!(
            opp_chips(&seat, true),
            ["deck 24", "runes 4/6", "trash 2"],
            "the backs already count the hand"
        );
        assert_eq!(opp_chips(&seat, false)[0], "hand 5");
    }
}

#[cfg(test)]
mod sheet_tests {
    use super::*;

    #[test]
    fn a_sheet_paints_a_scrim_over_the_whole_screen_under_its_panel() {
        let context = egui::Context::default();
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0));
        let input = egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        };
        let mut open = true;
        let mut output = None;
        for _ in 0..2 {
            context.begin_pass(input.clone());
            sheet(
                &context,
                "table menu",
                ViewportClass::Desktop,
                Side::Left,
                "table",
                &mut open,
                |ui| {
                    ui.label("resume");
                },
            );
            let mut full = context.end_pass();
            full.textures_delta.clear();
            output = Some(full);
        }
        let output = output.unwrap();
        let sheet_layer = egui::LayerId::new(egui::Order::Middle, egui::Id::new("table menu"));
        let rects: Vec<Rect> = output
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Rect(rect) => Some(rect.rect),
                _ => None,
            })
            .collect();
        assert!(
            rects
                .iter()
                .any(|rect| rect.contains_rect(screen.shrink(1.0))),
            "the scrim paints a rect over the whole screen: {rects:?}"
        );
        assert!(
            rects
                .iter()
                .any(|rect| (rect.width() - SHEET_W).abs() <= 2.0 && rect.min.x <= 1.0),
            "the sheet paints a {SHEET_W} pt panel on the left: {rects:?}"
        );
        assert_eq!(
            context.memory(|memory| memory.areas().top_layer_id(egui::Order::Middle)),
            Some(sheet_layer),
            "the sheet is the topmost middle layer"
        );
        assert_eq!(
            context.layer_id_at(pos2(1000.0, 400.0)),
            Some(sheet_layer),
            "the scrim is the sheet's own layer, so it is never sorted under a slot"
        );
        assert_eq!(context.layer_id_at(pos2(100.0, 400.0)), Some(sheet_layer));
        assert!(open);
    }
}
