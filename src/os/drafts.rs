use crate::deck::editor::{Draft, Origin};
use agni_deck::Snapshot;
use agni_importers::riftbound::snapshot;
use serde::{Deserialize, Serialize};

pub const FILE_NAME: &str = "deck-draft.cbor";
pub const WEB_KEY: &str = "kai.deck-draft";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotOrigin {
    pub kind: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    pub label: String,
    pub origin: SlotOrigin,
    pub snapshot: Snapshot,
}

pub fn origin_to_slot(origin: &Origin) -> SlotOrigin {
    let (kind, value) = match origin {
        Origin::New => ("new", String::new()),
        Origin::Pool(slug) => ("pool", slug.clone()),
        Origin::Saved(ci) => ("saved", ci.to_string()),
        Origin::Import(source) => ("import", source.clone()),
        Origin::Seated => ("seated", String::new()),
    };
    SlotOrigin {
        kind: kind.to_string(),
        value,
    }
}

pub fn origin_from_slot(slot: &SlotOrigin) -> Origin {
    match slot.kind.as_str() {
        "pool" => Origin::Pool(slot.value.clone()),
        "saved" => spirit_sdk::CiHash::parse(&slot.value)
            .map(Origin::Saved)
            .unwrap_or(Origin::New),
        "import" => Origin::Import(slot.value.clone()),
        "seated" => Origin::Seated,
        _ => Origin::New,
    }
}

pub fn slot_of(draft: &Draft) -> Slot {
    Slot {
        label: draft.label.clone(),
        origin: origin_to_slot(&draft.origin),
        snapshot: snapshot::snapshot(&draft.deck),
    }
}

pub fn draft_of(slot: &Slot) -> Option<Draft> {
    let deck = snapshot::deck(&slot.snapshot)?;
    let mut draft = Draft::from_deck(deck, &slot.label, origin_from_slot(&slot.origin));
    draft.dirty = true;
    Some(draft)
}

pub fn encode(slot: &Slot) -> Result<Vec<u8>, String> {
    spirit_sdk::canonical::to_vec(slot).map_err(|error| error.to_string())
}

pub fn decode(bytes: &[u8]) -> Result<Slot, String> {
    spirit_sdk::canonical::from_slice(bytes).map_err(|error| error.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
mod platform {
    use super::{decode, draft_of, encode, slot_of, Draft, FILE_NAME};
    use std::path::{Path, PathBuf};

    fn slot_path(dir: &Path) -> PathBuf {
        dir.join(FILE_NAME)
    }

    pub fn store_in(dir: &Path, draft: &Draft) -> Result<(), String> {
        let bytes = encode(&slot_of(draft))?;
        std::fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
        let path = slot_path(dir);
        std::fs::write(&path, bytes).map_err(|error| format!("{}: {error}", path.display()))
    }

    pub fn load_in(dir: &Path) -> Option<Draft> {
        let bytes = std::fs::read(slot_path(dir)).ok()?;
        draft_of(&decode(&bytes).ok()?)
    }

    pub fn clear_in(dir: &Path) {
        let _ = std::fs::remove_file(slot_path(dir));
    }

    pub fn store(draft: &Draft) -> Result<(), String> {
        store_in(&crate::os::paths::config_dir(), draft)
    }

    pub fn load() -> Option<Draft> {
        load_in(&crate::os::paths::config_dir())
    }

    pub fn clear() {
        clear_in(&crate::os::paths::config_dir());
    }
}

#[cfg(target_arch = "wasm32")]
mod platform {
    use super::{draft_of, slot_of, Draft, Slot, WEB_KEY};

    fn storage() -> Result<web_sys::Storage, String> {
        web_sys::window()
            .ok_or("no window")?
            .local_storage()
            .map_err(|_| "localStorage unavailable")?
            .ok_or_else(|| "localStorage disabled".to_string())
    }

    pub fn store(draft: &Draft) -> Result<(), String> {
        let text = serde_json::to_string(&slot_of(draft)).map_err(|error| error.to_string())?;
        storage()?
            .set_item(WEB_KEY, &text)
            .map_err(|_| "localStorage refused the draft".to_string())
    }

    pub fn load() -> Option<Draft> {
        let text = storage().ok()?.get_item(WEB_KEY).ok()??;
        let slot: Slot = serde_json::from_str(&text).ok()?;
        draft_of(&slot)
    }

    pub fn clear() {
        if let Ok(storage) = storage() {
            let _ = storage.remove_item(WEB_KEY);
        }
    }
}

pub use platform::{clear, load, store};
#[cfg(not(target_arch = "wasm32"))]
pub use platform::{clear_in, load_in, store_in};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::editor::Edit;
    use agni_riftbound::ResolvedCard;

    fn draft() -> Draft {
        let mut draft = Draft::new("Lillia Aggro", Origin::Pool("lillia-house".into()));
        draft
            .apply(Edit::Add(ResolvedCard {
                name: "Lonely Poro".into(),
                riftbound_id: "sfd-036-298".into(),
                kind: Some("Unit".into()),
                energy: Some(1),
                tags: vec!["Poro".into()],
                ..Default::default()
            }))
            .unwrap();
        draft
    }

    #[test]
    fn a_slot_round_trips_label_origin_and_deck() {
        let original = draft();
        let slot = slot_of(&original);
        let bytes = encode(&slot).unwrap();
        let restored = draft_of(&decode(&bytes).unwrap()).unwrap();
        assert_eq!(restored.label, "Lillia Aggro");
        assert_eq!(restored.origin, Origin::Pool("lillia-house".into()));
        assert_eq!(restored.deck, original.deck);
        assert!(restored.dirty, "a restored draft is unsaved work");
        assert_eq!(restored.deck.main_deck[0].card.tags, vec!["Poro"]);
    }

    #[test]
    fn every_origin_survives_the_slot() {
        let ci = spirit_sdk::CiHash::from_hash(spirit_sdk::BlobHash::of(b"deck"));
        for origin in [
            Origin::New,
            Origin::Pool("irelia-house".into()),
            Origin::Saved(ci),
            Origin::Import("paste".into()),
            Origin::Seated,
        ] {
            assert_eq!(origin_from_slot(&origin_to_slot(&origin)), origin);
        }
        assert_eq!(
            origin_from_slot(&SlotOrigin {
                kind: "saved".into(),
                value: "not a hash".into()
            }),
            Origin::New,
            "a saved origin whose hash no longer parses starts fresh"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_slot_lives_in_the_config_dir_and_clears() {
        let dir = std::env::temp_dir().join(format!("kai-drafts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(load_in(&dir).is_none());
        store_in(&dir, &draft()).unwrap();
        assert!(dir.join(FILE_NAME).is_file());
        let restored = load_in(&dir).unwrap();
        assert_eq!(restored.label, "Lillia Aggro");
        clear_in(&dir);
        assert!(load_in(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
