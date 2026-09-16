use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::MouseMotion;
use bevy::input::touch::Touches;
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{egui, EguiContexts, EguiInput};

pub const PHONE_MAX_WIDTH: f32 = 600.0;
pub const LANDSCAPE_MAX_HEIGHT: f32 = 480.0;
pub const DESKTOP_MIN_WIDTH: f32 = 1100.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewportClass {
    PhonePortrait,
    PhoneLandscape,
    Tablet,
    Desktop,
}

impl ViewportClass {
    pub fn is_phone(self) -> bool {
        matches!(self, Self::PhonePortrait | Self::PhoneLandscape)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::PhonePortrait => "phone portrait",
            Self::PhoneLandscape => "phone landscape",
            Self::Tablet => "tablet",
            Self::Desktop => "desktop",
        }
    }
}

pub fn viewport_class(logical: Vec2) -> ViewportClass {
    let (w, h) = (logical.x, logical.y);
    if w < PHONE_MAX_WIDTH && h >= w {
        ViewportClass::PhonePortrait
    } else if h < LANDSCAPE_MAX_HEIGHT {
        ViewportClass::PhoneLandscape
    } else if w < DESKTOP_MIN_WIDTH {
        ViewportClass::Tablet
    } else {
        ViewportClass::Desktop
    }
}

#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub class: ViewportClass,
    pub logical: Vec2,
    pub scale_factor: f32,
}

impl Viewport {
    pub fn of(logical: Vec2, scale_factor: f32) -> Self {
        Self {
            class: viewport_class(logical),
            logical,
            scale_factor,
        }
    }

    pub fn physical(&self) -> UVec2 {
        (self.logical * self.scale_factor).round().as_uvec2()
    }
}

impl Default for Viewport {
    fn default() -> Self {
        Self::of(Vec2::new(1280.0, 720.0), 1.0)
    }
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    Touch,
    Pointer,
}

impl InputKind {
    pub fn assumed() -> Self {
        if cfg!(target_os = "android") {
            Self::Touch
        } else {
            Self::Pointer
        }
    }

    pub fn is_touch(self) -> bool {
        matches!(self, Self::Touch)
    }
}

pub fn classify(windows: Query<&Window, With<PrimaryWindow>>, mut viewport: ResMut<Viewport>) {
    let Ok(window) = windows.single() else {
        return;
    };
    let fresh = Viewport::of(window.resolution.size(), window.resolution.scale_factor());
    if fresh.logical.x <= 0.0 || fresh.logical.y <= 0.0 {
        return;
    }
    if *viewport != fresh {
        if viewport.class != fresh.class {
            info!(
                "viewport {}x{} → {}",
                fresh.logical.x as u32,
                fresh.logical.y as u32,
                fresh.class.label()
            );
        }
        *viewport = fresh;
    }
}

pub fn track_input(
    touches: Res<Touches>,
    mut motion: MessageReader<MouseMotion>,
    mut kind: ResMut<InputKind>,
) {
    if touches.any_just_pressed() {
        kind.set_if_neq(InputKind::Touch);
        motion.clear();
        return;
    }
    if motion.read().next().is_some() {
        kind.set_if_neq(InputKind::Pointer);
    }
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct SafeInsets {
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
    pub ime: f32,
}

impl SafeInsets {
    pub fn from_array(values: [f32; 5]) -> Self {
        let clean = |value: f32| {
            if value.is_finite() {
                value.max(0.0)
            } else {
                0.0
            }
        };
        Self {
            top: clean(values[0]),
            right: clean(values[1]),
            bottom: clean(values[2]),
            left: clean(values[3]),
            ime: clean(values[4]),
        }
    }

    pub fn layout(&self) -> crate::table::hud::Insets {
        crate::table::hud::Insets {
            top: self.top,
            bottom: self.bottom.max(self.ime),
            left: self.left,
            right: self.right,
        }
    }

    pub fn egui(&self) -> egui::SafeAreaInsets {
        egui::SafeAreaInsets(egui::epaint::MarginF32 {
            left: self.left,
            right: self.right,
            top: self.top,
            bottom: self.bottom.max(self.ime),
        })
    }
}

pub fn sync_insets(safe: Res<SafeInsets>, mut insets: ResMut<crate::table::hud::Insets>) {
    let wanted = safe.layout();
    if *insets != wanted {
        *insets = wanted;
    }
}

pub fn feed_egui_insets(safe: Res<SafeInsets>, mut inputs: Query<&mut EguiInput>) {
    let wanted = safe.egui();
    for mut input in &mut inputs {
        if input.0.safe_area_insets != Some(wanted) {
            input.0.safe_area_insets = Some(wanted);
        }
    }
}

pub fn platform_insets(mut safe: ResMut<SafeInsets>) {
    if let Some(fresh) = platform::insets() {
        if *safe != fresh {
            *safe = fresh;
        }
    }
}

#[cfg(target_os = "android")]
mod platform {
    pub fn insets() -> Option<super::SafeInsets> {
        crate::os::android::take_insets().map(super::SafeInsets::from_array)
    }
}

#[cfg(target_arch = "wasm32")]
mod platform {
    pub fn insets() -> Option<super::SafeInsets> {
        let window = web_sys::window()?;
        let raw = js_sys::Reflect::get(&window, &"__kaiInsets".into()).ok()?;
        let array = raw.dyn_ref::<js_sys::Array>()?;
        let mut values = [0.0f32; 5];
        for (index, slot) in values.iter_mut().enumerate() {
            *slot = array.get(index as u32).as_f64().unwrap_or(0.0) as f32;
        }
        Some(super::SafeInsets::from_array(values))
    }

    use wasm_bindgen::JsCast;
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
mod platform {
    pub fn insets() -> Option<super::SafeInsets> {
        None
    }
}

pub const ZOOM_PHONE_PORTRAIT: f32 = 1.15;
pub const ZOOM_PHONE_LANDSCAPE: f32 = 1.10;
pub const UI_SCALE_MIN: f32 = 0.8;
pub const UI_SCALE_MAX: f32 = 1.5;

pub fn class_zoom(class: ViewportClass) -> f32 {
    match class {
        ViewportClass::PhonePortrait => ZOOM_PHONE_PORTRAIT,
        ViewportClass::PhoneLandscape => ZOOM_PHONE_LANDSCAPE,
        ViewportClass::Tablet | ViewportClass::Desktop => 1.0,
    }
}

#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct UiZoom {
    pub ui_scale: f32,
}

impl Default for UiZoom {
    fn default() -> Self {
        Self { ui_scale: 1.0 }
    }
}

impl UiZoom {
    pub fn factor(&self, class: ViewportClass) -> f32 {
        class_zoom(class) * self.ui_scale.clamp(UI_SCALE_MIN, UI_SCALE_MAX)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TouchStyle {
    pub interact_h: f32,
    pub button_padding: Vec2,
    pub item_spacing: Vec2,
    pub body_pt: f32,
}

pub fn touch_style(class: ViewportClass, kind: InputKind, at_table: bool) -> TouchStyle {
    let touch = class.is_phone() || kind.is_touch();
    let base = match (class, touch) {
        (_, true) => TouchStyle {
            interact_h: 48.0,
            button_padding: Vec2::new(16.0, 12.0),
            item_spacing: Vec2::new(8.0, 8.0),
            body_pt: 16.0,
        },
        (ViewportClass::Tablet, false) => TouchStyle {
            interact_h: 40.0,
            button_padding: Vec2::new(12.0, 8.0),
            item_spacing: Vec2::new(8.0, 6.0),
            body_pt: 15.0,
        },
        (_, false) => TouchStyle {
            interact_h: 24.0,
            button_padding: Vec2::new(8.0, 4.0),
            item_spacing: Vec2::new(6.0, 4.0),
            body_pt: 14.0,
        },
    };
    if at_table {
        TouchStyle {
            interact_h: 24.0,
            item_spacing: Vec2::new(6.0, 4.0),
            ..base
        }
    } else {
        base
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SheetStyle {
    pub style: TouchStyle,
    pub scale: f32,
}

fn sheet_style_id() -> egui::Id {
    egui::Id::new("sheet touch style")
}

pub fn write_style(egui_style: &mut egui::Style, style: TouchStyle, scale: f32) {
    egui_style.spacing.interact_size.y = style.interact_h * scale;
    egui_style.spacing.button_padding = egui::vec2(
        style.button_padding.x * scale,
        style.button_padding.y * scale,
    );
    egui_style.spacing.item_spacing =
        egui::vec2(style.item_spacing.x * scale, style.item_spacing.y * scale);
    let body = style.body_pt * scale;
    for (text_style, font) in egui_style.text_styles.iter_mut() {
        let size = match text_style {
            egui::TextStyle::Small => body * 0.75,
            egui::TextStyle::Body | egui::TextStyle::Monospace => body,
            egui::TextStyle::Button => body,
            egui::TextStyle::Heading => body * 1.4,
            egui::TextStyle::Name(_) => font.size,
        };
        font.size = size;
    }
}

pub fn dress_sheet(ui: &mut egui::Ui) {
    let stored: Option<SheetStyle> = ui.ctx().data(|data| data.get_temp(sheet_style_id()));
    if let Some(SheetStyle { style, scale }) = stored {
        write_style(ui.style_mut(), style, scale);
    }
}

pub fn apply_ui_style(
    viewport: Res<Viewport>,
    kind: Res<InputKind>,
    zoom: Res<UiZoom>,
    menu: Res<crate::menu::Menu>,
    mut contexts: EguiContexts,
    mut applied: Local<Option<(ViewportClass, InputKind, u32, bool)>>,
) {
    let scale = zoom.ui_scale.clamp(UI_SCALE_MIN, UI_SCALE_MAX);
    let at_table = menu.at_table();
    let key = (viewport.class, *kind, scale.to_bits(), at_table);
    if *applied == Some(key) {
        return;
    }
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    let style = touch_style(viewport.class, *kind, at_table);
    context.all_styles_mut(|egui_style| write_style(egui_style, style, scale));
    let sheet = SheetStyle {
        style: touch_style(viewport.class, *kind, false),
        scale,
    };
    context.data_mut(|data| data.insert_temp(sheet_style_id(), sheet));
    *applied = Some(key);
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BackKey {
    pub pressed: bool,
}

pub fn is_back_key(key: &Key) -> bool {
    matches!(key, Key::BrowserBack | Key::GoBack)
}

pub fn read_back_key(mut keys: MessageReader<KeyboardInput>, mut back: ResMut<BackKey>) {
    let pressed = keys.read().any(|input| {
        input.state == ButtonState::Pressed && !input.repeat && is_back_key(&input.logical_key)
    });
    if back.pressed != pressed {
        back.pressed = pressed;
    }
}

pub fn back_pressed(keys: &ButtonInput<KeyCode>, back: &BackKey) -> bool {
    keys.just_pressed(KeyCode::Escape) || keys.just_pressed(KeyCode::BrowserBack) || back.pressed
}

pub fn consume_back(keys: &mut ButtonInput<KeyCode>, back: &mut BackKey) {
    keys.clear_just_pressed(KeyCode::Escape);
    keys.clear_just_pressed(KeyCode::BrowserBack);
    back.pressed = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(w: f32, h: f32) -> ViewportClass {
        viewport_class(Vec2::new(w, h))
    }

    #[test]
    fn the_four_reference_sizes_land_in_their_classes() {
        assert_eq!(class(1280.0, 800.0), ViewportClass::Desktop);
        assert_eq!(class(1920.0, 1080.0), ViewportClass::Desktop);
        assert_eq!(class(1024.0, 768.0), ViewportClass::Tablet);
        assert_eq!(class(800.0, 360.0), ViewportClass::PhoneLandscape);
        assert_eq!(class(360.0, 800.0), ViewportClass::PhonePortrait);
    }

    #[test]
    fn the_boundaries_fall_on_the_documented_side() {
        assert_eq!(class(599.0, 800.0), ViewportClass::PhonePortrait);
        assert_eq!(class(600.0, 800.0), ViewportClass::Tablet);
        assert_eq!(class(1100.0, 480.0), ViewportClass::Desktop);
        assert_eq!(class(1099.0, 480.0), ViewportClass::Tablet);
        assert_eq!(class(1024.0, 479.0), ViewportClass::PhoneLandscape);
        assert_eq!(class(1280.0, 479.0), ViewportClass::PhoneLandscape);
        assert_eq!(class(599.0, 599.0), ViewportClass::PhonePortrait);
        assert_eq!(class(599.0, 598.0), ViewportClass::Tablet);
    }

    #[test]
    fn a_narrow_short_window_is_landscape_not_portrait() {
        assert_eq!(class(400.0, 300.0), ViewportClass::PhoneLandscape);
    }

    #[test]
    fn the_class_picks_the_layout_and_the_input_kind_is_a_separate_axis() {
        let viewport = Viewport::of(Vec2::new(1024.0, 768.0), 2.0);
        assert_eq!(viewport.class, ViewportClass::Tablet);
        assert_eq!(viewport.physical(), UVec2::new(2048, 1536));
        assert!(!viewport.class.is_phone());
        assert!(ViewportClass::PhonePortrait.is_phone());
        assert_eq!(InputKind::assumed(), InputKind::Pointer);
        assert!(InputKind::Touch.is_touch());
    }

    #[test]
    fn safe_insets_reject_negatives_and_the_ime_widens_the_bottom() {
        let insets = SafeInsets::from_array([24.0, -3.0, 16.0, f32::NAN, 300.0]);
        assert_eq!(insets.top, 24.0);
        assert_eq!(insets.right, 0.0);
        assert_eq!(insets.left, 0.0);
        assert_eq!(insets.bottom, 16.0);
        assert_eq!(insets.ime, 300.0);
        let layout = insets.layout();
        assert_eq!(layout.bottom, 300.0);
        assert_eq!(layout.top, 24.0);
        assert_eq!(insets.egui().0.bottom, 300.0);
        let plain = SafeInsets::from_array([0.0, 0.0, 48.0, 0.0, 0.0]);
        assert_eq!(plain.layout().bottom, 48.0);
        assert_eq!(
            SafeInsets::default().layout(),
            crate::table::hud::Insets::default()
        );
    }

    #[test]
    fn the_zoom_rule_scales_by_class_and_the_ui_scale_slider() {
        assert_eq!(class_zoom(ViewportClass::PhonePortrait), 1.15);
        assert_eq!(class_zoom(ViewportClass::PhoneLandscape), 1.10);
        assert_eq!(class_zoom(ViewportClass::Tablet), 1.0);
        assert_eq!(class_zoom(ViewportClass::Desktop), 1.0);
        let zoom = UiZoom { ui_scale: 1.2 };
        assert!((zoom.factor(ViewportClass::PhonePortrait) - 1.38).abs() < 1e-5);
        let clamped = UiZoom { ui_scale: 9.0 };
        assert_eq!(clamped.factor(ViewportClass::Desktop), UI_SCALE_MAX);
        assert_eq!(UiZoom::default().factor(ViewportClass::Desktop), 1.0);
    }

    #[test]
    fn touch_style_follows_the_phone_tablet_desktop_table() {
        let phone = touch_style(ViewportClass::PhonePortrait, InputKind::Pointer, false);
        assert_eq!(phone.interact_h, 48.0);
        assert_eq!(phone.body_pt, 16.0);
        let touched_desktop = touch_style(ViewportClass::Desktop, InputKind::Touch, false);
        assert_eq!(touched_desktop, phone);
        let tablet = touch_style(ViewportClass::Tablet, InputKind::Pointer, false);
        assert_eq!(tablet.interact_h, 40.0);
        assert_eq!(tablet.body_pt, 15.0);
        let desktop = touch_style(ViewportClass::Desktop, InputKind::Pointer, false);
        assert_eq!(desktop.interact_h, 24.0);
        assert_eq!(desktop.button_padding, Vec2::new(8.0, 4.0));
        assert_eq!(desktop.body_pt, 14.0);
        let at_table = touch_style(ViewportClass::PhonePortrait, InputKind::Touch, true);
        assert_eq!(at_table.interact_h, 24.0);
        assert_eq!(at_table.item_spacing, Vec2::new(6.0, 4.0));
        assert_eq!(at_table.button_padding, phone.button_padding);
        assert_eq!(at_table.body_pt, 16.0);
    }

    #[test]
    fn the_logical_back_key_is_browser_back_or_go_back_and_escape_stays_an_alias() {
        assert!(is_back_key(&Key::BrowserBack));
        assert!(is_back_key(&Key::GoBack));
        assert!(!is_back_key(&Key::Escape));
        assert!(!is_back_key(&Key::Character("b".into())));
        let mut keys = ButtonInput::<KeyCode>::default();
        assert!(back_pressed(&keys, &BackKey { pressed: true }));
        assert!(!back_pressed(&keys, &BackKey::default()));
        let mut back = BackKey { pressed: true };
        keys.press(KeyCode::Escape);
        assert!(back_pressed(&keys, &back));
        consume_back(&mut keys, &mut back);
        assert!(!back_pressed(&keys, &back));
        assert!(keys.pressed(KeyCode::Escape));
    }
}
