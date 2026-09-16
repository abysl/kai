use bevy::input::keyboard::{Key, KeyCode, KeyboardInput, NativeKeyCode};
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{egui, EguiContext, EguiContexts, EguiOutput};
use parking_lot::Mutex;

const PAD_WIDTH: usize = 64;
const PAD_FLOOR: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Edit {
    Delete(usize),
    Insert(String),
    Submit,
}

static BUFFER: Mutex<String> = Mutex::new(String::new());
static EDITS: Mutex<Vec<Edit>> = Mutex::new(Vec::new());
static FOCUSED: Mutex<bool> = Mutex::new(false);

fn pad() -> String {
    " ".repeat(PAD_WIDTH)
}

fn diff(before: &str, after: &str) -> Vec<Edit> {
    if before == after {
        return Vec::new();
    }
    let old: Vec<char> = before.chars().collect();
    let new: Vec<char> = after.chars().collect();
    let prefix = old
        .iter()
        .zip(new.iter())
        .take_while(|(left, right)| left == right)
        .count();
    let room = old.len().min(new.len()) - prefix;
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count()
        .min(room);
    let deleted = old.len() - prefix - suffix;
    let inserted: String = new[prefix..new.len() - suffix].iter().collect();
    let mut edits = Vec::new();
    if deleted > 0 {
        edits.push(Edit::Delete(deleted));
    }
    if !inserted.is_empty() {
        edits.push(Edit::Insert(inserted));
    }
    edits
}

pub fn observe(text: String) {
    let mut buffer = BUFFER.lock();
    let edits = diff(&buffer, &text);
    *buffer = text;
    drop(buffer);
    if !edits.is_empty() {
        EDITS.lock().extend(edits);
    }
}

pub fn submit() {
    EDITS.lock().push(Edit::Submit);
}

fn take_edits() -> Vec<Edit> {
    std::mem::take(&mut *EDITS.lock())
}

fn seed(text: &str) {
    *BUFFER.lock() = text.to_string();
    if let Err(error) = platform::seed_buffer(text) {
        warn!("could not seed the android text buffer: {error}");
    }
}

#[cfg(target_os = "android")]
mod platform {
    use crate::os::android::with_activity;
    use jni::objects::JValue;

    pub fn seed_buffer(text: &str) -> Result<(), String> {
        with_activity(|env, activity| {
            let value = env.new_string(text)?;
            env.call_method(
                activity,
                "setTextInputBuffer",
                "(Ljava/lang/String;)V",
                &[JValue::Object(&value)],
            )?;
            Ok(())
        })
    }
}

#[cfg(not(target_os = "android"))]
mod platform {
    pub fn seed_buffer(_text: &str) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_blue_rae_kai_MainActivity_nativeTextInput(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    text: jni::objects::JString,
    dismissed: jni::sys::jboolean,
) {
    let text = env.get_string(&text).map(String::from).unwrap_or_default();
    if dismissed != 0 {
        *BUFFER.lock() = text;
        return;
    }
    observe(text);
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_blue_rae_kai_MainActivity_nativeEditorAction(
    _env: jni::JNIEnv,
    _class: jni::objects::JClass,
    _action: jni::sys::jint,
) {
    submit();
}

pub fn follow_focus(outputs: Query<&EguiOutput, With<EguiContext>>) {
    let wants_text = outputs
        .iter()
        .any(|output| output.platform_output.ime.is_some());
    let was_focused = {
        let mut focused = FOCUSED.lock();
        let was = *focused;
        *focused = wants_text;
        was
    };
    if was_focused != wants_text {
        EDITS.lock().clear();
        let fresh = if wants_text { pad() } else { String::new() };
        seed(&fresh);
        return;
    }
    if wants_text && BUFFER.lock().chars().count() < PAD_FLOOR {
        seed(&pad());
    }
}

pub fn covered_by_ime(focused: egui::Rect, content: egui::Rect, ime: f32) -> bool {
    ime > 0.0 && focused.max.y > content.max.y
}

pub fn scroll_focused_above_ime(
    safe: Res<crate::viewport::SafeInsets>,
    mut contexts: EguiContexts,
) {
    if safe.ime <= 0.0 {
        return;
    }
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    let Some(focused) = context.memory(|memory| memory.focused()) else {
        return;
    };
    let Some(response) = context.read_response(focused) else {
        return;
    };
    if covered_by_ime(response.rect, context.content_rect(), safe.ime) {
        response.scroll_to_me(Some(egui::Align::Center));
    }
}

fn tap(
    keys: &mut MessageWriter<KeyboardInput>,
    window: Entity,
    key_code: KeyCode,
    logical_key: Key,
) {
    for state in [ButtonState::Pressed, ButtonState::Released] {
        keys.write(KeyboardInput {
            key_code,
            logical_key: logical_key.clone(),
            state,
            text: None,
            repeat: false,
            window,
        });
    }
}

pub fn drain_text(
    windows: Query<Entity, With<PrimaryWindow>>,
    mut keys: MessageWriter<KeyboardInput>,
) {
    let edits = take_edits();
    if edits.is_empty() {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    for edit in edits {
        match edit {
            Edit::Delete(count) => {
                for _ in 0..count {
                    tap(&mut keys, window, KeyCode::Backspace, Key::Backspace);
                }
            }
            Edit::Insert(text) => {
                keys.write(KeyboardInput {
                    key_code: KeyCode::Unidentified(NativeKeyCode::Unidentified),
                    logical_key: Key::Character(text.as_str().into()),
                    state: ButtonState::Pressed,
                    text: Some(text.as_str().into()),
                    repeat: false,
                    window,
                });
                keys.write(KeyboardInput {
                    key_code: KeyCode::Unidentified(NativeKeyCode::Unidentified),
                    logical_key: Key::Character(text.as_str().into()),
                    state: ButtonState::Released,
                    text: None,
                    repeat: false,
                    window,
                });
            }
            Edit::Submit => tap(&mut keys, window, KeyCode::Enter, Key::Enter),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unchanged_buffer_produces_no_edits() {
        assert!(diff("abc", "abc").is_empty());
        assert!(diff("", "").is_empty());
    }

    #[test]
    fn appended_text_becomes_one_insert() {
        assert_eq!(diff("", "a"), vec![Edit::Insert("a".into())]);
        assert_eq!(
            diff(
                &" ".repeat(PAD_WIDTH),
                &format!("{}hi", " ".repeat(PAD_WIDTH))
            ),
            vec![Edit::Insert("hi".into())]
        );
    }

    #[test]
    fn a_backspace_becomes_one_delete_and_nothing_else() {
        assert_eq!(diff("abc", "ab"), vec![Edit::Delete(1)]);
        assert_eq!(
            diff(&" ".repeat(PAD_WIDTH), &" ".repeat(PAD_WIDTH - 1)),
            vec![Edit::Delete(1)]
        );
    }

    #[test]
    fn a_word_swap_deletes_only_the_changed_middle() {
        assert_eq!(
            diff("hello there", "hello world there"),
            vec![Edit::Insert("world ".into())]
        );
        assert_eq!(
            diff("hello world", "hello there"),
            vec![Edit::Delete(5), Edit::Insert("there".into())]
        );
    }

    #[test]
    fn a_cleared_buffer_deletes_everything_typed() {
        assert_eq!(diff("abcd", ""), vec![Edit::Delete(4)]);
    }

    #[test]
    fn a_typed_line_over_the_pad_queues_exactly_what_the_field_should_receive() {
        seed(&pad());
        take_edits();
        observe(format!("{}rae", pad()));
        observe(format!("{}ra", pad()));
        observe(format!("{}raava", pad()));
        submit();
        assert_eq!(
            take_edits(),
            vec![
                Edit::Insert("rae".into()),
                Edit::Delete(1),
                Edit::Insert("ava".into()),
                Edit::Submit,
            ]
        );
        seed("");
    }

    #[test]
    fn a_focused_field_under_the_keyboard_asks_to_be_scrolled_up() {
        let content = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(360.0, 500.0));
        let under = egui::Rect::from_min_max(egui::pos2(16.0, 520.0), egui::pos2(300.0, 560.0));
        let above = egui::Rect::from_min_max(egui::pos2(16.0, 120.0), egui::pos2(300.0, 160.0));
        assert!(covered_by_ime(under, content, 300.0));
        assert!(!covered_by_ime(above, content, 300.0));
        assert!(!covered_by_ime(under, content, 0.0));
    }

    #[test]
    fn multibyte_text_counts_characters_not_bytes() {
        assert_eq!(diff("héllo", "héllo!"), vec![Edit::Insert("!".into())]);
        assert_eq!(diff("héllo", "héll"), vec![Edit::Delete(1)]);
        assert_eq!(diff("日本", "日本語"), vec![Edit::Insert("語".into())]);
    }
}
