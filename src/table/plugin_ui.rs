use super::*;
use agni_plugin_sdk::dice;
use agni_sim::view::TableView;
use agni_sim::wire::{Affordance, AffordanceKind, PluginView, PromptSummary};
use std::collections::BTreeMap;

#[derive(Message, Debug, Clone)]
pub struct PluginActionRequested(pub Vec<u8>, pub Option<u32>);

pub fn enforced(view: &PluginView) -> bool {
    plate::mode_of(view) == Some(plate::Mode::Enforced)
}

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tools {
    pub free: bool,
}

impl Default for Tools {
    fn default() -> Self {
        Self { free: true }
    }
}

impl From<&PluginView> for Tools {
    fn from(view: &PluginView) -> Self {
        Self {
            free: !enforced(view),
        }
    }
}

pub fn refresh_tools(panel: Res<PluginPanel>, mut tools: ResMut<Tools>) {
    let next = Tools::from(&panel.view);
    if *tools != next {
        *tools = next;
    }
}

#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct PluginPanel {
    pub view: PluginView,
}

#[derive(Resource, Default, Debug)]
pub struct RollSecrets {
    secrets: BTreeMap<u32, [u8; dice::SECRET_LEN]>,
    revealed: BTreeMap<u32, bool>,
}

impl RollSecrets {
    pub fn commit(&mut self, roll: u32, secret: [u8; dice::SECRET_LEN]) -> [u8; dice::SECRET_LEN] {
        if let (Some(held), Some(false)) = (self.secrets.get(&roll), self.revealed.get(&roll)) {
            return dice::commitment(held);
        }
        self.secrets.insert(roll, secret);
        self.revealed.insert(roll, false);
        dice::commitment(&secret)
    }

    pub fn take_reveal(&mut self, roll: u32) -> Option<[u8; dice::SECRET_LEN]> {
        let secret = *self.secrets.get(&roll)?;
        match self.revealed.insert(roll, true) {
            Some(false) => Some(secret),
            _ => None,
        }
    }

    pub fn forget(&mut self) {
        self.secrets.clear();
        self.revealed.clear();
    }

    pub fn rearm(&mut self) {
        for revealed in self.revealed.values_mut() {
            *revealed = false;
        }
    }
}

pub fn action_bytes(
    affordance: &Affordance,
    secrets: &mut RollSecrets,
    fresh: &dyn Fn() -> [u8; dice::SECRET_LEN],
) -> Option<Vec<u8>> {
    let mut bytes = affordance.data.to_vec();
    match affordance.kind {
        AffordanceKind::Plain => {}
        AffordanceKind::Commit { roll } => bytes.extend(secrets.commit(roll, fresh())),
        AffordanceKind::Reveal { roll } => bytes.extend(secrets.take_reveal(roll)?),
    }
    Some(bytes)
}

pub fn auto_reveal(
    panel: Res<PluginPanel>,
    mut secrets: ResMut<RollSecrets>,
    mut requests: MessageWriter<PluginActionRequested>,
) {
    if !panel.is_changed() {
        return;
    }
    for affordance in &panel.view.affordances {
        if !matches!(affordance.kind, AffordanceKind::Reveal { .. }) || !affordance.enabled {
            continue;
        }
        if let Some(bytes) = action_bytes(affordance, &mut secrets, &crate::os::entropy::secret) {
            requests.write(PluginActionRequested(bytes, None));
        }
    }
}

pub const FACE_DOWN: &str = "a face-down card";
pub const GREYED_HINT: &str = "cannot be played right now";
pub const PASS_KEY: &str = "w";
pub const ADVANCE_KEY: &str = "space";
pub const ESCAPE_KEY: &str = "x";
const RIM_OUTSET: f32 = 0.05;

#[allow(clippy::too_many_arguments)]
pub fn refresh_plugin_view(
    mirror: Res<Mirror>,
    info: Res<SessionInfo>,
    my_seat: Res<MySeat>,
    table: Res<crate::table::GameTable>,
    seated: Res<crate::deck::import::SeatedDeck>,
    mut host: ResMut<net::HostState>,
    mut client: ResMut<net::ClientState>,
    mut panel: ResMut<PluginPanel>,
    mut activity: ResMut<super::primary::TurnActivity>,
) {
    if !mirror.is_changed() && !info.is_changed() && !my_seat.is_changed() && !seated.is_changed() {
        return;
    }
    let mut view = match info.role {
        SessionRole::Host => host.plugin_view(my_seat.0 .0),
        SessionRole::Client => client.plugin_view(my_seat.0 .0),
        _ => None,
    }
    .unwrap_or_default();
    let pending = seated.0.as_ref().is_some_and(|record| {
        crate::deck::battlefield::placement_pending(record, &table.0, &mirror.view.zones, my_seat.0)
    });
    if pending {
        gate_roll(&mut view);
    }
    if panel.view != view {
        activity.observe_turn(view.turn.as_ref());
        panel.view = view;
    }
}

pub fn gate_roll(view: &mut PluginView) {
    for affordance in &mut view.affordances {
        if matches!(affordance.kind, AffordanceKind::Commit { .. }) && affordance.enabled {
            affordance.enabled = false;
            affordance.label = format!(
                "{} · {}",
                affordance.label,
                crate::deck::battlefield::ROLL_GATE
            );
        }
    }
}

pub fn card_label(table: &Table, view: &TableView, me: PlayerId, card: u32) -> String {
    match table.get(CardId(card)) {
        None => format!("card {card}"),
        Some(card) if card.face.is_hidden() => FACE_DOWN.to_string(),
        Some(card) if face_down_in(card, view) && !face_down_for(card, view, me) => {
            FACE_DOWN.to_string()
        }
        Some(card) => card.face.name.clone(),
    }
}

pub fn expand(
    text: &str,
    seat_name: &dyn Fn(u8) -> String,
    zone_name: &dyn Fn(u16) -> String,
    card_name: &dyn Fn(u32) -> String,
) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let token = &after[..end];
        let replaced = match token.split_once(' ') {
            Some(("seat", index)) => index.parse().ok().map(seat_name),
            Some(("zone", index)) => index.parse().ok().map(zone_name),
            Some(("card", index)) => index.parse().ok().map(card_name),
            _ => None,
        };
        match replaced {
            Some(name) => out.push_str(&name),
            None => {
                out.push('{');
                out.push_str(token);
                out.push('}');
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

pub fn prompt_line(summary: &PromptSummary, me: u8) -> String {
    let range = match (summary.min, summary.max) {
        (_, 0 | 1) => "pick one".to_string(),
        (min, max) if min == max => format!("pick {min}"),
        (0, max) => format!("up to {max}"),
        (min, max) => format!("{min} to {max}"),
    };
    let mut line = if summary.seat == me {
        summary.why.clone()
    } else {
        format!("{{seat {}}}: {}", summary.seat, summary.why)
    };
    line.push_str(" · ");
    line.push_str(&range);
    if summary.max > 1 {
        line.push_str(&format!(" · {} picked", summary.picked));
    }
    if summary.optional {
        line.push_str(" · optional");
    }
    line
}

pub fn group(affordance: &Affordance, prompt_open: bool) -> u8 {
    match affordance.hotkey.as_deref() {
        Some(PASS_KEY | ADVANCE_KEY) => 1,
        _ if prompt_open => 0,
        _ => 2,
    }
}

pub fn ordered(view: &PluginView) -> Vec<&Affordance> {
    let open = view.prompt.is_some();
    let mut shown: Vec<&Affordance> = view
        .shown()
        .map(|(_, affordance)| affordance)
        .filter(|affordance| !matches!(affordance.kind, AffordanceKind::Reveal { .. }))
        .collect();
    shown.sort_by_key(|affordance| group(affordance, open));
    shown
}

pub fn told_primary(view: &PluginView) -> Option<usize> {
    view.primary
        .map(usize::from)
        .filter(|index| !view.is_hidden(*index))
        .filter(|index| {
            view.affordances
                .get(*index)
                .is_some_and(|held| held.enabled)
        })
}

pub fn menu_only(view: &PluginView, index: usize) -> bool {
    view.is_hidden(index)
        || view
            .affordances
            .get(index)
            .is_some_and(|affordance| super::hud::is_free_table_offer(&affordance.label))
}

pub fn card_affordances(view: &PluginView, card: u32) -> Vec<&Affordance> {
    view.shown()
        .map(|(_, affordance)| affordance)
        .filter(|affordance| affordance.enabled && affordance.card == Some(card))
        .collect()
}

pub fn card_affordance(view: &PluginView, card: u32) -> Option<&Affordance> {
    match card_affordances(view, card).as_slice() {
        [only] => Some(only),
        _ => None,
    }
}

pub fn highlighted(view: &PluginView) -> BTreeSet<u32> {
    view.shown()
        .map(|(_, affordance)| affordance)
        .filter(|affordance| affordance.enabled)
        .filter_map(|affordance| affordance.card)
        .collect()
}

pub fn rim_corners(width: f32, height: f32) -> [Vec3; 4] {
    let x = width / 2.0 + RIM_OUTSET;
    let z = height / 2.0 + RIM_OUTSET;
    let y = dim::CARD_THICK / 2.0;
    [
        Vec3::new(-x, y, -z),
        Vec3::new(x, y, -z),
        Vec3::new(x, y, z),
        Vec3::new(-x, y, z),
    ]
}

pub const KAI_KEYS: [KeyCode; 15] = [
    KeyCode::KeyI,
    KeyCode::KeyH,
    KeyCode::KeyF,
    KeyCode::KeyP,
    KeyCode::KeyR,
    KeyCode::KeyL,
    KeyCode::KeyC,
    KeyCode::KeyK,
    KeyCode::KeyD,
    KeyCode::KeyT,
    KeyCode::KeyE,
    KeyCode::Tab,
    KeyCode::Home,
    KeyCode::Escape,
    KeyCode::Enter,
];

pub fn key_of(hotkey: &str) -> Option<KeyCode> {
    let key = match hotkey {
        "space" => KeyCode::Space,
        "enter" => KeyCode::Enter,
        "1" => KeyCode::Digit1,
        "2" => KeyCode::Digit2,
        "3" => KeyCode::Digit3,
        "4" => KeyCode::Digit4,
        "5" => KeyCode::Digit5,
        "6" => KeyCode::Digit6,
        "7" => KeyCode::Digit7,
        "8" => KeyCode::Digit8,
        "9" => KeyCode::Digit9,
        "a" => KeyCode::KeyA,
        "b" => KeyCode::KeyB,
        "f" => KeyCode::KeyF,
        "g" => KeyCode::KeyG,
        "j" => KeyCode::KeyJ,
        "m" => KeyCode::KeyM,
        "n" => KeyCode::KeyN,
        "o" => KeyCode::KeyO,
        "q" => KeyCode::KeyQ,
        "s" => KeyCode::KeyS,
        "u" => KeyCode::KeyU,
        "v" => KeyCode::KeyV,
        "w" => KeyCode::KeyW,
        "x" => KeyCode::KeyX,
        "y" => KeyCode::KeyY,
        "z" => KeyCode::KeyZ,
        _ => return None,
    };
    (!KAI_KEYS.contains(&key)).then_some(key)
}

#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct ClaimedKeys(pub BTreeSet<KeyCode>);

impl ClaimedKeys {
    pub fn taken(&self, key: KeyCode) -> bool {
        self.0.contains(&key)
    }

    pub fn claim(&mut self, key: KeyCode) {
        self.0.insert(key);
    }
}

pub fn claims(view: &PluginView, pressed: &dyn Fn(KeyCode) -> bool) -> ClaimedKeys {
    let mut out = ClaimedKeys::default();
    if let Some(key) = pressed_index(view, pressed).and_then(|index| effective_hotkey(view, index))
    {
        out.claim(key);
    }
    out
}

pub fn kai_key(claimed: &ClaimedKeys, key: KeyCode) -> bool {
    !claimed.taken(key)
}

pub fn pressed_index(view: &PluginView, pressed: &dyn Fn(KeyCode) -> bool) -> Option<usize> {
    view.affordances
        .iter()
        .enumerate()
        .position(|(index, affordance)| {
            affordance.enabled && effective_hotkey(view, index).is_some_and(pressed)
        })
}

pub fn effective_hotkey(view: &PluginView, index: usize) -> Option<KeyCode> {
    let affordance = view.affordances.get(index)?;
    if view.prompt.is_some() && affordance.card.is_none() {
        if affordance.label.eq_ignore_ascii_case("yes") {
            return Some(KeyCode::Digit1);
        }
        if affordance.label.eq_ignore_ascii_case("no") {
            return Some(KeyCode::Digit2);
        }
    }
    affordance.hotkey.as_deref().and_then(key_of)
}

pub fn effective_digit(view: &PluginView, index: usize) -> Option<usize> {
    match effective_hotkey(view, index) {
        Some(KeyCode::Digit1) => Some(1),
        Some(KeyCode::Digit2) => Some(2),
        Some(KeyCode::Digit3) => Some(3),
        Some(KeyCode::Digit4) => Some(4),
        Some(KeyCode::Digit5) => Some(5),
        Some(KeyCode::Digit6) => Some(6),
        Some(KeyCode::Digit7) => Some(7),
        Some(KeyCode::Digit8) => Some(8),
        Some(KeyCode::Digit9) => Some(9),
        _ => None,
    }
}

pub fn plugin_takes(view: &PluginView, pressed: &dyn Fn(KeyCode) -> bool, shift: bool) -> bool {
    let Some(index) = pressed_index(view, pressed) else {
        return false;
    };
    let key = effective_hotkey(view, index);
    !(shift && matches!(key, Some(KeyCode::Space | KeyCode::KeyW)))
}

pub fn pressed_affordance<'a>(
    view: &'a PluginView,
    pressed: &dyn Fn(KeyCode) -> bool,
) -> Option<&'a Affordance> {
    pressed_index(view, pressed).map(|index| &view.affordances[index])
}

pub fn plugin_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    mut contexts: EguiContexts,
    panel: Res<PluginPanel>,
    mut secrets: ResMut<RollSecrets>,
    mut requests: MessageWriter<PluginActionRequested>,
    menu: Res<crate::menu::Menu>,
    tuning: Res<Tuning>,
    mut claimed: ResMut<ClaimedKeys>,
    mut activity: ResMut<primary::TurnActivity>,
) {
    if !claimed.0.is_empty() {
        claimed.0.clear();
    }
    if !menu.at_table()
        || panel.view.affordances.is_empty()
        || keys.get_just_pressed().next().is_none()
        || ((keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight))
            && keys.just_pressed(KeyCode::KeyZ))
    {
        return;
    }
    if let Ok(context) = contexts.ctx_mut() {
        if context.egui_wants_keyboard_input() {
            return;
        }
    }
    let pressed = |key: KeyCode| keys.just_pressed(key);
    if !plugin_takes(&panel.view, &pressed, auto::shift_held(&keys)) {
        return;
    }
    if let Some(index) = pressed_index(&panel.view, &pressed) {
        if primary::confirm_guards(
            &panel.view,
            index,
            tuning.confirm_end_turn,
            !activity.acted(),
        ) {
            return;
        }
        let affordance = &panel.view.affordances[index];
        if let Some(bytes) = action_bytes(affordance, &mut secrets, &crate::os::entropy::secret) {
            activity.note_action(affordance);
            requests.write(PluginActionRequested(bytes, affordance.card));
            *claimed = claims(&panel.view, &pressed);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HiddenAction {
    Hide { zone: u16 },
    PlayFromFacedown,
    Reveal,
}

impl HiddenAction {
    pub fn label(self, zone_name: &dyn Fn(u16) -> String) -> String {
        match self {
            HiddenAction::Hide { zone } => {
                format!("hide at {} · 1 rune", zone_name(zone))
            }
            HiddenAction::PlayFromFacedown => "play from hidden".to_string(),
            HiddenAction::Reveal => "reveal".to_string(),
        }
    }
}

pub fn grey_hint(rims: &crate::table::highlight::Rims, card: u32, name: &str) -> Option<String> {
    rims.greys(card).then(|| format!("{name} · {GREYED_HINT}"))
}

pub fn hidden_actions(
    view: &PluginView,
    rims: &crate::table::highlight::Rims,
    card: u32,
    mine: bool,
    facedown: bool,
) -> Vec<HiddenAction> {
    if !mine {
        return Vec::new();
    }
    let mut out = Vec::new();
    for &zone in rims.hides(card) {
        if out.contains(&HiddenAction::Hide { zone }) {
            continue;
        }
        out.push(HiddenAction::Hide { zone });
    }
    if facedown {
        if view.legal.iter().any(|row| {
            row.card == card
                && row
                    .kinds
                    .iter()
                    .any(|kind| matches!(kind, agni_sim::wire::LegalKind::React))
        }) {
            out.push(HiddenAction::PlayFromFacedown);
        }
        out.push(HiddenAction::Reveal);
    }
    out
}

pub fn zone_label(zones: &[agni_sim::wire::ZoneDecl], zone: u16) -> String {
    zones
        .iter()
        .find(|decl| decl.id == zone)
        .map(|decl| decl.label.clone())
        .unwrap_or_else(|| format!("zone {zone}"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptKind {
    Mulligan,
    GroupMove { zone: u16 },
    PayOrLet { cost: String },
}

pub const MULLIGAN_WHY: &str = "set aside up to ";
pub const GROUP_MOVE_WHY: &str = "move others to {zone ";
pub const PAY_OR_LET_WHY: &str = "pay ";
pub const PAY_OR_LET_KEEP: &str = " to keep ";
pub const LET_IT_RESOLVE: &str = "let it resolve";

pub fn prompt_kind(view: &PluginView, me: u8) -> Option<PromptKind> {
    let summary = view.prompt.as_ref().filter(|summary| summary.seat == me)?;
    if summary.why.starts_with(MULLIGAN_WHY) && summary.why.ends_with(" to redraw") {
        return Some(PromptKind::Mulligan);
    }
    if let Some(rest) = summary.why.strip_prefix(PAY_OR_LET_WHY) {
        if let Some((cost, kept)) = rest.split_once(PAY_OR_LET_KEEP) {
            if kept.ends_with('?') && !cost.is_empty() {
                return Some(PromptKind::PayOrLet {
                    cost: cost.to_string(),
                });
            }
        }
    }
    let rest = summary.why.strip_prefix(GROUP_MOVE_WHY)?;
    let (zone, tail) = rest.split_once('}')?;
    (tail == " too?")
        .then(|| zone.parse().ok())
        .flatten()
        .map(|zone| PromptKind::GroupMove { zone })
}

pub fn answer_wording(view: &PluginView, me: u8, index: usize) -> Option<String> {
    let PromptKind::PayOrLet { cost } = prompt_kind(view, me)? else {
        return None;
    };
    let affordance = view.affordances.get(index)?;
    if affordance.card.is_some() {
        return None;
    }
    match affordance.label.as_str() {
        "yes" => Some(format!("pay {cost}")),
        "no" => Some(LET_IT_RESOLVE.to_string()),
        _ => None,
    }
}

pub fn answer_label(view: &PluginView, me: u8, index: usize) -> String {
    answer_wording(view, me, index)
        .or_else(|| view.affordances.get(index).map(|held| held.label.clone()))
        .unwrap_or_default()
}

pub fn group_move_options(view: &PluginView) -> Vec<usize> {
    view.affordances
        .iter()
        .enumerate()
        .filter(|(_, affordance)| affordance.enabled && affordance.card.is_some())
        .map(|(index, _)| index)
        .collect()
}

pub fn primary_label(view: &PluginView, me: u8) -> Option<String> {
    let picked = view.prompt.as_ref()?.picked;
    match prompt_kind(view, me)? {
        PromptKind::GroupMove { .. } => Some(format!("move {}", 1 + u32::from(picked))),
        PromptKind::Mulligan if picked > 0 => Some(format!("keep · {picked} set aside")),
        PromptKind::Mulligan | PromptKind::PayOrLet { .. } => None,
    }
}

pub const THINKING_SECS: f64 = 60.0;
pub const TRAY_FACE_W: f32 = 120.0;
pub const TRAY_FACE_H: f32 = 168.0;
pub const SELECTOR_MIN_OPTIONS: usize = 10;

#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct PromptSelector {
    pub search: String,
    pub key: Option<String>,
    pub catalog_generation: u32,
    pub terms: Vec<String>,
    pub open: bool,
}

pub fn selector_options(view: &PluginView, me: u8) -> Vec<usize> {
    let mine = view
        .prompt
        .as_ref()
        .is_some_and(|summary| summary.seat == me);
    mine.then(|| {
        view.shown()
            .into_iter()
            .filter(|(index, affordance)| {
                affordance.enabled
                    && !is_reveal(affordance)
                    && Some(*index) != cancel_index(view)
                    && !menu_only(view, *index)
            })
            .map(|(index, _)| index)
            .collect()
    })
    .filter(|options: &Vec<usize>| options.len() >= SELECTOR_MIN_OPTIONS)
    .unwrap_or_default()
}

pub fn selector_matches(label: &str, search: &str) -> bool {
    search
        .split_whitespace()
        .all(|word| label.to_lowercase().contains(&word.to_lowercase()))
}

pub fn selector_search(
    view: &PluginView,
    index: usize,
    table: &Table,
    mirror: &Mirror,
    me: PlayerId,
    catalog: &crate::deck::catalog::Catalog,
) -> String {
    let Some(affordance) = view.affordances.get(index) else {
        return String::new();
    };
    let label = affordance
        .card
        .map(|card| card_label(table, &mirror.view, me, card))
        .unwrap_or_else(|| answer_label(view, me.0, index));
    let mut terms = vec![label.clone()];
    let mut names = Vec::new();
    if label != FACE_DOWN {
        names.push(label);
    }
    for name in names {
        if let Some(group) = catalog
            .find_name(&name)
            .and_then(|index| catalog.groups.get(index))
        {
            terms.push(group.name.clone());
            terms.extend(group.tags.iter().cloned());
            terms.push(group.text_lower.clone());
        }
    }
    if affordance.card.is_none() {
        for group in &catalog.groups {
            if group
                .tags
                .iter()
                .any(|tag| tag.eq_ignore_ascii_case(&terms[0]))
            {
                terms.push(group.name.clone());
                terms.extend(group.tags.iter().cloned());
            }
        }
    }
    terms.join(" ")
}

pub fn selector_key(view: &PluginView, options: &[usize]) -> String {
    let question = view
        .prompt
        .as_ref()
        .map(|summary| summary.why.as_str())
        .unwrap_or_default();
    let labels = options
        .iter()
        .filter_map(|index| view.affordances.get(*index))
        .map(|affordance| affordance.label.as_str())
        .collect::<Vec<_>>()
        .join("\u{1f}");
    format!("{question}\u{1e}{labels}")
}

pub fn selector_terms(
    view: &PluginView,
    options: &[usize],
    table: &Table,
    mirror: &Mirror,
    me: PlayerId,
    catalog: &crate::deck::catalog::Catalog,
) -> Vec<String> {
    options
        .iter()
        .map(|index| selector_search(view, *index, table, mirror, me, catalog))
        .collect()
}

pub fn prompt_selector_ui(
    mut contexts: EguiContexts,
    hud: Res<super::hud::Hud>,
    panel: Res<PluginPanel>,
    table: Res<GameTable>,
    mirror: Res<Mirror>,
    my_seat: Res<MySeat>,
    catalog: Res<crate::deck::catalog::Catalog>,
    menu: Res<crate::menu::Menu>,
    mut selector: ResMut<PromptSelector>,
    mut sender: super::hud::Sender,
) -> Result {
    if !menu.at_table() {
        selector.key = None;
        selector.open = false;
        return Ok(());
    }
    let options = selector_options(&panel.view, my_seat.0 .0);
    if options.is_empty() {
        selector.key = None;
        selector.open = false;
        return Ok(());
    }
    let key = selector_key(&panel.view, &options);
    if selector.key.as_deref() != Some(&key) {
        selector.key = Some(key);
        selector.search.clear();
        selector.terms.clear();
        selector.open = true;
    }
    if !selector.open {
        return Ok(());
    }
    let context = contexts.ctx_mut()?.clone();
    let mut open = true;
    let mut picked = None;
    super::hud::sheet(
        &context,
        "prompt selector",
        hud.0.class,
        super::hud::Side::Right,
        &format!("choose · {} options", options.len()),
        &mut open,
        |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut selector.search)
                    .hint_text("search choices")
                    .desired_width(f32::INFINITY),
            );
            if !selector.search.trim().is_empty()
                && (selector.terms.len() != options.len()
                    || selector.catalog_generation != catalog.generation
                    || table.is_changed()
                    || mirror.is_changed())
            {
                selector.terms = selector_terms(
                    &panel.view,
                    &options,
                    &table.0,
                    &mirror,
                    my_seat.0,
                    &catalog,
                );
                selector.catalog_generation = catalog.generation;
            }
            egui::ScrollArea::vertical()
                .id_salt("prompt selector options")
                .max_height(440.0)
                .show(ui, |ui| {
                    for (slot, index) in options.iter().enumerate() {
                        let label = panel.view.affordances[*index]
                            .card
                            .map(|card| card_label(&table.0, &mirror.view, my_seat.0, card))
                            .unwrap_or_else(|| answer_label(&panel.view, my_seat.0 .0, *index));
                        let matches = selector.search.trim().is_empty()
                            || selector
                                .terms
                                .get(slot)
                                .is_some_and(|terms| selector_matches(terms, &selector.search));
                        if matches && ui.button(label).clicked() {
                            picked = Some(*index);
                        }
                    }
                });
        },
    );
    selector.open = open;
    if let Some(index) = picked {
        sender.fire(&panel.view.affordances[index]);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StripState {
    Prompt {
        question: String,
        count: String,
        cancel: Option<usize>,
        chips: Vec<usize>,
    },
    Response {
        text: String,
    },
    Waiting {
        seat: Option<u8>,
        text: String,
    },
    Opening {
        lines: Vec<String>,
        chips: Vec<usize>,
    },
}

impl StripState {
    pub fn headline(&self) -> String {
        match self {
            StripState::Prompt { question, .. } => question.clone(),
            StripState::Response { text } | StripState::Waiting { text, .. } => text.clone(),
            StripState::Opening { lines, .. } => lines.first().cloned().unwrap_or_default(),
        }
    }

    pub fn rows(&self) -> usize {
        match self {
            StripState::Prompt { .. } => 2,
            StripState::Response { .. } | StripState::Waiting { .. } => 1,
            StripState::Opening { lines, .. } => lines.len() + 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StripRows {
    pub leftover: bool,
}

pub fn strip_rows(budget: usize, refusal: bool, state_rows: usize, leftover: bool) -> StripRows {
    let left = budget.saturating_sub(usize::from(refusal) + state_rows);
    StripRows {
        leftover: leftover && left > 0,
    }
}

pub fn strip_frame<R>(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.set_clip_rect(rect);
    super::hud::panel_frame(ui.style())
        .show(ui, |ui| {
            ui.set_min_width(rect.width() - 16.0);
            ui.set_max_width(rect.width() - 16.0);
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            body(ui)
        })
        .inner
}

pub fn count_chip(summary: &PromptSummary) -> String {
    let mut chip = match (summary.min, summary.max) {
        (_, 0 | 1) => "1 of 1".to_string(),
        (min, max) if min == max => format!("{} of {max}", summary.picked),
        (0, max) => format!("{} picked · up to {max}", summary.picked),
        (min, max) => format!("{} picked · {min} to {max}", summary.picked),
    };
    if summary.optional {
        chip.push_str(" · optional");
    }
    chip
}

pub fn cancel_index(view: &PluginView) -> Option<usize> {
    view.affordances
        .iter()
        .enumerate()
        .position(|(index, affordance)| {
            affordance.enabled && effective_hotkey(view, index) == Some(KeyCode::KeyX)
        })
}

fn is_reveal(affordance: &Affordance) -> bool {
    matches!(affordance.kind, AffordanceKind::Reveal { .. })
}

pub fn strip_chips(view: &PluginView) -> Vec<usize> {
    let primary = super::primary::primary_of(view).and_then(|primary| primary.affordance);
    let secondary = primary.and_then(|_| super::primary::end_turn_index(view));
    let cancel = cancel_index(view);
    let open = view.prompt.is_some();
    let mut chips: Vec<usize> = view
        .affordances
        .iter()
        .enumerate()
        .filter(|(index, affordance)| {
            affordance.enabled
                && !is_reveal(affordance)
                && Some(*index) != primary
                && Some(*index) != secondary
                && Some(*index) != cancel
                && !menu_only(view, *index)
                && affordance.card.is_none()
        })
        .map(|(index, _)| index)
        .collect();
    chips.sort_by_key(|index| group(&view.affordances[*index], open));
    chips
}

pub fn strip_state(view: &PluginView, me: u8) -> Option<StripState> {
    use super::plate::StatusLine;
    let lines = super::plate::lines(view);
    if let Some(summary) = &view.prompt {
        if summary.seat == me {
            return Some(StripState::Prompt {
                question: summary.why.clone(),
                count: count_chip(summary),
                cancel: cancel_index(view),
                chips: strip_chips(view),
            });
        }
        return Some(StripState::Waiting {
            seat: Some(summary.seat),
            text: format!("waiting for {{seat {}}} · {}", summary.seat, summary.why),
        });
    }
    let opening = lines.iter().any(|line| {
        matches!(
            line,
            StatusLine::Roll { .. } | StatusLine::RollWon { .. } | StatusLine::RollMarks(_)
        )
    });
    if opening {
        let shown: Vec<String> = lines
            .iter()
            .zip(&view.status)
            .filter_map(|(line, raw)| match line {
                StatusLine::RollMarks(_) | StatusLine::RollWon { .. } => Some(raw.clone()),
                StatusLine::Waiting { .. } => Some(raw.clone()),
                _ => None,
            })
            .collect();
        return Some(StripState::Opening {
            lines: shown,
            chips: strip_chips(view),
        });
    }
    if super::plate::pass_offered(view) {
        if let Some(top) = view.chain.last() {
            let what = top
                .card
                .map(|card| format!("{{card {card}}}"))
                .unwrap_or_else(|| "an ability".to_string());
            return Some(StripState::Response {
                text: format!("{{seat {}}} played {what} — respond or pass", top.seat),
            });
        }
        let showdown = lines
            .iter()
            .zip(&view.status)
            .find_map(|(line, raw)| match line {
                StatusLine::Showdown { .. } => Some(raw.clone()),
                _ => None,
            });
        return Some(StripState::Response {
            text: showdown
                .map(|line| format!("{line} — pass or respond"))
                .unwrap_or_else(|| "respond or pass".to_string()),
        });
    }
    if let Some(waiting) = super::plate::waiting_of(view) {
        let seat = waiting.seat;
        let what = Some(waiting.what).filter(|what| !what.is_empty());
        let text = match (seat, what) {
            (Some(seat), Some(what)) => format!("waiting for {{seat {seat}}} · {what}"),
            (Some(seat), None) => format!("waiting for {{seat {seat}}}"),
            (None, Some(what)) => format!("waiting for {what}"),
            (None, None) => "waiting".to_string(),
        };
        return Some(StripState::Waiting { seat, text });
    }
    None
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrayItem {
    pub label: String,
    pub texture: Option<egui::TextureId>,
    pub frame: Option<[u8; 3]>,
    pub wide: bool,
}

#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct TrayItems {
    pub title: String,
    pub items: Vec<TrayItem>,
    pub picked: Option<usize>,
    pub peek: bool,
}

impl TrayItems {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.picked = None;
    }
}

pub fn faceless_options(view: &PluginView, shown: &BTreeSet<u32>) -> Vec<usize> {
    view.affordances
        .iter()
        .enumerate()
        .filter(|(_, affordance)| {
            affordance.enabled && affordance.card.is_some_and(|card| !shown.contains(&card))
        })
        .map(|(index, _)| index)
        .collect()
}

pub fn tray_ui(
    context: &egui::Context,
    rect: egui::Rect,
    title: &str,
    items: &[TrayItem],
    peek: &mut bool,
) -> Option<usize> {
    let mut picked = None;
    egui::Area::new(egui::Id::new("tray"))
        .fixed_pos(rect.min)
        .order(egui::Order::Middle)
        .show(context, |ui| {
            ui.set_max_size(rect.size());
            super::hud::panel_frame(ui.style()).show(ui, |ui| {
                ui.set_max_width(rect.width() - 16.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(title).strong().color(super::hud::INK));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = if *peek { "show faces" } else { "peek at table" };
                        if ui.small_button(label).clicked() {
                            *peek = !*peek;
                        }
                    });
                });
                if *peek {
                    ui.horizontal_wrapped(|ui| {
                        for (index, item) in items.iter().enumerate() {
                            if ui.button(&item.label).clicked() {
                                picked = Some(index);
                            }
                        }
                    });
                    return;
                }
                egui::ScrollArea::horizontal()
                    .id_salt("tray faces")
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            for (index, item) in items.iter().enumerate() {
                                ui.vertical(|ui| {
                                    let size = if item.wide {
                                        egui::vec2(TRAY_FACE_H, TRAY_FACE_W)
                                    } else {
                                        egui::vec2(TRAY_FACE_W, TRAY_FACE_H)
                                    };
                                    ui.set_width(size.x);
                                    let hit = match item.texture {
                                        Some(texture) => ui.add(egui::Button::image(
                                            egui::load::SizedTexture::new(texture, size),
                                        )),
                                        None => {
                                            let (face, response) =
                                                ui.allocate_exact_size(size, egui::Sense::click());
                                            ui.painter().rect_filled(
                                                face,
                                                4.0,
                                                egui::Color32::from_gray(58),
                                            );
                                            response
                                        }
                                    };
                                    if let Some(frame) = item.frame {
                                        ui.painter().rect_stroke(
                                            hit.rect,
                                            4.0,
                                            egui::Stroke::new(
                                                2.0,
                                                egui::Color32::from_rgb(
                                                    frame[0], frame[1], frame[2],
                                                ),
                                            ),
                                            egui::StrokeKind::Outside,
                                        );
                                    }
                                    let label = egui::Button::new(item.label.as_str())
                                        .min_size(egui::vec2(size.x, 0.0));
                                    if hit.clicked() || ui.add(label).clicked() {
                                        picked = Some(index);
                                    }
                                });
                            }
                        });
                    });
            });
        });
    picked
}

fn spinner(ui: &mut egui::Ui, seconds: f64) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
    let angle = (seconds * std::f64::consts::TAU) as f32;
    let center = rect.center();
    let points: Vec<egui::Pos2> = (0..12)
        .map(|step| {
            let theta = angle + step as f32 / 12.0 * std::f32::consts::TAU * 0.75;
            egui::pos2(center.x + theta.cos() * 5.0, center.y + theta.sin() * 5.0)
        })
        .collect();
    ui.painter().add(egui::Shape::line(
        points,
        egui::Stroke::new(2.0, super::hud::INK_WEAK),
    ));
    ui.ctx().request_repaint();
}

pub fn chip_label(digit: Option<usize>, label: &str) -> String {
    match digit {
        Some(digit) => format!("{digit} · {label}"),
        None => label.to_string(),
    }
}

fn chip_button(ui: &mut egui::Ui, label: &str, hollow: bool, digit: Option<usize>) -> bool {
    let button = if hollow {
        egui::Button::new(egui::RichText::new(format!("× {label}")).color(super::hud::INK_WEAK))
            .fill(egui::Color32::TRANSPARENT)
            .stroke(egui::Stroke::new(1.0, super::hud::INK_WEAK))
            .corner_radius(12.0)
    } else {
        egui::Button::new(egui::RichText::new(chip_label(digit, label)).color(super::hud::INK))
            .fill(super::hud::SURFACE_2)
            .corner_radius(12.0)
    };
    ui.add(button).clicked()
}

#[allow(clippy::too_many_arguments)]
pub fn plugin_ui(
    mut contexts: EguiContexts,
    hud: Res<super::hud::Hud>,
    panel: Res<PluginPanel>,
    seats: super::hud::Seats,
    mut sender: super::hud::Sender,
    time: Res<Time>,
    refusals: Res<super::toast::Refusals>,
    menu: Res<crate::menu::Menu>,
    mut art: super::hud::Art,
    mut tray: ResMut<TrayItems>,
    mut banner: ResMut<super::hud::Banner>,
    mut selector: ResMut<PromptSelector>,
    mut pile_sheet: ResMut<super::ui::PileSheet>,
    shown_cards: Query<(&CardView, &ViewVisibility)>,
    mut waiting_since: Local<Option<(String, f64)>>,
) -> Result {
    let now = time.elapsed_secs_f64();
    let refusal = refusals.on_strip(now).map(|refusal| refusal.text.clone());
    let table = &*seats.table;
    let mirror = &*seats.mirror;
    let info = &*seats.info;
    let my_seat = seats.me();
    if !info.active() {
        sender.secrets.forget();
    }
    if !menu.at_table() {
        return Ok(());
    }
    let seat_name = |seat: u8| seats.name(seat);
    let zone_name = |zone: u16| zone_label(&mirror.view.zones, zone);
    let card_name = |card: u32| card_label(&table.0, &mirror.view, my_seat, card);
    let expanded = |text: &str| expand(text, &seat_name, &zone_name, &card_name);
    let state = strip_state(&panel.view, my_seat.0);
    let leftover: Vec<usize> = if matches!(
        state,
        Some(StripState::Prompt { .. }) | Some(StripState::Opening { .. })
    ) {
        Vec::new()
    } else {
        strip_chips(&panel.view)
    };
    let empty_seat = info.role == SessionRole::Host
        && state.is_none()
        && panel.view.status.is_empty()
        && info.roster.len() < seats.players.0;
    let budget = (hud.0.strip.height() / super::hud::STRIP_ROW_H).round() as usize;
    let rows = strip_rows(
        budget,
        refusal.is_some(),
        state
            .as_ref()
            .map(StripState::rows)
            .unwrap_or(usize::from(empty_seat)),
        !leftover.is_empty(),
    );
    let leftover = if rows.leftover { leftover } else { Vec::new() };
    let pending = state.is_some() || refusal.is_some() || !leftover.is_empty() || empty_seat;
    if !pending {
        *waiting_since = None;
        if tray.is_empty() {
            return Ok(());
        }
    }
    let context = contexts.ctx_mut()?.clone();
    let rect = hud.0.strip;
    let phone = hud.0.class.is_phone();
    let collapsed = phone && banner.collapsed;
    let mut fire: Option<usize> = None;
    let mut waiting_key = None;
    let mut toggle_banner = false;
    let strip = |ui: &mut egui::Ui| {
        strip_frame(ui, rect, |ui| {
            if phone {
                let chevron = egui::Rect::from_min_size(
                    egui::pos2(rect.max.x - 30.0, rect.min.y + 2.0),
                    egui::vec2(24.0, 24.0),
                );
                let hit = ui.interact(
                    chevron,
                    egui::Id::new("banner chevron"),
                    egui::Sense::click(),
                );
                let centre = chevron.center();
                let (dy, tip) = if collapsed { (-4.0, 4.0) } else { (4.0, -4.0) };
                ui.painter().add(egui::Shape::line(
                    vec![
                        egui::pos2(centre.x - 6.0, centre.y + dy),
                        egui::pos2(centre.x, centre.y + tip),
                        egui::pos2(centre.x + 6.0, centre.y + dy),
                    ],
                    egui::Stroke::new(2.0, super::hud::INK_WEAK),
                ));
                if hit.clicked() {
                    toggle_banner = true;
                }
            }
            if collapsed {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                ui.set_max_width(rect.width() - 48.0);
                let line = refusal
                    .clone()
                    .or_else(|| state.as_ref().map(|state| expanded(&state.headline())))
                    .unwrap_or_default();
                ui.label(egui::RichText::new(line).color(super::hud::INK));
                return;
            }
            if let Some(text) = &refusal {
                super::toast::strip_row(ui, text);
            }
            match &state {
                Some(StripState::Prompt {
                    question,
                    count,
                    cancel,
                    chips,
                }) => {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            egui::RichText::new(expanded(question))
                                .strong()
                                .color(super::hud::INK),
                        );
                        ui.label(
                            egui::RichText::new(count)
                                .size(11.0)
                                .color(super::hud::INK)
                                .background_color(super::hud::SURFACE_2),
                        );
                    });
                    ui.horizontal_wrapped(|ui| {
                        for (zone, seat, count) in
                            super::ui::discard_piles(&mirror.view.zones, &table.0, seats.players.0)
                        {
                            let label = if seat == my_seat {
                                format!("trash · {count}")
                            } else {
                                format!("{}'s trash · {count}", seats.name(seat.0))
                            };
                            if chip_button(ui, &label, false, None) {
                                pile_sheet.0 = super::ui::toggle_pile(pile_sheet.0, zone, seat);
                            }
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        if let Some(index) = cancel {
                            let label = expanded(&answer_label(&panel.view, my_seat.0, *index));
                            if chip_button(ui, &label, true, None) {
                                fire = Some(*index);
                            }
                        }
                        let options = selector_options(&panel.view, my_seat.0);
                        if options.is_empty() {
                            for (slot, index) in chips.iter().enumerate() {
                                let label = expanded(&answer_label(&panel.view, my_seat.0, *index));
                                let digit = effective_digit(&panel.view, *index)
                                    .or_else(|| (slot < 9).then_some(slot + 1));
                                if chip_button(ui, &label, false, digit) {
                                    fire = Some(*index);
                                }
                            }
                        } else if chip_button(
                            ui,
                            &format!("search {} choices", options.len()),
                            false,
                            None,
                        ) {
                            selector.open = true;
                        }
                    });
                }
                Some(StripState::Response { text }) => {
                    ui.label(
                        egui::RichText::new(expanded(text))
                            .strong()
                            .color(super::hud::INK),
                    );
                }
                Some(StripState::Waiting { seat, text }) => {
                    let text = expanded(text);
                    waiting_key = Some(text.clone());
                    let since = match &*waiting_since {
                        Some((key, since)) if *key == text => *since,
                        _ => now,
                    };
                    let elapsed = now - since;
                    let disconnected = seat.is_some_and(|seat| {
                        info.roster
                            .iter()
                            .any(|held| held.seat == seat && !held.connected)
                    });
                    ui.horizontal_wrapped(|ui| {
                        spinner(ui, elapsed);
                        ui.label(egui::RichText::new(text).color(super::hud::INK));
                        ui.label(
                            egui::RichText::new(format!("{} s", elapsed as u64))
                                .size(11.0)
                                .color(super::hud::INK_WEAK),
                        );
                        if disconnected {
                            ui.label(
                                egui::RichText::new("disconnected")
                                    .strong()
                                    .color(super::hud::DANGER),
                            );
                        } else if elapsed >= THINKING_SECS {
                            ui.label(
                                egui::RichText::new("still thinking…")
                                    .italics()
                                    .color(super::hud::INK_WEAK),
                            );
                        }
                    });
                }
                Some(StripState::Opening { lines, chips }) => {
                    for line in lines {
                        ui.label(egui::RichText::new(expanded(line)).color(super::hud::INK));
                    }
                    ui.horizontal_wrapped(|ui| {
                        for index in chips {
                            let label = expanded(&panel.view.affordances[*index].label);
                            if chip_button(ui, &label, false, None) {
                                fire = Some(*index);
                            }
                        }
                    });
                }
                None => {
                    if empty_seat {
                        ui.label(
                            egui::RichText::new(
                                "waiting for a player · share the ticket from the lobby",
                            )
                            .color(super::hud::INK),
                        );
                    }
                }
            }
            if !leftover.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    for index in &leftover {
                        let label = expanded(&panel.view.affordances[*index].label);
                        if chip_button(ui, &label, false, None) {
                            fire = Some(*index);
                        }
                    }
                });
            }
        });
    };
    if pending {
        super::hud::slot(&context, "plugin strip", rect, strip);
    }
    if toggle_banner {
        banner.collapsed = !banner.collapsed;
    }
    *waiting_since = waiting_key.map(|key| match waiting_since.take() {
        Some((held, since)) if held == key => (held, since),
        _ => (key, now),
    });
    let shown: BTreeSet<u32> = shown_cards
        .iter()
        .filter(|(_, visibility)| visibility.get())
        .map(|(view, _)| view.0 .0)
        .collect();
    let mulligan = prompt_kind(&panel.view, my_seat.0) == Some(PromptKind::Mulligan);
    let faceless = if matches!(state, Some(StripState::Prompt { .. })) && !mulligan {
        faceless_options(&panel.view, &shown)
    } else {
        Vec::new()
    };
    if !faceless.is_empty() {
        let items: Vec<TrayItem> = faceless
            .iter()
            .map(|index| {
                let affordance = &panel.view.affordances[*index];
                let card = affordance.card.and_then(|card| table.0.get(CardId(card)));
                let texture = card
                    .filter(|card| !card.face.is_hidden())
                    .and_then(|card| art.texture(&mut contexts, &card.face.name));
                let frame = card
                    .filter(|card| card.owner != my_seat)
                    .map(|card| seats.label(card.owner).1);
                TrayItem {
                    label: expanded(&affordance.label),
                    texture,
                    frame,
                    wide: false,
                }
            })
            .collect();
        let title = panel
            .view
            .prompt
            .as_ref()
            .map(|summary| expanded(&summary.why))
            .unwrap_or_default();
        let mut peek = tray.peek;
        if let Some(picked) = tray_ui(&context, hud.0.banner, &title, &items, &mut peek) {
            fire = Some(faceless[picked]);
        }
        if tray.peek != peek {
            tray.peek = peek;
        }
    } else if !tray.is_empty() {
        let mut peek = tray.peek;
        let title = tray.title.clone();
        let picked = tray_ui(&context, hud.0.banner, &title, &tray.items, &mut peek);
        if tray.peek != peek {
            tray.peek = peek;
        }
        if picked.is_some() && tray.picked != picked {
            tray.picked = picked;
        }
    }
    if let Some(index) = fire {
        sender.fire(&panel.view.affordances[index]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agni_core::CardFace;
    use serde_bytes::ByteBuf;

    #[test]
    fn the_roll_waits_for_the_battlefield_and_every_other_offer_stands() {
        let mut view = PluginView {
            affordances: vec![
                Affordance {
                    kind: AffordanceKind::Commit { roll: 1 },
                    ..offer("roll", None, 0)
                },
                offer("pass", Some("w"), 1),
                Affordance {
                    enabled: false,
                    kind: AffordanceKind::Commit { roll: 1 },
                    ..offer("roll again", None, 2)
                },
            ],
            ..Default::default()
        };
        gate_roll(&mut view);
        assert!(!view.affordances[0].enabled);
        assert_eq!(
            view.affordances[0].label,
            format!("roll · {}", crate::deck::battlefield::ROLL_GATE)
        );
        assert!(view.affordances[1].enabled, "passing is not the roll");
        assert_eq!(
            view.affordances[2].label, "roll again",
            "an already disabled roll keeps its label"
        );
    }

    fn offer(label: &str, hotkey: Option<&str>, data: u8) -> Affordance {
        Affordance {
            label: label.into(),
            hotkey: hotkey.map(str::to_string),
            enabled: true,
            kind: AffordanceKind::Plain,
            data: ByteBuf::from(vec![data]),
            card: None,
        }
    }

    fn names() -> (
        impl Fn(u8) -> String,
        impl Fn(u16) -> String,
        impl Fn(u32) -> String,
    ) {
        (
            |seat: u8| format!("seat-{seat}"),
            |zone: u16| format!("zone-{zone}"),
            |card: u32| format!("card-{card}"),
        )
    }

    #[test]
    fn the_strip_budget_keeps_three_rows_and_the_leftover_chips_go_first() {
        let prompt = StripState::Prompt {
            question: "choose".into(),
            count: "0 of 1".into(),
            cancel: None,
            chips: Vec::new(),
        };
        assert_eq!(prompt.rows(), 2);
        let crowded = strip_rows(3, true, prompt.rows(), true);
        assert_eq!(
            crowded,
            StripRows { leftover: false },
            "a refusal over a prompt fills the strip"
        );
        let idle = strip_rows(3, false, 0, true);
        assert_eq!(idle, StripRows { leftover: true });
        let opening = StripState::Opening {
            lines: vec!["a".into(), "b".into(), "c".into()],
            chips: Vec::new(),
        };
        assert_eq!(opening.rows(), 4);
        assert_eq!(
            strip_rows(3, false, opening.rows(), true),
            StripRows { leftover: false }
        );
        assert_eq!(chip_label(Some(2), "your base"), "2 · your base");
        assert_eq!(chip_label(None, "your base"), "your base");
    }

    #[test]
    fn the_strip_frame_never_paints_past_its_slot() {
        let context = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 800.0));
        let input = egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        };
        let rect = egui::Rect::from_min_size(
            egui::pos2(300.0, 12.0),
            egui::vec2(520.0, 3.0 * super::super::hud::STRIP_ROW_H),
        );
        let mut output = None;
        for _ in 0..2 {
            context.begin_pass(input.clone());
            super::super::hud::slot(&context, "plugin strip", rect, |ui| {
                strip_frame(ui, rect, |ui| {
                    for row in 0..8 {
                        ui.label(format!("row {row} of a strip that has too much to say"));
                    }
                });
            });
            let mut full = context.end_pass();
            full.textures_delta.clear();
            output = Some(full);
        }
        let output = output.unwrap();
        let painted: Vec<egui::Rect> = output
            .shapes
            .iter()
            .map(|clipped| clipped.clip_rect)
            .collect();
        assert!(!painted.is_empty());
        assert!(
            painted
                .iter()
                .all(|clip| rect.expand(1.0).contains_rect(*clip)),
            "every shape is clipped to the strip's slot: {painted:?}"
        );
    }

    #[test]
    fn a_commit_press_appends_a_commitment_and_the_reveal_follows_exactly_once() {
        let mut secrets = RollSecrets::default();
        let fresh = || [4u8; dice::SECRET_LEN];
        let commit = Affordance {
            kind: AffordanceKind::Commit { roll: 1 },
            ..offer("roll", None, 6)
        };
        let sent = action_bytes(&commit, &mut secrets, &fresh).unwrap();
        assert_eq!(sent[0], 6);
        assert_eq!(&sent[1..], &dice::commitment(&[4; dice::SECRET_LEN]));
        let again = action_bytes(&commit, &mut secrets, &|| [9u8; dice::SECRET_LEN]).unwrap();
        assert_eq!(
            again, sent,
            "a second press before the reveal repeats the same commitment, so the reveal still matches"
        );
        let reveal = Affordance {
            kind: AffordanceKind::Reveal { roll: 1 },
            ..offer("reveal", None, 7)
        };
        let revealed = action_bytes(&reveal, &mut secrets, &fresh).unwrap();
        assert_eq!(revealed, [7, 4, 4, 4, 4, 4, 4, 4, 4]);
        assert!(action_bytes(&reveal, &mut secrets, &fresh).is_none());
        let next_round = action_bytes(&commit, &mut secrets, &|| [9u8; dice::SECRET_LEN]).unwrap();
        assert_eq!(
            &next_round[1..],
            &dice::commitment(&[9; dice::SECRET_LEN]),
            "once revealed, the same roll id takes a fresh secret"
        );
        let unknown = Affordance {
            kind: AffordanceKind::Reveal { roll: 2 },
            ..offer("reveal", None, 7)
        };
        assert!(action_bytes(&unknown, &mut secrets, &fresh).is_none());
        let plain = offer("end turn", None, 2);
        assert_eq!(action_bytes(&plain, &mut secrets, &fresh).unwrap(), [2]);
    }

    #[test]
    fn undo_rearms_the_original_dice_secret_without_changing_its_commitment() {
        let mut secrets = RollSecrets::default();
        let original = [4; dice::SECRET_LEN];
        let commitment = secrets.commit(7, original);
        assert_eq!(secrets.take_reveal(7), Some(original));
        assert_eq!(secrets.take_reveal(7), None);
        secrets.rearm();
        assert_eq!(secrets.commit(7, [9; dice::SECRET_LEN]), commitment);
        assert_eq!(secrets.take_reveal(7), Some(original));
        assert_eq!(secrets.take_reveal(7), None);
    }

    #[test]
    fn placeholders_expand_to_the_clients_own_names_and_unknown_ones_survive() {
        let (seat, zone, card) = names();
        assert_eq!(
            expand("turn 1 · {seat 0} · {zone 9}", &seat, &zone, &card),
            "turn 1 · seat-0 · zone-9"
        );
        assert_eq!(
            expand("set aside {card 3}", &seat, &zone, &card),
            "set aside card-3"
        );
        assert_eq!(
            expand("{card x} stays {odd", &seat, &zone, &card),
            "{card x} stays {odd"
        );
        assert_eq!(expand("plain", &seat, &zone, &card), "plain");
    }

    #[test]
    fn a_card_placeholder_names_a_visible_face_and_hides_a_face_down_one() {
        let mut table = Table::new();
        let zones = agni_riftbound::zone_table();
        let base = Zone::Plugin(agni_riftbound::ZONE_BASE);
        let hand = Zone::Plugin(agni_riftbound::ZONE_HAND);
        let poro = table.add(PlayerId(0), base, "Punching Poro", [0; 3]);
        let hidden_play = table.add(PlayerId(0), base, "Cleave", [0; 3]);
        let mine = table.add(PlayerId(0), hand, "Lure of the Depths", [0; 3]);
        let theirs = table.add_face(PlayerId(1), hand, CardFace::hidden());
        let view = TableView {
            zones,
            revealed: vec![poro.0],
            ..Default::default()
        };
        let me = PlayerId(1);
        assert_eq!(card_label(&table, &view, me, poro.0), "Punching Poro");
        assert_eq!(card_label(&table, &view, me, hidden_play.0), FACE_DOWN);
        assert_eq!(card_label(&table, &view, me, mine.0), "Lure of the Depths");
        assert_eq!(card_label(&table, &view, me, theirs.0), FACE_DOWN);
        assert_eq!(card_label(&table, &view, me, 99), "card 99");
        assert_eq!(
            card_label(&table, &view, PlayerId(0), hidden_play.0),
            "Cleave",
            "the seat that hid a card reads its own name"
        );
        let peeked = TableView {
            peeked: vec![hidden_play.0],
            ..view.clone()
        };
        assert_eq!(
            card_label(&table, &peeked, me, hidden_play.0),
            "Cleave",
            "and so does a looker who was granted the face"
        );
        assert_eq!(
            card_label(&table, &view, PlayerId(2), hidden_play.0),
            FACE_DOWN,
            "everyone else still reads a face-down card"
        );
        let (seat, zone) = (
            |seat: u8| format!("seat-{seat}"),
            |zone: u16| format!("zone-{zone}"),
        );
        let card = |card: u32| card_label(&table, &view, me, card);
        assert_eq!(
            expand(
                &format!("set aside {{card {}}} or {{card {}}}", poro.0, theirs.0),
                &seat,
                &zone,
                &card
            ),
            "set aside Punching Poro or a face-down card"
        );
    }

    #[test]
    fn a_click_on_a_card_an_enabled_affordance_names_routes_its_bytes() {
        let view = PluginView {
            affordances: vec![
                Affordance {
                    card: Some(7),
                    enabled: false,
                    ..offer("set aside {card 7}", None, 1)
                },
                Affordance {
                    card: Some(7),
                    ..offer("set aside {card 7}", None, 2)
                },
                Affordance {
                    card: Some(8),
                    ..offer("set aside {card 8}", None, 3)
                },
                offer("keep", None, 4),
            ],
            ..Default::default()
        };
        let mut secrets = RollSecrets::default();
        let fresh = || [0u8; dice::SECRET_LEN];
        let picked = card_affordance(&view, 7).unwrap();
        assert_eq!(action_bytes(picked, &mut secrets, &fresh).unwrap(), [2]);
        assert!(card_affordance(&view, 9).is_none());
        assert_eq!(highlighted(&view).into_iter().collect::<Vec<_>>(), [7, 8]);
        let mut quiet = view.clone();
        quiet.affordances[2].enabled = false;
        assert_eq!(highlighted(&quiet).into_iter().collect::<Vec<_>>(), [7]);
    }

    #[test]
    fn two_enabled_offers_on_one_card_leave_the_click_to_the_strip() {
        let view = PluginView {
            affordances: vec![
                Affordance {
                    card: Some(7),
                    ..offer("{card 7}: play a Sprite (3 energy, exhaust)", None, 1)
                },
                Affordance {
                    card: Some(7),
                    ..offer("{card 7}: draw a card (exhaust)", None, 2)
                },
                Affordance {
                    card: Some(8),
                    ..offer("{card 8}: draw a card (exhaust)", None, 3)
                },
            ],
            ..Default::default()
        };
        assert_eq!(card_affordances(&view, 7).len(), 2);
        assert!(
            card_affordance(&view, 7).is_none(),
            "a click cannot guess which of the two the player meant"
        );
        assert_eq!(
            card_affordance(&view, 8).map(|affordance| affordance.label.as_str()),
            Some("{card 8}: draw a card (exhaust)"),
            "one offer still answers a click"
        );
        assert_eq!(highlighted(&view).into_iter().collect::<Vec<_>>(), [7, 8]);
    }

    fn pick_bytes(prompt: u16, option: u16) -> Vec<u8> {
        let mut bytes = vec![10];
        bytes.extend(agni_plugin_sdk::prompt::Pick { prompt, option }.encode());
        bytes
    }

    fn picked(bytes: &[u8]) -> agni_plugin_sdk::prompt::Pick {
        assert_eq!(bytes[0], 10);
        agni_plugin_sdk::prompt::Pick::decode(&bytes[1..]).unwrap()
    }

    fn m1_prompt(
        why: &str,
        max: u8,
        options: Vec<(String, Option<u32>, Option<&str>)>,
    ) -> PluginView {
        let affordances = options
            .into_iter()
            .enumerate()
            .map(|(index, (label, card, hotkey))| Affordance {
                label,
                hotkey: hotkey.map(str::to_string),
                enabled: true,
                kind: AffordanceKind::Plain,
                data: ByteBuf::from(pick_bytes(3, index as u16)),
                card,
            })
            .chain(std::iter::once(offer("free table", None, 13)))
            .collect();
        PluginView {
            status: vec![
                "turn 1 · {seat 0} · setup · rules enforced".into(),
                why.into(),
            ],
            affordances,
            prompt: Some(PromptSummary {
                seat: 0,
                why: why.into(),
                min: 0,
                max,
                picked: 0,
                optional: false,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn the_m1_presenter_options_with_cards_highlight_click_and_number_the_same_way() {
        let mulligan = m1_prompt(
            "set aside up to 2 cards to redraw",
            2,
            vec![
                ("set aside {card 70}".into(), Some(70), None),
                ("set aside {card 71}".into(), Some(71), None),
                ("keep".into(), None, None),
            ],
        );
        assert_eq!(
            highlighted(&mulligan).into_iter().collect::<Vec<_>>(),
            [70, 71]
        );
        let mut secrets = RollSecrets::default();
        let fresh = || [0u8; dice::SECRET_LEN];
        let clicked = card_affordance(&mulligan, 71).unwrap();
        let bytes = action_bytes(clicked, &mut secrets, &fresh).unwrap();
        assert_eq!(
            picked(&bytes),
            agni_plugin_sdk::prompt::Pick {
                prompt: 3,
                option: 1
            }
        );
        assert_eq!(
            action_bytes(&mulligan.affordances[2], &mut secrets, &fresh).unwrap(),
            pick_bytes(3, 2)
        );
        assert!(card_affordance(&mulligan, 72).is_none());
        assert_eq!(
            ordered(&mulligan)
                .into_iter()
                .map(|affordance| affordance.label.as_str())
                .collect::<Vec<_>>(),
            [
                "set aside {card 70}",
                "set aside {card 71}",
                "keep",
                "free table"
            ]
        );
        assert_eq!(
            prompt_line(mulligan.prompt.as_ref().unwrap(), 0),
            "set aside up to 2 cards to redraw · up to 2 · 0 picked"
        );
        assert_eq!(
            prompt_line(mulligan.prompt.as_ref().unwrap(), 1),
            "{seat 0}: set aside up to 2 cards to redraw · up to 2 · 0 picked"
        );
        let group = m1_prompt(
            "move others to {zone 9} too?",
            1,
            vec![
                ("{card 90}".into(), Some(90), None),
                ("done".into(), None, None),
            ],
        );
        assert_eq!(highlighted(&group).into_iter().collect::<Vec<_>>(), [90]);
        let (seat, zone, card) = names();
        assert_eq!(
            expand(&group.affordances[0].label, &seat, &zone, &card),
            "card-90"
        );
        assert_eq!(
            expand(&group.status[1], &seat, &zone, &card),
            "move others to zone-9 too?"
        );
        let location = m1_prompt(
            "where does {card 70} enter?",
            1,
            vec![
                ("your base".into(), None, None),
                ("{zone 9}".into(), None, None),
                ("cancel".into(), None, Some("x")),
            ],
        );
        assert!(highlighted(&location).is_empty());
        let pressed = |key: KeyCode| key == KeyCode::KeyX;
        let escaped = pressed_affordance(&location, &pressed).unwrap();
        assert_eq!(
            picked(&escaped.data),
            agni_plugin_sdk::prompt::Pick {
                prompt: 3,
                option: 2
            }
        );
        assert_eq!(
            ordered(&location)
                .into_iter()
                .map(|affordance| affordance.label.as_str())
                .collect::<Vec<_>>(),
            ["your base", "{zone 9}", "cancel", "free table"]
        );
    }

    #[test]
    fn the_strip_puts_prompt_options_first_then_pass_and_end_turn_then_the_rest() {
        let mut view = PluginView {
            affordances: vec![
                offer("Lillia: play a Sprite", None, 5),
                offer("end turn", Some("space"), 2),
                offer("pass", Some("w"), 9),
                Affordance {
                    kind: AffordanceKind::Reveal { roll: 1 },
                    ..offer("reveal", None, 7)
                },
                Affordance {
                    card: Some(3),
                    ..offer("{card 3}", None, 10)
                },
                offer("cancel", Some("x"), 11),
            ],
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
        let labels = |view: &PluginView| -> Vec<String> {
            ordered(view)
                .into_iter()
                .map(|affordance| affordance.label.clone())
                .collect()
        };
        assert_eq!(
            labels(&view),
            [
                "Lillia: play a Sprite",
                "{card 3}",
                "cancel",
                "end turn",
                "pass"
            ]
        );
        view.prompt = None;
        assert_eq!(
            labels(&view),
            [
                "end turn",
                "pass",
                "Lillia: play a Sprite",
                "{card 3}",
                "cancel"
            ]
        );
    }

    #[test]
    fn a_prompt_summary_reads_as_a_line_for_the_asked_seat_and_the_others() {
        let mut summary = PromptSummary {
            seat: 1,
            why: "set aside up to 2 cards".into(),
            min: 0,
            max: 2,
            picked: 1,
            optional: false,
        };
        assert_eq!(
            prompt_line(&summary, 1),
            "set aside up to 2 cards · up to 2 · 1 picked"
        );
        assert_eq!(
            prompt_line(&summary, 0),
            "{seat 1}: set aside up to 2 cards · up to 2 · 1 picked"
        );
        summary.why = "choose".into();
        summary.min = 1;
        summary.max = 1;
        summary.optional = true;
        assert_eq!(prompt_line(&summary, 1), "choose · pick one · optional");
        summary.min = 2;
        summary.max = 3;
        summary.optional = false;
        assert_eq!(prompt_line(&summary, 1), "choose · 2 to 3 · 1 picked");
        summary.min = 2;
        summary.max = 2;
        assert_eq!(prompt_line(&summary, 1), "choose · pick 2 · 1 picked");
    }

    #[test]
    fn a_rim_hugs_the_card_just_outside_its_edge() {
        let corners = rim_corners(dim::CARD_W, dim::CARD_H);
        assert!(corners.iter().all(|corner| corner.y > 0.0));
        assert!(corners
            .iter()
            .all(|corner| corner.x.abs() > dim::CARD_W / 2.0));
        assert!(corners
            .iter()
            .all(|corner| corner.z.abs() > dim::CARD_H / 2.0));
        let wide = rim_corners(dim::CARD_H, dim::CARD_W);
        assert!(wide[1].x > corners[1].x);
    }

    #[test]
    fn a_hotkey_press_picks_the_matching_enabled_affordance() {
        let mut view = PluginView {
            affordances: vec![
                offer("end turn", Some("space"), 2),
                offer("pass", Some("w"), 4),
                offer("mystery", Some("f13"), 9),
            ],
            ..Default::default()
        };
        let pressed = |key: KeyCode| key == KeyCode::KeyW;
        assert_eq!(pressed_affordance(&view, &pressed).unwrap().data[0], 4);
        view.affordances[1].enabled = false;
        assert!(pressed_affordance(&view, &pressed).is_none());
        assert_eq!(key_of("space"), Some(KeyCode::Space));
        assert_eq!(key_of("x"), Some(KeyCode::KeyX));
        assert_eq!(key_of("3"), Some(KeyCode::Digit3));
        assert_eq!(key_of("f13"), None);
        for key in ["i", "h", "p", "r", "l", "c", "k", "d", "t", "e"] {
            assert_eq!(key_of(key), None, "{key} is kai's own key");
        }
    }

    #[test]
    fn a_key_the_plugin_fires_is_claimed_and_kais_handler_skips_it() {
        let view = PluginView {
            affordances: vec![
                offer("end turn", Some("space"), 2),
                offer("pass", Some("w"), 4),
                offer("cancel", Some("x"), 5),
            ],
            ..Default::default()
        };
        let pressed = |key: KeyCode| key == KeyCode::KeyX;
        let claimed = claims(&view, &pressed);
        assert!(claimed.taken(KeyCode::KeyX));
        assert!(!kai_key(&claimed, KeyCode::KeyX));
        assert!(kai_key(&claimed, KeyCode::Escape));
        assert!(kai_key(&claimed, KeyCode::Space));
        let nothing = claims(&view, &|key| key == KeyCode::KeyH);
        assert!(
            nothing.0.is_empty(),
            "a key the plugin does not tag stays kai's"
        );
        let mut quiet = view.clone();
        quiet.affordances[2].enabled = false;
        assert!(
            claims(&quiet, &pressed).0.is_empty(),
            "a disabled affordance claims nothing, so kai's escape ladder runs"
        );
        assert!(KAI_KEYS
            .iter()
            .all(|key| !matches!(key, KeyCode::Space | KeyCode::KeyW | KeyCode::KeyX)));
        assert_eq!(
            key_of("enter"),
            None,
            "a plugin cannot claim kai's default-action key"
        );
        assert_eq!(key_of("space"), Some(KeyCode::Space));
        assert_eq!(key_of("w"), Some(KeyCode::KeyW));
        assert!(plugin_takes(&view, &pressed, true), "shift+x is still x");
        let space = |key: KeyCode| key == KeyCode::Space;
        assert!(plugin_takes(&view, &space, false));
        assert!(
            !plugin_takes(&view, &space, true),
            "shift+space is kai's pass-through, never the plugin's end turn"
        );
    }

    #[test]
    fn the_mulligan_and_the_group_move_are_read_from_the_presenters_own_words() {
        let mut view = PluginView {
            prompt: Some(PromptSummary {
                seat: 0,
                why: "set aside up to 2 cards to redraw".into(),
                min: 0,
                max: 2,
                picked: 0,
                optional: false,
            }),
            ..Default::default()
        };
        assert_eq!(prompt_kind(&view, 0), Some(PromptKind::Mulligan));
        assert_eq!(
            prompt_kind(&view, 1),
            None,
            "the other seat's mulligan is a wait"
        );
        assert_eq!(primary_label(&view, 0), None);
        view.prompt.as_mut().unwrap().picked = 2;
        assert_eq!(
            primary_label(&view, 0).as_deref(),
            Some("keep · 2 set aside")
        );
        view.prompt.as_mut().unwrap().why = "move others to {zone 10} too?".into();
        view.prompt.as_mut().unwrap().picked = 1;
        assert_eq!(
            prompt_kind(&view, 0),
            Some(PromptKind::GroupMove { zone: 10 })
        );
        assert_eq!(primary_label(&view, 0).as_deref(), Some("move 2"));
        view.prompt.as_mut().unwrap().why = "where does {card 70} enter?".into();
        assert_eq!(prompt_kind(&view, 0), None);
        assert_eq!(primary_label(&view, 0), None);
        view.affordances = vec![
            Affordance {
                card: Some(90),
                ..offer("{card 90}", None, 1)
            },
            Affordance {
                card: Some(91),
                enabled: false,
                ..offer("{card 91}", None, 2)
            },
            offer("done", None, 3),
        ];
        assert_eq!(group_move_options(&view), [0]);
    }

    #[test]
    fn a_pay_or_let_confirm_words_its_yes_and_no_as_the_presenter_answers_them() {
        let mut view = PluginView {
            prompt: Some(PromptSummary {
                seat: 1,
                why: "pay 2 energy to keep {card 71}?".into(),
                min: 1,
                max: 1,
                picked: 0,
                optional: false,
            }),
            affordances: vec![offer("yes", None, 1), offer("no", Some("x"), 2)],
            ..Default::default()
        };
        assert_eq!(
            prompt_kind(&view, 1),
            Some(PromptKind::PayOrLet {
                cost: "2 energy".into()
            })
        );
        assert_eq!(prompt_kind(&view, 0), None, "the payer is asked, not me");
        assert_eq!(primary_label(&view, 1), None, "yes and no stay chips");
        assert_eq!(answer_label(&view, 1, 0), "pay 2 energy");
        assert_eq!(answer_label(&view, 1, 1), LET_IT_RESOLVE);
        assert_eq!(answer_wording(&view, 0, 0), None);
        assert_eq!(
            strip_state(&view, 0),
            Some(StripState::Waiting {
                seat: Some(1),
                text: "waiting for {seat 1} · pay 2 energy to keep {card 71}?".into(),
            }),
            "the other seat reads the ransom through the same path"
        );
        let mine = strip_state(&view, 1).unwrap();
        assert!(
            matches!(mine, StripState::Prompt { ref question, ref chips, cancel: Some(1), .. } if question == "pay 2 energy to keep {card 71}?" && chips == &[0])
        );
        view.affordances.remove(0);
        assert_eq!(
            answer_label(&view, 1, 0),
            LET_IT_RESOLVE,
            "unaffordable: only no"
        );
        view.prompt.as_mut().unwrap().why =
            "pay 1 any power for the {card 3} trigger · {card 4}?".into();
        assert_eq!(
            prompt_kind(&view, 1),
            None,
            "an optional cost keeps yes and no"
        );
        assert_eq!(answer_label(&view, 1, 0), "no");
    }
}

#[cfg(test)]
mod hidden_action_tests {
    use super::*;
    use crate::table::highlight::Rims;
    use agni_sim::wire::{Legal, LegalKind};

    fn view_with(rows: Vec<Legal>) -> PluginView {
        PluginView {
            legal: rows,
            ..Default::default()
        }
    }

    fn offering(card: u32, zones: &[u16]) -> Rims {
        let mut rims = Rims::default();
        rims.hides.insert(card, zones.to_vec());
        rims
    }

    #[test]
    fn a_hidden_card_in_hand_offers_a_hide_and_nothing_else() {
        let bf1 = agni_riftbound::ZONE_BATTLEFIELD_FIRST;
        let view = view_with(vec![Legal {
            card: 7,
            kinds: vec![LegalKind::Play { accelerate: false }, LegalKind::Hide],
            zones: Vec::new(),
            hidden: vec![bf1],
        }]);
        assert_eq!(
            hidden_actions(&view, &offering(7, &[bf1]), 7, true, false),
            [HiddenAction::Hide { zone: bf1 }]
        );
        assert!(
            hidden_actions(&view, &offering(7, &[bf1]), 7, false, false).is_empty(),
            "the other seat is offered nothing about my card"
        );
    }

    #[test]
    fn every_legal_hide_destination_is_a_visible_action() {
        let bf1 = agni_riftbound::ZONE_BATTLEFIELD_FIRST;
        let bf2 = bf1 + 1;
        let view = view_with(vec![Legal {
            card: 7,
            kinds: vec![LegalKind::Hide],
            zones: Vec::new(),
            hidden: vec![bf1, bf2],
        }]);
        assert_eq!(
            hidden_actions(&view, &offering(7, &[bf1, bf2]), 7, true, false),
            [
                HiddenAction::Hide { zone: bf1 },
                HiddenAction::Hide { zone: bf2 }
            ]
        );
    }

    #[test]
    fn a_facedown_card_offers_the_reaction_and_the_voluntary_reveal() {
        let view = view_with(vec![Legal {
            card: 7,
            kinds: vec![LegalKind::React],
            zones: vec![agni_riftbound::ZONE_CHAIN],
            hidden: Vec::new(),
        }]);
        assert_eq!(
            hidden_actions(&view, &Rims::default(), 7, true, true),
            [HiddenAction::PlayFromFacedown, HiddenAction::Reveal],
            "737.6 · the grant and the voluntary reveal, in that order"
        );
        let quiet = view_with(Vec::new());
        assert_eq!(
            hidden_actions(&quiet, &Rims::default(), 7, true, true),
            [HiddenAction::Reveal],
            "737.1.b · on the hiding turn only the reveal is on offer"
        );
    }

    #[test]
    fn a_card_shown_while_it_still_lies_face_down_keeps_its_offers() {
        let view = view_with(vec![Legal {
            card: 7,
            kinds: vec![LegalKind::React],
            zones: Vec::new(),
            hidden: Vec::new(),
        }]);
        assert_eq!(
            hidden_actions(&view, &Rims::default(), 7, true, true),
            [HiddenAction::PlayFromFacedown, HiddenAction::Reveal],
            "a voluntary reveal, or a cancelled play from facedown, leaves the card facedown \
             with the strip still offering the play"
        );
    }

    #[test]
    fn a_greyed_card_under_the_cursor_says_it_cannot_be_played_and_others_say_nothing() {
        let mut rims = Rims::default();
        rims.grey.insert(7);
        assert_eq!(
            grey_hint(&rims, 7, "Cleave").as_deref(),
            Some("Cleave · cannot be played right now")
        );
        assert_eq!(grey_hint(&rims, 8, "Vi"), None);
        assert_eq!(grey_hint(&Rims::default(), 7, "Cleave"), None);
    }

    #[test]
    fn every_offer_names_its_battlefield_and_its_price() {
        let bf1 = agni_riftbound::ZONE_BATTLEFIELD_FIRST;
        let zone_name = |zone: u16| format!("battlefield {zone}");
        assert_eq!(
            HiddenAction::Hide { zone: bf1 }.label(&zone_name),
            format!("hide at battlefield {bf1} · 1 rune"),
            "737.1.a · the [A] price is on the chip, not a surprise in the rune pool"
        );
        assert_eq!(
            HiddenAction::PlayFromFacedown.label(&zone_name),
            "play from hidden"
        );
        assert_eq!(HiddenAction::Reveal.label(&zone_name), "reveal");
    }
}

#[cfg(test)]
mod strip_tests {
    use super::*;
    use serde_bytes::ByteBuf;

    fn offer(label: &str, hotkey: Option<&str>, card: Option<u32>) -> Affordance {
        Affordance {
            label: label.into(),
            hotkey: hotkey.map(str::to_string),
            enabled: true,
            kind: AffordanceKind::Plain,
            data: ByteBuf::from(vec![1]),
            card,
        }
    }

    fn summary(seat: u8, why: &str, min: u8, max: u8, picked: u8, optional: bool) -> PromptSummary {
        PromptSummary {
            seat,
            why: why.into(),
            min,
            max,
            picked,
            optional,
        }
    }

    #[test]
    fn the_primary_is_never_one_of_the_strips_chips() {
        let mut mulligan = PluginView {
            status: vec![
                "turn 1 · {seat 0} · setup · rules enforced".into(),
                "set aside up to 2 cards to redraw".into(),
            ],
            affordances: vec![
                offer("set aside {card 70}", None, Some(70)),
                offer("set aside {card 71}", None, Some(71)),
                offer("keep", None, None),
                offer("free table", None, None),
            ],
            ..Default::default()
        };
        mulligan.prompt = Some(summary(
            0,
            "set aside up to 2 cards to redraw",
            0,
            2,
            0,
            true,
        ));
        let primary = super::super::primary::primary_of(&mulligan).unwrap();
        assert_eq!(primary.affordance, Some(2));
        let chips = strip_chips(&mulligan);
        assert!(!chips.contains(&2), "keep is the primary, not a chip");
        assert!(
            !chips.contains(&3),
            "the free-table offer lives in the menu"
        );
        assert!(
            chips.is_empty(),
            "card options are answered on the cards: {chips:?}"
        );

        let location = PluginView {
            status: vec!["turn 1 · {seat 0} · action phase · rules enforced".into()],
            affordances: vec![
                offer("your base", None, None),
                offer("{zone 9}", None, None),
                offer("cancel", Some("x"), None),
                offer("free table", None, None),
            ],
            prompt: Some(summary(0, "where does {card 70} enter?", 0, 1, 0, false)),
            ..Default::default()
        };
        assert_eq!(strip_chips(&location), [0, 1]);
        assert_eq!(cancel_index(&location), Some(2));
        match strip_state(&location, 0).unwrap() {
            StripState::Prompt {
                question,
                count,
                cancel,
                chips,
            } => {
                assert_eq!(question, "where does {card 70} enter?");
                assert_eq!(count, "1 of 1");
                assert_eq!(cancel, Some(2));
                assert_eq!(chips, [0, 1]);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            strip_state(&location, 1),
            Some(StripState::Waiting {
                seat: Some(0),
                text: "waiting for {seat 0} · where does {card 70} enter?".into()
            })
        );

        let play = PluginView {
            status: vec!["turn 2 · {seat 0} · action phase · rules enforced".into()],
            affordances: vec![
                offer("end turn", Some("space"), None),
                offer("Lillia: play a Sprite", None, Some(3)),
                offer("free table", None, None),
            ],
            ..Default::default()
        };
        let primary = super::super::primary::primary_of(&play).unwrap();
        assert_eq!(primary.affordance, Some(0));
        assert!(
            strip_chips(&play).is_empty(),
            "an activation is a chip on its card, never on the strip"
        );
        assert_eq!(
            strip_state(&play, 0),
            None,
            "nothing pending: the strip collapses"
        );
    }

    #[test]
    fn hidden_affordances_stay_off_the_strip_and_the_primary_field_wins() {
        let mut told = PluginView {
            status: vec!["turn 2 · {seat 0} · action phase · rules enforced".into()],
            affordances: vec![
                offer("end turn", Some("space"), None),
                offer("free table", None, None),
                offer("concede", None, None),
                offer("withdraw free table", None, None),
            ],
            hidden: vec![2, 3],
            primary: Some(0),
            ..Default::default()
        };
        assert!(
            strip_chips(&told).is_empty(),
            "the menu verbs are not chips"
        );
        assert_eq!(told_primary(&told), Some(0));
        assert_eq!(
            super::super::primary::primary_of(&told).unwrap().affordance,
            told_primary(&told),
            "the presenter and the derivation name the same primary"
        );
        assert!(menu_only(&told, 1) && menu_only(&told, 2) && menu_only(&told, 3));
        assert!(!menu_only(&told, 0));
        let listed: Vec<&str> = ordered(&told)
            .iter()
            .map(|affordance| affordance.label.as_str())
            .collect();
        assert_eq!(listed, ["end turn", "free table"]);
        assert!(
            !super::super::primary::plays_remain(&told, 0),
            "a hidden concede is not a play that keeps the primary amber"
        );
        let tools = Tools::from(&told);
        assert!(!tools.free);

        told.primary = Some(2);
        assert_eq!(told_primary(&told), None, "a hidden primary is ignored");
        told.primary = Some(9);
        assert_eq!(told_primary(&told), None, "an out-of-range primary too");
        told.primary = None;
        assert_eq!(told_primary(&told), None);

        let mut waiting = PluginView {
            status: vec!["turn 2 · {seat 1} · action phase · rules enforced".into()],
            waiting: Some(agni_sim::wire::Waiting {
                seat: Some(1),
                what: "their action phase".into(),
            }),
            ..Default::default()
        };
        assert_eq!(
            strip_state(&waiting, 0),
            Some(StripState::Waiting {
                seat: Some(1),
                text: "waiting for {seat 1} · their action phase".into()
            })
        );
        waiting.waiting = Some(agni_sim::wire::Waiting {
            seat: Some(1),
            what: String::new(),
        });
        assert_eq!(
            strip_state(&waiting, 0),
            Some(StripState::Waiting {
                seat: Some(1),
                text: "waiting for {seat 1}".into()
            })
        );
        waiting.waiting = None;
        waiting.status.push("waiting for every seat to roll".into());
        assert_eq!(
            strip_state(&waiting, 0),
            Some(StripState::Waiting {
                seat: None,
                text: "waiting for every seat to roll".into()
            }),
            "without the field the parsed line is the source"
        );
    }

    #[test]
    fn the_strip_has_a_response_window_a_waiting_state_and_an_opening() {
        let mut response = PluginView {
            status: vec!["turn 2 · {seat 1} · action phase · rules enforced".into()],
            affordances: vec![offer("pass", Some("w"), None)],
            ..Default::default()
        };
        response.chain = vec![agni_sim::wire::ChainRow {
            item: 4,
            card: Some(40),
            seat: 1,
        }];
        assert_eq!(
            strip_state(&response, 0),
            Some(StripState::Response {
                text: "{seat 1} played {card 40} — respond or pass".into()
            })
        );
        let waiting = PluginView {
            status: vec![
                "turn 2 · {seat 1} · action phase · rules enforced".into(),
                "waiting for {seat 1}: their action phase".into(),
            ],
            ..Default::default()
        };
        assert_eq!(
            strip_state(&waiting, 0),
            Some(StripState::Waiting {
                seat: Some(1),
                text: "waiting for {seat 1} · their action phase".into()
            })
        );
        let opening = PluginView {
            status: vec![
                "roll for first player".into(),
                "{seat 0}: 4 · {seat 1}: 2".into(),
                "you won the roll — who goes first?".into(),
                "mode: rules enforced".into(),
                "every deck must be dealt before the start: the first turn draws and channels at once".into(),
            ],
            affordances: vec![
                offer("go first", None, None),
                offer("let {seat 1} go first", None, None),
                offer("switch to free table", None, None),
            ],
            ..Default::default()
        };
        match strip_state(&opening, 0).unwrap() {
            StripState::Opening { lines, chips } => {
                assert_eq!(
                    lines,
                    [
                        "{seat 0}: 4 · {seat 1}: 2",
                        "you won the roll — who goes first?"
                    ]
                );
                assert_eq!(
                    chips,
                    [0, 1],
                    "the mode switch is a table-menu row, never a chip beside go first"
                );
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            count_chip(&summary(0, "", 0, 2, 1, true)),
            "1 picked · up to 2 · optional"
        );
        assert_eq!(count_chip(&summary(0, "", 2, 2, 0, false)), "0 of 2");
        assert_eq!(
            count_chip(&summary(0, "", 1, 3, 1, false)),
            "1 picked · 1 to 3"
        );
    }

    #[test]
    fn options_whose_cards_have_no_face_on_the_felt_go_to_the_tray() {
        let view = PluginView {
            affordances: vec![
                offer("{card 5}", None, Some(5)),
                offer("{card 6}", None, Some(6)),
                offer("done", None, None),
            ],
            prompt: Some(summary(0, "look at the top 2", 0, 1, 0, false)),
            ..Default::default()
        };
        let shown: BTreeSet<u32> = [6].into_iter().collect();
        assert_eq!(faceless_options(&view, &shown), [0]);
        assert!(faceless_options(&view, &[5, 6].into_iter().collect()).is_empty());
        let mut tray = TrayItems::default();
        assert!(tray.is_empty());
        tray.items.push(TrayItem {
            label: "Rockfall Path".into(),
            texture: None,
            frame: None,
            wide: true,
        });
        tray.picked = Some(0);
        tray.clear();
        assert!(tray.is_empty() && tray.picked.is_none());
    }

    #[test]
    fn large_prompt_options_filter_without_inventing_a_choice() {
        let mut view = PluginView {
            prompt: Some(summary(0, "name a tag", 1, 1, 0, false)),
            affordances: (0..SELECTOR_MIN_OPTIONS)
                .map(|index| offer(&format!("tag {index}"), None, None))
                .collect(),
            ..Default::default()
        };
        assert_eq!(
            selector_options(&view, 0),
            (0..SELECTOR_MIN_OPTIONS).collect::<Vec<_>>()
        );
        assert!(selector_matches("Kennen", "ken"));
        assert!(!selector_matches("Kennen", "poro"));
        assert!(selector_options(&view, 1).is_empty());
        view.affordances.truncate(SELECTOR_MIN_OPTIONS - 1);
        assert!(selector_options(&view, 0).is_empty());
    }

    #[test]
    fn large_card_prompts_keep_only_the_enabled_offered_cards() {
        let mut view = PluginView {
            prompt: Some(summary(0, "choose a target", 1, 1, 0, false)),
            affordances: (0..=SELECTOR_MIN_OPTIONS)
                .map(|index| offer(&format!("{{card {index}}}"), None, Some(index as u32)))
                .collect(),
            ..Default::default()
        };
        view.affordances[3].enabled = false;
        let options = selector_options(&view, 0);
        assert_eq!(options.len(), SELECTOR_MIN_OPTIONS);
        assert!(!options.contains(&3));
    }

    #[test]
    fn card_choices_search_public_catalog_tags_and_rules_without_hidden_faces() {
        use agni_importers::riftbound::catalog::{CardKind, CatalogCard};
        use agni_sim::wire::{ZoneDecl, ZoneVisibility};

        let catalog = crate::deck::catalog::Catalog::from_cards(
            vec![CatalogCard {
                name: "Lonely Poro".into(),
                riftbound_id: "sfd-036-221".into(),
                kind: CardKind::Unit,
                tags: vec!["Poro".into()],
                text: Some("growl at dawn".into()),
                ..Default::default()
            }],
            crate::deck::catalog::Source::Store(0),
        );
        let mut table = Table::new();
        let shown = table.add_face(PlayerId(0), Zone::Board, CardFace::named("Lonely Poro"));
        let view = PluginView {
            affordances: vec![offer("{card 1}", None, Some(shown.0))],
            ..Default::default()
        };
        let terms = selector_search(&view, 0, &table, &Mirror::default(), PlayerId(0), &catalog);
        assert!(selector_matches(&terms, "poro"));
        assert!(selector_matches(&terms, "growl"));

        let hidden = table.add_face(PlayerId(1), Zone::Plugin(9), CardFace::named("Hidden Poro"));
        let hidden_view = PluginView {
            affordances: vec![offer("{card 2}", None, Some(hidden.0))],
            ..Default::default()
        };
        let mirror = Mirror {
            view: TableView {
                zones: vec![ZoneDecl {
                    id: 9,
                    visibility: ZoneVisibility::All,
                    ..Default::default()
                }],
                ..Default::default()
            },
            ..Default::default()
        };
        let hidden_terms = selector_search(&hidden_view, 0, &table, &mirror, PlayerId(0), &catalog);
        assert!(selector_matches(&hidden_terms, FACE_DOWN));
        assert!(!hidden_terms.contains("hidden poro"));
    }

    #[test]
    fn yes_no_prompts_take_one_and_two_not_the_plugin_cancel_key() {
        let view = PluginView {
            prompt: Some(summary(0, "pay?", 1, 1, 0, false)),
            affordances: vec![offer("yes", Some("1"), None), offer("no", Some("x"), None)],
            ..Default::default()
        };
        assert_eq!(pressed_index(&view, &|key| key == KeyCode::Digit1), Some(0));
        assert_eq!(pressed_index(&view, &|key| key == KeyCode::Digit2), Some(1));
        assert_eq!(pressed_index(&view, &|key| key == KeyCode::KeyX), None);
        assert_eq!(effective_hotkey(&view, 0), Some(KeyCode::Digit1));
        assert_eq!(effective_hotkey(&view, 1), Some(KeyCode::Digit2));
        assert_eq!(cancel_index(&view), None);
        let claimed = claims(&view, &|key| key == KeyCode::Digit2);
        assert!(claimed.taken(KeyCode::Digit2));
        assert!(!claimed.taken(KeyCode::KeyX));
    }
}
