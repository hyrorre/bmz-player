use super::*;

#[derive(Debug)]
pub(in crate::app) enum SelectListAction {
    Enter(String),
    Refresh,
    Exit,
}

pub(in crate::app) struct PreparedSelectList {
    pub folder_stack: Vec<String>,
    pub selected_index_stack: Vec<usize>,
    pub selected_index: usize,
    pub selected_key: Option<SelectItemKey>,
    pub items: Vec<SelectItem>,
    pub mode_filter: SelectModeFilter,
    pub left_empty_folder: bool,
}

/// Prepare navigation without changing the visible list or its history. An empty
/// destination rejects entry; refresh/exit instead walk up to the nearest usable
/// parent. Each candidate is loaded once, and errors leave the current view intact.
pub(in crate::app) fn prepare_select_list(
    action: &SelectListAction,
    folder_stack: &[String],
    selected_index_stack: &[usize],
    selected_index: usize,
    selected_key: Option<SelectItemKey>,
    mut load: impl FnMut(&[String]) -> Result<(Vec<SelectItem>, SelectModeFilter)>,
) -> Result<Option<PreparedSelectList>> {
    let mut stack = folder_stack.to_vec();
    let mut indices = selected_index_stack.to_vec();
    let mut index = selected_index;
    let mut key = selected_key;
    match action {
        SelectListAction::Enter(path) => {
            stack.push(path.clone());
            indices.push(index);
            index = 0;
            key = None;
        }
        SelectListAction::Exit => {
            let Some(path) = stack.pop() else {
                return Ok(None);
            };
            index = indices.pop().unwrap_or(0);
            key = Some(SelectItemKey::Folder(path));
        }
        SelectListAction::Refresh => {}
    }
    let mut left_empty_folder = false;
    loop {
        let (items, mode_filter) = load(&stack)?;
        if !items.is_empty() || stack.is_empty() {
            return Ok(Some(PreparedSelectList {
                folder_stack: stack,
                selected_index_stack: indices,
                selected_index: restored_select_index(&items, key.as_ref(), index),
                selected_key: key,
                items,
                mode_filter,
                left_empty_folder,
            }));
        }
        if matches!(action, SelectListAction::Enter(_)) {
            return Ok(None);
        }
        key = stack.pop().map(SelectItemKey::Folder);
        index = indices.pop().unwrap_or(0);
        left_empty_folder = true;
    }
}
