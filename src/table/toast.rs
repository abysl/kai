use super::*;

pub const STRIP_SECS: f64 = 2.5;
pub const CARD_SECS: f64 = 1.5;
pub const RIM_SECS: f64 = 0.6;
pub const SHAKE_SECS: f32 = 0.12;
pub const SHAKE_AMPLITUDE: f32 = 0.12;
pub const ANCHOR_WINDOW_SECS: f64 = 2.0;
pub const LOG_KEEP: usize = 5;
const CARD_GAP: f32 = 8.0;
const RIM_WIDTH: f32 = 3.0;

pub const AMBER: egui::Color32 = egui::Color32::from_rgb(240, 178, 50);
pub const DANGER: egui::Color32 = egui::Color32::from_rgb(255, 107, 107);

pub fn amber_fill() -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(240, 178, 50, 51)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Refusal {
    pub text: String,
    pub card: Option<u32>,
    pub at: f64,
}

impl Refusal {
    pub fn age(&self, now: f64) -> f64 {
        now - self.at
    }
}

#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct Refusals {
    pub current: Option<Refusal>,
    pub log: Vec<String>,
}

impl Refusals {
    pub fn push(&mut self, text: String, card: Option<u32>, now: f64) {
        self.log.push(text.clone());
        if self.log.len() > LOG_KEEP {
            let extra = self.log.len() - LOG_KEEP;
            self.log.drain(..extra);
        }
        self.current = Some(Refusal {
            text,
            card,
            at: now,
        });
    }

    pub fn on_strip(&self, now: f64) -> Option<&Refusal> {
        self.current
            .as_ref()
            .filter(|refusal| refusal.age(now) < STRIP_SECS)
    }

    pub fn at_card(&self, now: f64) -> Option<(u32, f64)> {
        let refusal = self.current.as_ref()?;
        let card = refusal.card?;
        let age = refusal.age(now);
        (age < CARD_SECS).then_some((card, age))
    }

    pub fn shaking(&self, card: u32, now: f64) -> Option<f32> {
        let (refused, age) = self.at_card(now)?;
        (refused == card && age < f64::from(SHAKE_SECS)).then(|| shake(age as f32))
    }
}

#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct LastIntent {
    pub card: Option<u32>,
    pub sent_at: f64,
}

impl LastIntent {
    pub fn note(&mut self, card: Option<u32>, now: f64) {
        self.card = card;
        self.sent_at = now;
    }

    pub fn anchor(&self, now: f64) -> Option<u32> {
        let age = now - self.sent_at;
        self.card
            .filter(|_| (0.0..=ANCHOR_WINDOW_SECS).contains(&age))
    }
}

pub fn shake(age: f32) -> f32 {
    if !(0.0..SHAKE_SECS).contains(&age) {
        return 0.0;
    }
    let phase = age / SHAKE_SECS;
    (phase * 3.0 * std::f32::consts::PI).sin() * SHAKE_AMPLITUDE * (1.0 - phase)
}

pub fn rim_alpha(age: f64) -> u8 {
    if age < 0.0 || age >= RIM_SECS {
        return 0;
    }
    (255.0 * (1.0 - age / RIM_SECS)) as u8
}

pub fn note_intents(
    time: Res<Time>,
    mut intent: ResMut<LastIntent>,
    mut drops: MessageReader<CardDropped>,
    mut exhausts: MessageReader<ExhaustToggled>,
    mut reveals: MessageReader<RevealRequested>,
    mut actions: MessageReader<plugin_ui::PluginActionRequested>,
) {
    let now = time.elapsed_secs_f64();
    let mut last: Option<Option<u32>> = None;
    for drop in drops.read() {
        last = Some(Some(drop.card.0));
    }
    for toggle in exhausts.read() {
        last = Some(Some(toggle.card.0));
    }
    for reveal in reveals.read() {
        last = Some(Some(reveal.0 .0));
    }
    for action in actions.read() {
        last = Some(action.1);
    }
    if let Some(card) = last {
        intent.note(card, now);
    }
}

pub fn collect_refusals(
    time: Res<Time>,
    mut info: ResMut<SessionInfo>,
    intent: Res<LastIntent>,
    mut refusals: ResMut<Refusals>,
    table: Res<GameTable>,
    mirror: Res<Mirror>,
    my_seat: Res<MySeat>,
    colors: Res<colors::SeatColors>,
) {
    if info.notices.is_empty() {
        return;
    }
    let now = time.elapsed_secs_f64();
    let anchor = intent.anchor(now);
    let notices = std::mem::take(&mut info.notices);
    let seat_name =
        |seat: u8| colors::seat_label(&info.roster, &colors, my_seat.0, PlayerId(seat)).0;
    let zone_name = |zone: u16| super::plugin_ui::zone_label(&mirror.view.zones, zone);
    let card_name =
        |card: u32| super::plugin_ui::card_label(&table.0, &mirror.view, my_seat.0, card);
    for text in notices {
        let expanded = super::plugin_ui::expand(&text, &seat_name, &zone_name, &card_name);
        refusals.push(expanded, anchor, now);
    }
}

pub fn expire_refusals(time: Res<Time>, mut refusals: ResMut<Refusals>) {
    let now = time.elapsed_secs_f64();
    let stale = refusals
        .current
        .as_ref()
        .is_some_and(|refusal| refusal.age(now) >= STRIP_SECS);
    if stale {
        refusals.current = None;
    }
}

pub fn strip_row(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new()
        .fill(amber_fill())
        .stroke(egui::Stroke::new(1.0, AMBER))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).color(DANGER).strong());
        });
}

fn screen_corners(
    camera: &Camera,
    camera_transform: &GlobalTransform,
    transform: &GlobalTransform,
    landscape: bool,
) -> Vec<egui::Pos2> {
    let (width, height) = if landscape {
        (dim::CARD_H, dim::CARD_W)
    } else {
        (dim::CARD_W, dim::CARD_H)
    };
    plugin_ui::rim_corners(width, height)
        .iter()
        .filter_map(|corner| {
            camera
                .world_to_viewport(camera_transform, transform.transform_point(*corner))
                .ok()
                .map(|screen| egui::pos2(screen.x, screen.y))
        })
        .collect()
}

pub fn toast_top(card_top: f32, chip_row: Option<egui::Rect>) -> f32 {
    match chip_row {
        Some(row) => row.min.y.min(card_top),
        None => card_top,
    }
}

pub fn card_toast_ui(
    mut contexts: EguiContexts,
    time: Res<Time>,
    refusals: Res<Refusals>,
    menu: Res<crate::menu::Menu>,
    chip_row: Res<chips::ChipRow>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    cards: Query<(
        Entity,
        &CardView,
        &GlobalTransform,
        &ViewVisibility,
        Has<Landscape>,
    )>,
) -> Result {
    if !menu.at_table() {
        return Ok(());
    }
    let now = time.elapsed_secs_f64();
    let Some(refusal) = refusals.current.as_ref() else {
        return Ok(());
    };
    let Some((card, age)) = refusals.at_card(now) else {
        return Ok(());
    };
    let Ok((camera, camera_transform)) = camera.single() else {
        return Ok(());
    };
    let Some((entity, _, transform, _, landscape)) = cards
        .iter()
        .find(|(_, view, _, visibility, _)| view.0 .0 == card && visibility.get())
    else {
        return Ok(());
    };
    let corners = screen_corners(camera, camera_transform, transform, landscape);
    if corners.len() < 4 {
        return Ok(());
    }
    let context = contexts.ctx_mut()?.clone();
    let alpha = rim_alpha(age);
    if alpha > 0 {
        let painter = context.layer_painter(egui::LayerId::background());
        let mut ring = corners.clone();
        ring.push(corners[0]);
        painter.add(egui::Shape::line(
            ring,
            egui::Stroke::new(RIM_WIDTH, highlight::faded(AMBER, alpha)),
        ));
    }
    let top = corners
        .iter()
        .map(|corner| corner.y)
        .fold(f32::MAX, f32::min);
    let centre = corners.iter().map(|corner| corner.x).sum::<f32>() / corners.len() as f32;
    let top = toast_top(
        top,
        chip_row
            .0
            .filter(|row| row.entity == entity)
            .map(|row| row.rect),
    );
    egui::Area::new(egui::Id::new("refusal toast"))
        .fixed_pos(egui::pos2(centre, top - CARD_GAP))
        .pivot(egui::Align2::CENTER_BOTTOM)
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(&context, |ui| {
            ui.set_max_width(260.0);
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
            strip_row(ui, &refusal.text);
        });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_toast_stacks_above_the_chip_row_of_the_same_card() {
        let row = egui::Rect::from_min_max(egui::pos2(100.0, 400.0), egui::pos2(300.0, 428.0));
        assert_eq!(toast_top(436.0, Some(row)), 400.0);
        assert_eq!(toast_top(436.0, None), 436.0);
        assert_eq!(toast_top(380.0, Some(row)), 380.0);
    }

    #[test]
    fn a_notice_within_two_seconds_of_a_card_intent_anchors_to_that_card() {
        let mut intent = LastIntent::default();
        intent.note(Some(7), 10.0);
        assert_eq!(intent.anchor(11.5), Some(7));
        assert_eq!(intent.anchor(12.0), Some(7));
        assert_eq!(intent.anchor(12.5), None, "too late to be about that card");
        assert_eq!(
            intent.anchor(9.0),
            None,
            "a refusal from before the intent is not its answer"
        );
        intent.note(None, 13.0);
        assert_eq!(
            intent.anchor(13.1),
            None,
            "a chip with no card gives the strip the refusal"
        );
        let mut refusals = Refusals::default();
        refusals.push(
            "not enough runes to pay for that: 3 needed, 1 ready".into(),
            intent.anchor(13.1),
            13.1,
        );
        assert_eq!(refusals.current.as_ref().unwrap().card, None);
        intent.note(Some(9), 14.0);
        refusals.push(
            "units are played to your base, then attack from there".into(),
            intent.anchor(14.2),
            14.2,
        );
        assert_eq!(refusals.current.as_ref().unwrap().card, Some(9));
        let (card, age) = refusals.at_card(15.0).unwrap();
        assert_eq!(card, 9);
        assert!((age - 0.8).abs() < 1e-9);
        assert!(
            refusals.at_card(15.8).is_none(),
            "the card toast lasts a second and a half"
        );
        assert!(
            refusals.on_strip(16.6).is_some(),
            "the strip row lasts two and a half"
        );
        assert!(refusals.on_strip(16.8).is_none());
    }

    #[test]
    fn a_host_side_refusal_is_expanded_like_a_client_notice() {
        use bevy::ecs::system::RunSystemOnce;
        let mut world = World::new();
        world.init_resource::<Time>();
        world.init_resource::<LastIntent>();
        world.init_resource::<Refusals>();
        world.init_resource::<GameTable>();
        world.init_resource::<Mirror>();
        world.init_resource::<MySeat>();
        world.init_resource::<colors::SeatColors>();
        let mut info = SessionInfo::default();
        info.roster.push(agni_net::session::SeatInfo {
            seat: 1,
            name: "ada".into(),
            host: false,
            connected: true,
            color: 0,
            playmat: None,
        });
        info.notices
            .push("the game is over: {seat 1} won · free the table to keep playing".into());
        world.insert_resource(info);
        world
            .run_system_once(collect_refusals)
            .expect("the collector runs");
        let refusals = world.resource::<Refusals>();
        assert_eq!(
            refusals.current.as_ref().unwrap().text,
            "the game is over: ada won · free the table to keep playing"
        );
        assert!(world.resource::<SessionInfo>().notices.is_empty());
    }

    #[test]
    fn the_log_keeps_the_last_five_refusals() {
        let mut refusals = Refusals::default();
        for index in 0..7 {
            refusals.push(format!("refusal {index}"), None, index as f64);
        }
        assert_eq!(
            refusals.log,
            [
                "refusal 2",
                "refusal 3",
                "refusal 4",
                "refusal 5",
                "refusal 6"
            ]
        );
        assert_eq!(refusals.current.as_ref().unwrap().text, "refusal 6");
    }

    #[test]
    fn the_shake_is_short_and_settles_and_the_rim_fades() {
        assert_eq!(shake(-0.01), 0.0);
        assert_eq!(shake(SHAKE_SECS), 0.0);
        assert!(shake(0.02).abs() > 0.0);
        assert!(shake(0.02).abs() <= SHAKE_AMPLITUDE);
        assert!(shake(0.11).abs() < shake(0.02).abs(), "the shake dies down");
        let mut refusals = Refusals::default();
        refusals.push("no".into(), Some(4), 1.0);
        assert!(refusals.shaking(4, 1.05).is_some());
        assert!(
            refusals.shaking(5, 1.05).is_none(),
            "only the refused card shakes"
        );
        assert!(refusals.shaking(4, 1.2).is_none(), "and only for 120 ms");
        assert_eq!(rim_alpha(0.0), 255);
        assert!(rim_alpha(0.3) < 255 && rim_alpha(0.3) > 0);
        assert_eq!(rim_alpha(RIM_SECS), 0);
    }
}
