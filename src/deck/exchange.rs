use crate::deck::editor::{Draft, Origin};
use crate::deck::import::{self, ImportPanel, ImportedDeck};
use crate::menu::Menu;
use crate::net::TableGame;
use crate::os::clipboard;
use agni_importers::riftbound::{code_list, deck_code, link, text_list};
use agni_riftbound::ResolvedDeck;
use bevy::prelude::*;
use bevy_egui::egui;
use spirit_sdk::CiHash;

pub const EDITOR_SOURCE: &str = "editor";
pub const FILE_LIMIT: u64 = 64 * 1024;
pub const QR_MAX_SIDE: f32 = 300.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Share {
    TextList,
    DeckCode,
    CodeList,
    PiltoverLink,
    RiftAtlasLink,
}

impl Share {
    pub const ALL: [Share; 5] = [
        Share::TextList,
        Share::DeckCode,
        Share::CodeList,
        Share::PiltoverLink,
        Share::RiftAtlasLink,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Share::TextList => "copy text list",
            Share::DeckCode => "copy deck code",
            Share::CodeList => "copy code list",
            Share::PiltoverLink => "copy piltover link",
            Share::RiftAtlasLink => "copy rift atlas link",
        }
    }

    pub fn note(self) -> &'static str {
        match self {
            Share::TextList => {
                "the list format Rift Atlas and Piltover Archive import — names, not prints"
            }
            Share::DeckCode => {
                "the official deck code both sites read — prints and sideboard, reprints fold to the base print"
            }
            Share::CodeList => "SET-NNN-COUNT tokens — no sideboard and no champion slot",
            Share::PiltoverLink => "the deck code as a piltoverarchive.com deckbuilder link",
            Share::RiftAtlasLink => "the deck code as a play.riftatlas.com link",
        }
    }
}

pub fn render(deck: &ResolvedDeck, what: Share) -> Result<String, String> {
    match what {
        Share::TextList => Ok(text_list::render(deck)),
        Share::DeckCode => deck_code::encode_deck(deck),
        Share::CodeList => code_list::render(deck),
        Share::PiltoverLink => deck_code::encode_deck(deck).map(|code| link::piltover_url(&code)),
        Share::RiftAtlasLink => deck_code::encode_deck(deck).map(|code| link::riftatlas_url(&code)),
    }
}

pub fn share_menu(
    ui: &mut egui::Ui,
    deck: &ResolvedDeck,
    qr: &mut Option<String>,
) -> Option<String> {
    let mut status = None;
    ui.menu_button("share", |ui| {
        for what in Share::ALL {
            match render(deck, what) {
                Ok(text) => {
                    if ui.button(what.label()).on_hover_text(what.note()).clicked() {
                        status = Some(clipboard::copy_status(&text));
                        ui.close();
                    }
                }
                Err(reason) => {
                    ui.add_enabled(false, egui::Button::new(what.label()))
                        .on_disabled_hover_text(reason);
                }
            }
        }
        match deck_code::encode_deck(deck) {
            Ok(code) => {
                if ui
                    .button("show QR")
                    .on_hover_text("the deck code as a QR — the other phone scans and pastes it")
                    .clicked()
                {
                    *qr = Some(code);
                    ui.close();
                }
            }
            Err(reason) => {
                ui.add_enabled(false, egui::Button::new("show QR"))
                    .on_disabled_hover_text(reason);
            }
        }
    });
    status
}

pub fn qr_side(screen: egui::Vec2) -> f32 {
    (screen.min_elem() - 64.0).clamp(120.0, QR_MAX_SIDE)
}

pub fn qr_modal(context: &egui::Context, code: &mut Option<String>) {
    let Some(text) = code.clone() else {
        return;
    };
    let id = egui::Id::new("deck-qr");
    let texture_id = id.with(&text);
    let held: Option<Result<egui::TextureHandle, String>> =
        context.data(|data| data.get_temp(texture_id));
    let texture = match held {
        Some(texture) => texture,
        None => {
            let texture = crate::os::qr::image(&text)
                .map(|image| context.load_texture("deck-qr", image, egui::TextureOptions::NEAREST));
            context.data_mut(|data| data.insert_temp(texture_id, texture.clone()));
            texture
        }
    };
    let side = qr_side(context.content_rect().size());
    let mut close = false;
    let response = egui::Modal::new(id).show(context, |ui| {
        ui.set_max_width(side + 16.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("scan to import").strong());
            match &texture {
                Ok(texture) => {
                    ui.add(egui::Image::new(texture).fit_to_exact_size(egui::vec2(side, side)));
                }
                Err(error) => {
                    ui.colored_label(
                        crate::theme::tokens(ui.ctx()).danger,
                        format!("qr render failed: {error}"),
                    );
                }
            }
            ui.label(
                egui::RichText::new(
                    "the other phone scans it and pastes the code into its deck box",
                )
                .weak()
                .small(),
            );
            ui.horizontal(|ui| {
                if ui.button("copy code").clicked() {
                    let _ = clipboard::set_text(&text);
                }
                close |= ui.button("close").clicked();
            });
        });
    });
    if close || response.should_close() {
        *code = None;
    }
}

pub fn save_label(draft: &Draft) -> String {
    let label = draft.label.trim();
    if !label.is_empty() {
        return label.to_string();
    }
    crate::deck::history::label(&ImportedDeck::Riftbound(draft.deck.clone()))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save(draft: &mut Draft, game: &str) -> Result<CiHash, String> {
    let dir = crate::os::paths::store_dir().ok_or("no spirit store on this device")?;
    save_in(&dir, draft, game)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_in(dir: &std::path::Path, draft: &mut Draft, game: &str) -> Result<CiHash, String> {
    use crate::deck::history::store;
    let label = save_label(draft);
    let deck = ImportedDeck::Riftbound(draft.deck.clone());
    let identity = agni_importers::deck::history::identity_of(
        &agni_importers::riftbound::snapshot::snapshot(&draft.deck),
    )?;
    let ci = match &draft.origin {
        Origin::Saved(old) if *old == identity => {
            let stored = store::rows_in(dir, game)
                .into_iter()
                .find(|row| row.ci == *old)
                .map(|row| row.label);
            if stored.as_deref() == Some(label.as_str()) {
                *old
            } else {
                store::rename_in(dir, game, *old, &label)?
            }
        }
        Origin::Saved(old) => store::replace_in(dir, game, *old, &deck, EDITOR_SOURCE, &label)?,
        Origin::New | Origin::Pool(_) | Origin::Import(_) | Origin::Seated => {
            store::remember_as_in(dir, &deck, EDITOR_SOURCE, &label)?
        }
    };
    draft.origin = Origin::Saved(ci);
    draft.dirty = false;
    Ok(ci)
}

#[cfg(target_arch = "wasm32")]
pub fn save(draft: &mut Draft, game: &str) -> Result<CiHash, String> {
    use crate::deck::history::store;
    let label = save_label(draft);
    let deck = ImportedDeck::Riftbound(draft.deck.clone());
    let identity = store::identity_of(&agni_importers::riftbound::snapshot::snapshot(&draft.deck))?;
    let ci = match &draft.origin {
        Origin::Saved(old) if *old == identity => {
            let stored = store::rows(game)
                .into_iter()
                .find(|row| row.ci == *old)
                .map(|row| row.label);
            if stored.as_deref() == Some(label.as_str()) {
                *old
            } else {
                store::rename(game, *old, &label)?
            }
        }
        Origin::Saved(old) => store::replace(game, *old, &deck, EDITOR_SOURCE, &label)?,
        Origin::New | Origin::Pool(_) | Origin::Import(_) | Origin::Seated => {
            store::remember_as(&deck, EDITOR_SOURCE, &label)?
        }
    };
    draft.origin = Origin::Saved(ci);
    draft.dirty = false;
    Ok(ci)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_as_copy(draft: &mut Draft) -> Result<CiHash, String> {
    let dir = crate::os::paths::store_dir().ok_or("no spirit store on this device")?;
    save_as_copy_in(&dir, draft)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_as_copy_in(dir: &std::path::Path, draft: &mut Draft) -> Result<CiHash, String> {
    let label = save_label(draft);
    let deck = ImportedDeck::Riftbound(draft.deck.clone());
    let ci = crate::deck::history::store::remember_as_in(dir, &deck, EDITOR_SOURCE, &label)?;
    draft.origin = Origin::Saved(ci);
    draft.dirty = false;
    Ok(ci)
}

#[cfg(target_arch = "wasm32")]
pub fn save_as_copy(draft: &mut Draft) -> Result<CiHash, String> {
    let label = save_label(draft);
    let deck = ImportedDeck::Riftbound(draft.deck.clone());
    let ci = crate::deck::history::store::remember_as(&deck, EDITOR_SOURCE, &label)?;
    draft.origin = Origin::Saved(ci);
    draft.dirty = false;
    Ok(ci)
}

pub fn dropped_text(path: &std::path::Path) -> Result<String, String> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if extension != "txt" && extension != "md" {
        return Err(format!(
            "{} is not a .txt or .md deck list",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("that file")
        ));
    }
    let size = std::fs::metadata(path)
        .map_err(|error| format!("{}: {error}", path.display()))?
        .len();
    if size > FILE_LIMIT {
        return Err(format!(
            "{} is {size} bytes — a deck list is under {} KiB",
            path.display(),
            FILE_LIMIT / 1024
        ));
    }
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(text_list::deck_block(&text)
        .map(str::to_string)
        .unwrap_or(text))
}

pub fn take_drop(
    panel: &mut ImportPanel,
    menu: &mut Menu,
    editor: &mut crate::deck::editor::DeckEditor,
    sheet: &mut crate::menu::editor::EditorSheet,
    text: String,
) {
    if stage_drop(panel, menu, editor, &text).is_some() {
        crate::menu::editor::import_text(sheet, panel, text);
    }
}

pub fn stage_drop(
    panel: &mut ImportPanel,
    menu: &mut Menu,
    editor: &mut crate::deck::editor::DeckEditor,
    text: &str,
) -> Option<TableGame> {
    let Some(game) = import::game_of_paste(text) else {
        panel.error = Some("that file holds neither a deck list nor a deck link".into());
        return None;
    };
    if menu.at_table() {
        return None;
    }
    if editor.draft.is_none() {
        editor.draft = Some(Draft::new(crate::deck::editor::NEW_LABEL, Origin::New));
    }
    if menu.screen != crate::menu::Screen::DeckEditor {
        crate::deck::editor::reopen(editor, menu);
    }
    Some(game)
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
pub fn file_drop(
    mut drops: MessageReader<bevy::window::FileDragAndDrop>,
    mut panel: ResMut<ImportPanel>,
    mut menu: ResMut<Menu>,
    mut editor: ResMut<crate::deck::editor::DeckEditor>,
    mut sheet: ResMut<crate::menu::editor::EditorSheet>,
) {
    for drop in drops.read() {
        let bevy::window::FileDragAndDrop::DroppedFile { path_buf, .. } = drop else {
            continue;
        };
        match dropped_text(path_buf) {
            Ok(text) => take_drop(&mut panel, &mut menu, &mut editor, &mut sheet, text),
            Err(error) => panel.error = Some(error),
        }
    }
}

pub fn pasted_text(events: &[egui::Event]) -> Option<String> {
    let command_v = events.iter().any(|event| {
        matches!(
            event,
            egui::Event::Key {
                key: egui::Key::V,
                pressed: true,
                modifiers,
                ..
            } if modifiers.command
        )
    });
    events.iter().find_map(|event| match event {
        egui::Event::Paste(text) if !text.trim().is_empty() => Some(text.clone()),
        egui::Event::Text(text) if command_v && !text.trim().is_empty() => Some(text.clone()),
        _ => None,
    })
}

pub fn unfocused_paste(context: &egui::Context) -> Option<String> {
    if context.egui_wants_keyboard_input() {
        return None;
    }
    context.input(|input| pasted_text(&input.events))
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
pub fn paste_when_unfocused(
    mut contexts: bevy_egui::EguiContexts,
    mut panel: ResMut<ImportPanel>,
    menu: Res<Menu>,
    mut sheet: ResMut<crate::menu::editor::EditorSheet>,
) -> Result {
    if menu.screen != crate::menu::Screen::DeckEditor {
        return Ok(());
    }
    let context = contexts.ctx_mut()?;
    if let Some(text) = unfocused_paste(context) {
        crate::menu::editor::import_text(&mut sheet, &mut panel, text);
    }
    Ok(())
}

pub fn register(app: &mut App) {
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
    app.add_systems(
        Update,
        file_drop.before(crate::deck::import::collect_results),
    )
    .add_systems(bevy_egui::EguiPrimaryContextPass, paste_when_unfocused);
    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    let _ = app;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::pool;
    use crate::menu::Screen;
    use agni_importers::riftbound::{parse_any, resolve};

    fn pool_decks() -> Vec<(String, ResolvedDeck)> {
        pool::decks()
            .into_iter()
            .map(|deck| {
                let resolved = pool::deck(&deck.slug).unwrap();
                (deck.slug, resolved)
            })
            .collect()
    }

    fn identity(deck: &ResolvedDeck) -> agni_deck::DeckIdentity {
        agni_importers::riftbound::snapshot::snapshot(deck).identity()
    }

    fn reresolve(
        text: &str,
        catalog: &[agni_importers::riftbound::catalog::CatalogCard],
    ) -> ResolvedDeck {
        let parsed = parse_any(text).unwrap();
        let mut lookup = agni_importers::riftbound::catalog::StaticCatalog::new(catalog.to_vec());
        let resolution = resolve::resolve(&parsed, &mut lookup).unwrap();
        assert!(
            resolution.unresolved.is_empty(),
            "{:?}",
            resolution.unresolved
        );
        resolution.deck
    }

    #[test]
    fn every_share_renders_for_every_pool_deck() {
        let decks = pool_decks();
        assert_eq!(decks.len(), 6);
        for (slug, deck) in &decks {
            for what in Share::ALL {
                let rendered =
                    render(deck, what).unwrap_or_else(|e| panic!("{slug} {what:?}: {e}"));
                assert!(!rendered.trim().is_empty(), "{slug} {what:?}");
            }
            let code = render(deck, Share::DeckCode).unwrap();
            assert_eq!(
                render(deck, Share::PiltoverLink).unwrap(),
                format!("{}{code}", link::PILTOVER_DECKBUILDER)
            );
            assert!(render(deck, Share::TextList)
                .unwrap()
                .starts_with(text_list::HEADER_LEGEND));
        }
    }

    #[test]
    fn the_code_list_refuses_a_sideboard_and_the_code_keeps_it() {
        let mut deck = pool::deck("lillia-jonnynick").unwrap();
        let spare = deck.main_deck[0].card.clone();
        deck.sideboard.push(agni_riftbound::DeckEntry {
            card: spare,
            count: 1,
        });
        assert_eq!(
            render(&deck, Share::CodeList).unwrap_err(),
            code_list::NO_SIDEBOARD
        );
        let code = render(&deck, Share::DeckCode).unwrap();
        let decoded = deck_code::decode(&code).unwrap();
        assert_eq!(decoded.sideboard.len(), 1);
    }

    #[test]
    fn the_deck_code_round_trips_through_parse_any_for_every_pool_deck() {
        let catalog = pool::cards();
        for (slug, deck) in pool_decks() {
            let code = render(&deck, Share::DeckCode).unwrap();
            let back = reresolve(&code, &catalog);
            assert_eq!(identity(&back), identity(&deck), "{slug}");
            assert_eq!(render(&back, Share::DeckCode).unwrap(), code, "{slug}");
            let list = render(&deck, Share::CodeList).unwrap();
            let mut folded = deck.clone();
            if let Some(champion) = folded.chosen_champion.take() {
                agni_deck::push(&mut folded.main_deck, champion, 1);
            }
            let back = reresolve(&list, &catalog);
            let mut back_folded = back.clone();
            if let Some(champion) = back_folded.chosen_champion.take() {
                agni_deck::push(&mut back_folded.main_deck, champion, 1);
            }
            assert_eq!(
                identity(&back_folded),
                identity(&folded),
                "{slug} code list"
            );
            let text = render(&deck, Share::TextList).unwrap();
            let back = reresolve(&text, &catalog);
            assert_eq!(identity(&back), identity(&deck), "{slug} text list");
        }
    }

    #[test]
    fn a_dropped_markdown_file_feeds_the_paste_box_with_its_deck_block() {
        let dir = std::env::temp_dir().join(format!("kai-drop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let md = dir.join("lillia.md");
        std::fs::write(&md, pool::FILES[0].1).unwrap();
        let text = dropped_text(&md).unwrap();
        assert!(!text.contains("## Deck"));
        assert!(parse_any(&text).is_ok());
        let txt = dir.join("plain.txt");
        std::fs::write(&txt, "Legend\n1 Lillia - Bashful Bloom\n").unwrap();
        assert_eq!(
            dropped_text(&txt).unwrap(),
            "Legend\n1 Lillia - Bashful Bloom\n"
        );
        let png = dir.join("art.png");
        std::fs::write(&png, b"not a list").unwrap();
        assert!(dropped_text(&png).unwrap_err().contains("art.png"));
        let big = dir.join("big.txt");
        std::fs::write(&big, "x".repeat(FILE_LIMIT as usize + 1)).unwrap();
        assert!(dropped_text(&big).unwrap_err().contains("KiB"));
        let mut panel = ImportPanel::default();
        let mut menu = Menu::default();
        let mut editor = crate::deck::editor::DeckEditor::default();
        menu.open_lobby(TableGame::Mtg);
        let game = stage_drop(&mut panel, &mut menu, &mut editor, &text);
        assert_eq!(game, Some(TableGame::Riftbound));
        assert_eq!(panel.error, None);
        assert_eq!(
            menu.screen,
            Screen::DeckEditor,
            "a drop opens the deck editor on a fresh draft"
        );
        assert_eq!(menu.decks_from, Screen::Lobby(TableGame::Mtg));
        assert_eq!(
            editor.draft.as_ref().map(|draft| draft.label.as_str()),
            Some(crate::deck::editor::NEW_LABEL)
        );
        assert_eq!(editor.opens, 1);
        let mut named = Draft::new("mine", Origin::New);
        named.rename("Mine");
        editor.draft = Some(named);
        stage_drop(&mut panel, &mut menu, &mut editor, &text);
        assert_eq!(
            editor.draft.as_ref().map(|draft| draft.label.as_str()),
            Some("Mine"),
            "an open draft stays open; the import loads into it on request"
        );
        assert_eq!(editor.opens, 1, "already on the editor screen, no reopen");
        let mut at_table = Menu {
            screen: Screen::Table,
            ..Menu::default()
        };
        assert_eq!(
            stage_drop(&mut panel, &mut at_table, &mut editor, &text),
            None
        );
        assert_eq!(
            at_table.screen,
            Screen::Table,
            "a drop never leaves the table"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_drop_that_is_no_deck_reports_it_and_opens_nothing() {
        let mut panel = ImportPanel::default();
        let mut menu = Menu::default();
        let mut editor = crate::deck::editor::DeckEditor::default();
        assert_eq!(stage_drop(&mut panel, &mut menu, &mut editor, "3\n"), None);
        assert!(panel.error.as_deref().unwrap().contains("neither"));
        assert_eq!(menu.screen, Screen::Games);
        assert!(editor.draft.is_none());
    }

    #[test]
    fn a_paste_lands_only_while_no_text_field_has_focus() {
        let context = egui::Context::default();
        let input = egui::RawInput {
            events: vec![egui::Event::Paste("1 Lonely Poro".into())],
            ..Default::default()
        };
        context.begin_pass(input.clone());
        assert_eq!(unfocused_paste(&context).as_deref(), Some("1 Lonely Poro"));
        context.end_pass().textures_delta.clear();
        let mut text = String::new();
        context
            .run_ui(input, |ui| {
                let response = ui.text_edit_singleline(&mut text);
                response.request_focus();
            })
            .textures_delta
            .clear();
        let mut focused = None;
        let mut output = context.run_ui(
            egui::RawInput {
                events: vec![egui::Event::Paste("1 Lonely Poro".into())],
                ..Default::default()
            },
            |ui| {
                ui.text_edit_singleline(&mut text);
                focused = Some(unfocused_paste(ui.ctx()));
            },
        );
        output.textures_delta.clear();
        assert_eq!(focused, Some(None));
    }

    #[test]
    fn a_command_v_arrives_as_text_beside_the_key_and_plain_typing_does_not() {
        let key = |command: bool| egui::Event::Key {
            key: egui::Key::V,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                command,
                ..egui::Modifiers::NONE
            },
        };
        let text = egui::Event::Text("1 Lonely Poro".into());
        assert_eq!(
            pasted_text(&[key(true), text.clone()]).as_deref(),
            Some("1 Lonely Poro")
        );
        assert_eq!(pasted_text(&[key(false), text.clone()]), None);
        assert_eq!(pasted_text(&[text]), None);
        assert_eq!(
            pasted_text(&[key(true), egui::Event::Text("  ".into())]),
            None
        );
        assert_eq!(
            pasted_text(&[egui::Event::Paste("code".into())]).as_deref(),
            Some("code")
        );
    }

    #[test]
    fn the_share_menu_labels_and_notes_name_every_form() {
        for what in Share::ALL {
            assert!(what.label().starts_with("copy "));
            assert!(!what.note().is_empty());
        }
        assert!(Share::CodeList.note().contains("no sideboard"));
        assert!(Share::TextList.note().contains("names, not prints"));
    }

    #[test]
    fn the_qr_side_fits_the_smaller_screen_edge() {
        assert_eq!(qr_side(egui::vec2(1280.0, 800.0)), QR_MAX_SIDE);
        assert_eq!(qr_side(egui::vec2(360.0, 640.0)), 296.0);
        assert_eq!(qr_side(egui::vec2(800.0, 360.0)), 296.0);
        assert_eq!(qr_side(egui::vec2(100.0, 100.0)), 120.0);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn saving_renames_replaces_or_copies_per_the_origin() {
        use crate::deck::history::store;
        let dir = std::env::temp_dir().join(format!("kai-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let game = agni_riftbound::GAME;
        let deck = pool::deck("lillia-jonnynick").unwrap();
        let mut draft = Draft::from_deck(deck, "Lillia Aggro", Origin::New);
        draft.dirty = true;
        let first = save_in(&dir, &mut draft, game).unwrap();
        assert_eq!(draft.origin, Origin::Saved(first));
        assert!(!draft.dirty);
        let rows = store::rows_in(&dir, game);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "Lillia Aggro");

        assert_eq!(save_in(&dir, &mut draft, game).unwrap(), first);
        assert_eq!(store::rows_in(&dir, game).len(), 1);

        draft.label = "Lillia Tempo".into();
        let renamed = save_in(&dir, &mut draft, game).unwrap();
        assert_eq!(renamed, first, "a rename keeps the identity");
        let rows = store::rows_in(&dir, game);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "Lillia Tempo");

        let spare = draft.deck.main_deck[0].card.clone();
        agni_deck::push(&mut draft.deck.sideboard, spare, 1);
        draft.dirty = true;
        let replaced = save_in(&dir, &mut draft, game).unwrap();
        assert_ne!(replaced, first);
        let rows = store::rows_in(&dir, game);
        assert_eq!(rows.len(), 1, "replace forgets the old row: {rows:?}");
        assert_eq!(rows[0].ci, replaced);
        assert_eq!(rows[0].label, "Lillia Tempo");
        assert_eq!(draft.origin, Origin::Saved(replaced));

        draft.deck.sideboard.clear();
        draft.label = "Lillia Aggro".into();
        let copy = save_as_copy_in(&dir, &mut draft).unwrap();
        assert_eq!(copy, first);
        let rows = store::rows_in(&dir, game);
        assert_eq!(rows.len(), 2, "save as copy keeps both: {rows:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_save_label_falls_back_to_the_deck_label() {
        let deck = pool::deck("lillia-jonnynick").unwrap();
        let mut draft = Draft::from_deck(deck, "  ", Origin::New);
        assert_eq!(save_label(&draft), "Lillia (Jonnynick)");
        draft.label = " Lillia Aggro ".into();
        assert_eq!(save_label(&draft), "Lillia Aggro");
    }
}
