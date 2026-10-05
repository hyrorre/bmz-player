//! Read-only E2 panel data. IDs are explicit and independent of display order.
use std::sync::Arc;

pub const DETAIL_OPTION_ROWS: usize = 7;
pub const DETAIL_OPTION_CENTER: usize = DETAIL_OPTION_ROWS / 2;
/// Two extra, offscreen slots preserve the departing columns during movement.
pub const DETAIL_OPTION_DRAW_SLOTS: usize = DETAIL_OPTION_ROWS + 2;
pub const DETAIL_OPTIONS_CLOSE_MS: i64 = 300;

/// Frozen display data; never used as an editable panel.
#[derive(Debug, Clone, PartialEq)]
pub struct DetailOptionsClosingSnapshot {
    pub panel: Arc<DetailOptionsSnapshot>,
    pub scroll: f32,
}

pub fn detail_options_column(slot: usize) -> i32 {
    match slot {
        7 => -1,
        8 => 7,
        _ => slot as i32,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailValueKind {
    Bool,
    Enum,
    Number { min: i64, max: i64, step: i64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetailOptionChoice {
    pub value: i64,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetailOptionRow {
    pub item_id: i64,
    pub category_id: i64,
    pub scope: i64,
    pub value: i64,
    pub value_index: i64,
    pub choice_count: i64,
    pub choices: Arc<[DetailOptionChoice]>,
    pub kind: DetailValueKind,
    pub label: String,
    pub value_label: String,
    pub category: String,
    pub description: String,
    pub reason: String,
    pub auxiliary: String,
    pub status: String,
    pub editable: bool,
    pub effective: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetailOptionsSnapshot {
    pub cursor: usize,
    pub viewport_start: usize,
    pub items: Arc<[DetailOptionRow]>,
    pub title: String,
    pub scope_label: String,
    pub guide: String,
    pub position: String,
}

impl DetailOptionsSnapshot {
    pub fn selected(&self) -> Option<&DetailOptionRow> {
        self.items.get(self.cursor)
    }

    pub fn row(&self, slot: usize) -> Option<&DetailOptionRow> {
        detail_options_row_index(self.cursor, self.items.len(), slot)
            .and_then(|index| self.items.get(index))
    }
}

pub fn detail_options_viewport(cursor: usize, count: usize) -> usize {
    if count == 0 { 0 } else { (cursor % count + count - DETAIL_OPTION_CENTER % count) % count }
}

/// Circular viewport with a unique selection at the center. Small catalogues
/// leave empty slots around their centered window rather than repeat items.
pub fn detail_options_row_index(cursor: usize, count: usize, slot: usize) -> Option<usize> {
    if count == 0 || cursor >= count || slot >= DETAIL_OPTION_DRAW_SLOTS {
        return None;
    }
    if slot >= DETAIL_OPTION_ROWS {
        return (count > DETAIL_OPTION_ROWS).then(|| {
            (cursor as i64 + i64::from(detail_options_column(slot)) - DETAIL_OPTION_CENTER as i64)
                .rem_euclid(count as i64) as usize
        });
    }
    let visible = count.min(DETAIL_OPTION_ROWS);
    let first = DETAIL_OPTION_CENTER - (visible - 1) / 2;
    (first..first + visible)
        .contains(&slot)
        .then(|| (detail_options_viewport(cursor, count) + slot) % count)
}

fn slot(id: i32) -> Option<(usize, i32)> {
    use bmz_skin_document::*;
    (SKIN_DETAIL_OPTIONS_ROW_BASE..=SKIN_DETAIL_OPTIONS_ROW_LAST).contains(&id).then(|| {
        (
            ((id - SKIN_DETAIL_OPTIONS_ROW_BASE) / SKIN_DETAIL_OPTIONS_ROW_STRIDE) as usize,
            (id - SKIN_DETAIL_OPTIONS_ROW_BASE) % SKIN_DETAIL_OPTIONS_ROW_STRIDE,
        )
    })
}

pub fn number(id: i32, panel: Option<&DetailOptionsSnapshot>) -> Option<i64> {
    use bmz_skin_document::*;
    if let Some((slot, choice, field)) = detail_options_choice_slot(id) {
        return Some(
            panel
                .and_then(|p| p.row(slot))
                .and_then(|r| r.choices.get(choice))
                .filter(|_| choice < SKIN_DETAIL_OPTIONS_CHOICES && field == 0)
                .map_or(-1, |c| c.value),
        );
    }
    if let Some((slot, field)) = slot(id) {
        return Some(
            panel
                .and_then(|p| p.row(slot))
                .map(|row| match field {
                    0 => row.item_id,
                    1 => row.category_id,
                    2 => row.value,
                    3 => row.scope,
                    4 => row.choice_count,
                    5 => row.value_index,
                    6..=8 => match row.kind {
                        DetailValueKind::Number { min, max, step } => {
                            [min, max, step][(field - 6) as usize]
                        }
                        _ => -1,
                    },
                    _ => -1,
                })
                .unwrap_or(-1),
        );
    }
    if !(SKIN_REF_DETAIL_OPTIONS_BASE..=SKIN_REF_DETAIL_OPTIONS_LAST).contains(&id) {
        return None;
    }
    let field = id - SKIN_REF_DETAIL_OPTIONS_BASE;
    let Some(p) = panel else {
        return Some(if matches!(field, 1 | 7 | 12) { 0 } else { -1 });
    };
    let r = p.selected();
    Some(match field {
        0 => p.cursor as i64,
        1 => p.items.len() as i64,
        2 => r.map(|r| r.item_id).unwrap_or(-1),
        3 => r.map(|r| r.category_id).unwrap_or(-1),
        4 => r.map(|r| r.scope).unwrap_or(-1),
        5 => r.map(|r| r.value).unwrap_or(-1),
        6 => r.map(|r| r.value_index).unwrap_or(-1),
        7 => r.map(|r| r.choice_count).unwrap_or(0),
        8..=10 => match r.map(|r| r.kind) {
            Some(DetailValueKind::Number { min, max, step }) => {
                [min, max, step][(field - 8) as usize]
            }
            _ => -1,
        },
        11 => p.viewport_start as i64,
        12 => DETAIL_OPTION_ROWS as i64,
        _ => -1,
    })
}

pub fn text(id: i32, panel: Option<&DetailOptionsSnapshot>) -> Option<&str> {
    use bmz_skin_document::*;
    if let Some((slot, choice, field)) = detail_options_choice_slot(id) {
        return Some(
            panel
                .and_then(|p| p.row(slot))
                .and_then(|r| r.choices.get(choice))
                .filter(|_| choice < SKIN_DETAIL_OPTIONS_CHOICES && field == 0)
                .map_or("", |c| c.label.as_str()),
        );
    }
    if let Some((slot, field)) = slot(id) {
        return Some(
            panel
                .and_then(|p| p.row(slot))
                .map(|r| match field {
                    0 => r.label.as_str(),
                    1 => &r.value_label,
                    2 => &r.status,
                    _ => "",
                })
                .unwrap_or(""),
        );
    }
    if !(SKIN_TEXT_DETAIL_OPTIONS_BASE..=SKIN_TEXT_DETAIL_OPTIONS_LAST).contains(&id) {
        return None;
    }
    let Some(p) = panel else {
        return Some("");
    };
    let r = p.selected();
    Some(match id - SKIN_TEXT_DETAIL_OPTIONS_BASE {
        0 => r.map(|r| r.label.as_str()).unwrap_or(""),
        1 => r.map(|r| r.value_label.as_str()).unwrap_or(""),
        2 => r.map(|r| r.category.as_str()).unwrap_or(""),
        3 => r.map(|r| r.description.as_str()).unwrap_or(""),
        4 => r.map(|r| r.reason.as_str()).unwrap_or(""),
        5 => r.map(|r| r.auxiliary.as_str()).unwrap_or(""),
        6 => &p.title,
        7 => &p.scope_label,
        8 => &p.guide,
        9 => &p.position,
        _ => "",
    })
}

pub fn option(id: i32, panel: Option<&DetailOptionsSnapshot>) -> Option<bool> {
    use bmz_skin_document::*;
    if let Some((slot, choice, field)) = detail_options_choice_slot(id) {
        return Some(panel.and_then(|p| p.row(slot)).is_some_and(|r| {
            choice < SKIN_DETAIL_OPTIONS_CHOICES
                && r.choices.get(choice).is_some_and(|c| match field {
                    0 => true,
                    1 => r.value_index >= 0 && r.value == c.value,
                    2 => r.editable,
                    _ => false,
                })
        }));
    }
    if let Some((slot, field)) = slot(id) {
        return Some(panel.is_some_and(|p| {
            p.row(slot).is_some_and(|r| match field {
                0 => true,
                1 => slot == DETAIL_OPTION_CENTER,
                2 => r.editable,
                3 => r.effective,
                4 => r.kind == DetailValueKind::Enum && r.value_index < 0,
                5 => matches!(r.kind, DetailValueKind::Number { .. }),
                6 => {
                    r.editable
                        && matches!(r.kind, DetailValueKind::Number { min, .. } if r.value > min)
                }
                7 => {
                    r.editable
                        && matches!(r.kind, DetailValueKind::Number { max, .. } if r.value < max)
                }
                _ => false,
            })
        }));
    }
    if !(SKIN_OPTION_DETAIL_OPTIONS_BASE..=SKIN_OPTION_DETAIL_OPTIONS_LAST).contains(&id) {
        return None;
    }
    Some(panel.is_some_and(|p| match id - SKIN_OPTION_DETAIL_OPTIONS_BASE {
        0 => true,
        1 => p.selected().is_some_and(|r| r.editable),
        2 => p.selected().is_some_and(|r| r.effective),
        3 => p.selected().is_some_and(|r| r.kind == DetailValueKind::Bool),
        4 => p.selected().is_some_and(|r| r.kind == DetailValueKind::Enum),
        5 => p.selected().is_some_and(|r| matches!(r.kind, DetailValueKind::Number { .. })),
        _ => false,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel(count: usize, cursor: usize) -> DetailOptionsSnapshot {
        DetailOptionsSnapshot {
            cursor,
            viewport_start: detail_options_viewport(cursor, count),
            items: (0..count)
                .map(|i| DetailOptionRow {
                    item_id: 101 + i as i64 * 10,
                    category_id: 1,
                    scope: 7,
                    value: 1,
                    value_index: 1,
                    choice_count: 2,
                    choices: vec![
                        DetailOptionChoice { value: 0, label: "OFF".into() },
                        DetailOptionChoice { value: 1, label: "ON".into() },
                    ]
                    .into(),
                    kind: DetailValueKind::Bool,
                    label: format!("item {i}"),
                    value_label: "ON".into(),
                    category: "LANE".into(),
                    description: "description".into(),
                    reason: String::new(),
                    auxiliary: "amount".into(),
                    status: String::new(),
                    editable: true,
                    effective: false,
                })
                .collect(),
            title: "DETAIL OPTIONS".into(),
            scope_label: "7K".into(),
            guide: "guide".into(),
            position: format!("{} / {count}", cursor + 1),
        }
    }

    #[test]
    fn detail_rows_have_one_selection_and_keep_cursor_visible() {
        for count in [0, 1, 2, 3, 4, 5, 6, 7, 8, 15, 80] {
            for cursor in 0..count.max(1) {
                let p = panel(count, cursor);
                let valid =
                    (0..7).filter(|i| option(19400 + i * 10, Some(&p)) == Some(true)).count();
                let selected =
                    (0..7).filter(|i| option(19401 + i * 10, Some(&p)) == Some(true)).count();
                assert_eq!(valid, count.min(7));
                assert_eq!(selected, usize::from(count > 0));
                assert_eq!(number(19300, Some(&p)), Some(cursor as i64));
                assert_eq!(number(19301, Some(&p)), Some(count as i64));
                assert!(p.row(9).is_none());
                assert_eq!(p.row(7).is_some(), count > 7);
                assert_eq!(p.row(8).is_some(), count > 7);
                if count > 0 {
                    let slot = DETAIL_OPTION_CENTER;
                    assert_eq!(number(19302, Some(&p)), number(19400 + slot as i32 * 10, Some(&p)));
                    assert_eq!(text(19300, Some(&p)), text(19400 + slot as i32 * 10, Some(&p)));
                }
                let indices: std::collections::HashSet<_> = (0..7)
                    .filter_map(|slot| detail_options_row_index(cursor, count, slot))
                    .collect();
                assert_eq!(indices.len(), count.min(7));
            }
        }
    }

    #[test]
    fn detail_viewport_wraps_both_ends_without_moving_the_center() {
        for (cursor, expected) in [(0, [12, 13, 14, 0, 1, 2, 3]), (14, [11, 12, 13, 14, 0, 1, 2])] {
            assert_eq!(
                std::array::from_fn::<_, 7, _>(
                    |slot| detail_options_row_index(cursor, 15, slot).unwrap()
                ),
                expected
            );
        }
        assert_eq!(detail_options_row_index(0, 15, 7), Some(11));
        assert_eq!(detail_options_row_index(14, 15, 8), Some(3));
        let p = panel(15, 0);
        assert_eq!(number(19470, Some(&p)), Some(211));
        assert_eq!(option(19471, Some(&p)), Some(false));
        assert_eq!(option(19481, Some(&p)), Some(false));
    }

    #[test]
    fn detail_hidden_refs_and_invalid_rows_are_explicit() {
        assert_eq!(number(19300, None), Some(-1));
        assert_eq!(number(19301, None), Some(0));
        assert_eq!(number(19305, None), Some(-1));
        assert_eq!(text(19300, None), Some(""));
        assert_eq!(option(19300, None), Some(false));
        let p = panel(2, 0);
        assert_eq!(option(19420, Some(&p)), Some(false));
        assert_eq!(number(19420, Some(&p)), Some(-1));
        assert_eq!(text(19420, Some(&p)), Some(""));
        assert_eq!(option(19301, Some(&p)), Some(true));
        assert_eq!(option(19302, Some(&p)), Some(false));
        assert_eq!(option(19303, Some(&p)), Some(true));
        assert_eq!(number(19308, Some(&p)), Some(-1));
    }

    #[test]
    fn numeric_rows_publish_range_without_enum_external_value_state() {
        let mut p = panel(1, 0);
        let mut row = p.items[0].clone();
        row.kind = DetailValueKind::Number { min: -500, max: 500, step: 1 };
        row.value = -1;
        row.value_index = -1;
        row.choice_count = 0;
        row.choices = Vec::new().into();
        p.items = vec![row].into();
        assert_eq!(number(19305, Some(&p)), Some(-1));
        assert_eq!(option(19301, Some(&p)), Some(true));
        for (id, value) in [(19434, 0), (19435, -1), (19436, -500), (19437, 500), (19438, 1)] {
            assert_eq!(number(id, Some(&p)), Some(value));
        }
        assert_eq!(option(19434, Some(&p)), Some(false));
        assert_eq!(option(19435, Some(&p)), Some(true));
        assert_eq!(option(19436, Some(&p)), Some(true));
        assert_eq!(option(19437, Some(&p)), Some(true));
        assert_eq!(option(19692, Some(&p)), Some(false));
        for slot in 0..9 {
            for (offset, direction) in [(0, -1), (1, 1)] {
                let id = 19320 + slot * 2 + offset;
                assert_eq!(
                    bmz_skin_document::detail_options_numeric_event(id),
                    Some((slot as usize, direction))
                );
                assert!(bmz_skin_document::is_detail_options_event(id));
            }
        }
        assert!(bmz_skin_document::detail_options_numeric_event(19338).is_none());
    }
}
