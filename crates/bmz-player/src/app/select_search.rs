use std::collections::VecDeque;
use std::time::{Duration, Instant};

use winit::event::{ElementState, Ime, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::screens::select_model::MAX_SEARCH_HISTORY;

const PLACEHOLDER_ALPHA: f32 = 0.45;
const MESSAGE_ALPHA: f32 = 0.6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SearchInputAction {
    Ignored,
    Consumed,
    CursorMoved,
    EnterMode,
    ExitMode,
    Execute,
}

/// 選曲画面の検索テキスト、UTF-8 cursor、IME preedit、履歴、feedback状態。
pub(super) struct SelectSearchRuntime {
    active: bool,
    query: String,
    cursor: usize,
    caret_blink_started_at: Instant,
    preedit: String,
    history: VecDeque<String>,
    message: Option<String>,
}

impl SelectSearchRuntime {
    pub(super) fn new(now: Instant) -> Self {
        Self {
            active: false,
            query: String::new(),
            cursor: 0,
            caret_blink_started_at: now,
            preedit: String::new(),
            history: VecDeque::new(),
            message: None,
        }
    }

    pub(super) fn is_active(&self) -> bool {
        self.active
    }

    pub(super) fn history(&self) -> &VecDeque<String> {
        &self.history
    }

    pub(super) fn trimmed_query(&self) -> String {
        self.query.trim().to_string()
    }

    pub(super) fn set_active(&mut self, active: bool) {
        self.active = active;
        self.query.clear();
        self.cursor = 0;
        self.reset_caret_blink();
        self.preedit.clear();
        if !active {
            self.message = None;
        }
    }

    pub(super) fn clear_message(&mut self) {
        self.message = None;
    }

    pub(super) fn set_message(&mut self, message: String) {
        self.message = Some(message);
    }

    pub(super) fn set_no_results(&mut self, message: String) {
        self.query.clear();
        self.cursor = 0;
        self.reset_caret_blink();
        self.message = Some(message);
    }

    pub(super) fn record_successful_query(&mut self, query: String) {
        self.history.retain(|existing| existing != &query);
        while self.history.len() >= MAX_SEARCH_HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(query);
    }

    pub(super) fn display_word(
        &self,
        hidden: bool,
        placeholder: String,
    ) -> (String, f32, Option<usize>) {
        if hidden {
            return (String::new(), 0.0, None);
        }
        let blink_on = search_caret_visible(self.caret_blink_started_at.elapsed());
        if self.active {
            if self.query.is_empty()
                && self.preedit.is_empty()
                && let Some(message) = &self.message
            {
                return (message.clone(), MESSAGE_ALPHA, None);
            }
            let cursor = clamp_search_cursor(&self.query, self.cursor);
            let text = search_display_text(&self.query, cursor, &self.preedit);
            let caret = blink_on.then_some(cursor + self.preedit.len());
            (text, 1.0, caret)
        } else if let Some(message) = &self.message {
            (message.clone(), MESSAGE_ALPHA, None)
        } else {
            (placeholder, PLACEHOLDER_ALPHA, None)
        }
    }

    pub(super) fn cursor_to_end(&mut self) {
        self.cursor = self.query.len();
        self.reset_caret_blink();
    }

    pub(super) fn apply_ime(&mut self, ime: &Ime) {
        match ime {
            Ime::Enabled | Ime::Disabled => self.preedit.clear(),
            Ime::Preedit(text, _) => self.preedit = text.clone(),
            Ime::Commit(text) => {
                search_insert_text(&mut self.query, &mut self.cursor, text);
                self.reset_caret_blink();
                self.preedit.clear();
                self.message = None;
            }
        }
    }

    pub(super) fn paste(&mut self, text: &str) -> bool {
        if !self.active || !self.preedit.is_empty() {
            return false;
        }
        let text: String = text
            .replace("\r\n", "\n")
            .chars()
            .filter_map(|ch| match ch {
                '\r' | '\n' | '\t' | '\u{2028}' | '\u{2029}' => Some(' '),
                ch if ch.is_control() => None,
                ch => Some(ch),
            })
            .collect();
        if text.trim().is_empty() {
            return false;
        }
        search_insert_text(&mut self.query, &mut self.cursor, &text);
        self.reset_caret_blink();
        self.clear_message();
        true
    }

    pub(super) fn handle_key(
        &mut self,
        event: &KeyEvent,
        e_action_held: bool,
        in_settings: bool,
    ) -> SearchInputAction {
        if !self.active {
            return if should_start_search_mode(
                event.physical_key,
                event.state,
                event.repeat,
                e_action_held,
                in_settings,
            ) {
                SearchInputAction::EnterMode
            } else {
                SearchInputAction::Ignored
            };
        }
        if event.state != ElementState::Pressed {
            return SearchInputAction::Consumed;
        }

        match event.physical_key {
            PhysicalKey::Code(KeyCode::Escape) => SearchInputAction::ExitMode,
            PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter) => {
                if event.repeat || !self.preedit.is_empty() {
                    SearchInputAction::Consumed
                } else {
                    SearchInputAction::Execute
                }
            }
            PhysicalKey::Code(KeyCode::Backspace) => {
                if self.preedit.is_empty() {
                    search_delete_backward(&mut self.query, &mut self.cursor);
                    self.reset_caret_blink();
                }
                SearchInputAction::Consumed
            }
            PhysicalKey::Code(KeyCode::Delete) => {
                if self.preedit.is_empty() {
                    search_delete_forward(&mut self.query, &mut self.cursor);
                    self.reset_caret_blink();
                }
                SearchInputAction::Consumed
            }
            PhysicalKey::Code(KeyCode::ArrowLeft) if self.preedit.is_empty() => {
                self.cursor_action(previous_search_cursor(&self.query, self.cursor))
            }
            PhysicalKey::Code(KeyCode::ArrowRight) if self.preedit.is_empty() => {
                self.cursor_action(next_search_cursor(&self.query, self.cursor))
            }
            PhysicalKey::Code(KeyCode::Home) if self.preedit.is_empty() => self.cursor_action(0),
            PhysicalKey::Code(KeyCode::End) if self.preedit.is_empty() => {
                self.cursor_to_end();
                SearchInputAction::CursorMoved
            }
            _ => {
                if let Some(text) = event.text.as_ref() {
                    if self.message.is_some() && self.query.is_empty() && text.as_str() == "/" {
                        self.message = None;
                        return SearchInputAction::Consumed;
                    }
                    for ch in text.chars().filter(|ch| !ch.is_control()) {
                        search_insert_char(&mut self.query, &mut self.cursor, ch);
                        self.reset_caret_blink();
                        self.message = None;
                    }
                }
                SearchInputAction::Consumed
            }
        }
    }

    fn cursor_action(&mut self, cursor: usize) -> SearchInputAction {
        let cursor = clamp_search_cursor(&self.query, cursor);
        if self.cursor != cursor {
            self.cursor = cursor;
            self.reset_caret_blink();
            SearchInputAction::CursorMoved
        } else {
            SearchInputAction::Consumed
        }
    }

    fn reset_caret_blink(&mut self) {
        self.caret_blink_started_at = Instant::now();
    }
}

fn should_start_search_mode(
    physical_key: PhysicalKey,
    state: ElementState,
    repeat: bool,
    e_action_held: bool,
    in_settings: bool,
) -> bool {
    physical_key == PhysicalKey::Code(KeyCode::Slash)
        && state == ElementState::Pressed
        && !repeat
        && !e_action_held
        && !in_settings
}

fn search_display_text(query: &str, cursor: usize, preedit: &str) -> String {
    let cursor = clamp_search_cursor(query, cursor);
    let mut text = String::with_capacity(query.len() + preedit.len());
    text.push_str(&query[..cursor]);
    text.push_str(preedit);
    text.push_str(&query[cursor..]);
    text
}

fn search_caret_visible(elapsed: Duration) -> bool {
    (elapsed.as_micros() / 500_000).is_multiple_of(2)
}

fn search_insert_char(query: &mut String, cursor: &mut usize, ch: char) {
    let index = clamp_search_cursor(query, *cursor);
    query.insert(index, ch);
    *cursor = index + ch.len_utf8();
}

fn search_insert_text(query: &mut String, cursor: &mut usize, text: &str) {
    let index = clamp_search_cursor(query, *cursor);
    query.insert_str(index, text);
    *cursor = index + text.len();
}

fn search_delete_backward(query: &mut String, cursor: &mut usize) {
    let index = clamp_search_cursor(query, *cursor);
    if index == 0 {
        *cursor = 0;
        return;
    }
    let previous = previous_search_cursor(query, index);
    query.drain(previous..index);
    *cursor = previous;
}

fn search_delete_forward(query: &mut String, cursor: &mut usize) {
    let index = clamp_search_cursor(query, *cursor);
    if index >= query.len() {
        *cursor = query.len();
        return;
    }
    let next = next_search_cursor(query, index);
    query.drain(index..next);
    *cursor = index;
}

fn previous_search_cursor(query: &str, cursor: usize) -> usize {
    let cursor = clamp_search_cursor(query, cursor);
    query[..cursor].char_indices().last().map(|(index, _)| index).unwrap_or(0)
}

fn next_search_cursor(query: &str, cursor: usize) -> usize {
    let cursor = clamp_search_cursor(query, cursor);
    if cursor >= query.len() {
        return query.len();
    }
    query[cursor..].char_indices().nth(1).map(|(offset, _)| cursor + offset).unwrap_or(query.len())
}

fn clamp_search_cursor(query: &str, cursor: usize) -> usize {
    let mut cursor = cursor.min(query.len());
    while cursor > 0 && !query.is_char_boundary(cursor) {
        cursor -= 1;
    }
    cursor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_input_inserts_and_deletes_at_cursor() {
        let mut query = "abcd".to_string();
        let mut cursor = 2;
        search_insert_char(&mut query, &mut cursor, 'X');
        assert_eq!((&query, cursor), (&"abXcd".to_string(), 3));
        search_delete_backward(&mut query, &mut cursor);
        assert_eq!((&query, cursor), (&"abcd".to_string(), 2));
        search_delete_forward(&mut query, &mut cursor);
        assert_eq!((&query, cursor), (&"abd".to_string(), 2));
    }

    #[test]
    fn search_mode_start_respects_settings_and_e_action_holds() {
        let slash = PhysicalKey::Code(KeyCode::Slash);
        assert!(should_start_search_mode(slash, ElementState::Pressed, false, false, false));
        assert!(!should_start_search_mode(slash, ElementState::Pressed, false, true, false));
        assert!(!should_start_search_mode(slash, ElementState::Pressed, false, false, true));
        assert!(!should_start_search_mode(slash, ElementState::Pressed, true, false, false));
    }

    #[test]
    fn search_input_moves_by_utf8_char_boundaries() {
        let query = "a楽b".to_string();
        let mut cursor = query.len();
        cursor = previous_search_cursor(&query, cursor);
        assert_eq!(cursor, "a楽".len());
        cursor = previous_search_cursor(&query, cursor);
        assert_eq!(cursor, "a".len());
        cursor = next_search_cursor(&query, cursor);
        assert_eq!(cursor, "a楽".len());
        let mut edited = query;
        search_delete_backward(&mut edited, &mut cursor);
        assert_eq!((&edited, cursor), (&"ab".to_string(), "a".len()));
    }

    #[test]
    fn search_paste_inserts_single_line_at_utf8_cursor() {
        let mut search = SelectSearchRuntime::new(Instant::now());
        search.set_active(true);
        search.apply_ime(&Ime::Commit("音曲".into()));
        search.cursor_action("音".len());
        search.set_message("no results".into());

        assert!(search.paste("楽\r\n祭\t2026\u{0000}"));
        assert_eq!(search.trimmed_query(), "音楽 祭 2026曲");
        assert_eq!(search.cursor, "音楽 祭 2026".len());
        assert!(search.message.is_none());
    }

    #[test]
    fn search_paste_preserves_state_without_text_or_during_composition() {
        let mut search = SelectSearchRuntime::new(Instant::now());
        assert!(!search.paste("outside the search field"));
        search.set_active(true);
        search.apply_ime(&Ime::Commit("音".into()));
        search.set_message("no results".into());
        assert!(!search.paste(""));
        assert!(!search.paste("\r\n\t"));
        search.apply_ime(&Ime::Preedit("がく".into(), None));
        assert!(!search.paste("楽"));
        assert_eq!(search.display_word(false, String::new()).0, "音がく");
        assert_eq!(search.cursor, "音".len());
        assert_eq!(search.message.as_deref(), Some("no results"));
    }

    #[test]
    fn search_display_inserts_preedit_without_caret_character() {
        assert_eq!(search_display_text("ab cd", 2, "変換"), "ab変換 cd");
        assert_eq!(search_display_text("a楽b", 2, ""), "a楽b");
    }

    #[test]
    fn search_caret_blink_starts_visible_after_reset() {
        assert!(search_caret_visible(Duration::ZERO));
        assert!(search_caret_visible(Duration::from_millis(499)));
        assert!(!search_caret_visible(Duration::from_millis(500)));
        assert!(search_caret_visible(Duration::from_millis(1_000)));
    }

    #[test]
    fn successful_query_history_is_bounded_and_deduplicated() {
        let mut search = SelectSearchRuntime::new(Instant::now());
        for index in 0..=MAX_SEARCH_HISTORY {
            search.record_successful_query(format!("query-{index}"));
        }
        search.record_successful_query("query-1".to_string());
        assert_eq!(search.history.len(), MAX_SEARCH_HISTORY);
        assert_eq!(search.history.back().map(String::as_str), Some("query-1"));
        assert_eq!(search.history.iter().filter(|query| *query == "query-1").count(), 1);
    }
}
