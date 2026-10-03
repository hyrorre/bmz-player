use super::*;

pub(super) struct Processor {
    pub(super) ops: HashMap<i32, bool>,
    runtime_aliases: HashMap<i32, Vec<i32>>,
    stack: Vec<IfState>,
    pub(super) option_dependencies: BTreeMap<i32, bool>,
}

#[derive(Debug, Clone)]
struct IfState {
    parent_active: bool,
    branch_taken: bool,
    active: bool,
    runtime_ops: Vec<i32>,
    runtime_branches: Vec<Vec<i32>>,
}

impl Processor {
    pub(super) fn new(ops: HashMap<i32, bool>) -> Self {
        Self {
            ops,
            runtime_aliases: HashMap::new(),
            stack: Vec::new(),
            option_dependencies: BTreeMap::new(),
        }
    }

    pub(super) fn process_lines(
        &mut self,
        lines: &[CsvLine],
        current_path: &Path,
        builder: &mut CsvBuilder,
    ) -> Result<()> {
        for line in lines {
            if self.handle_control(line) {
                continue;
            }
            if !self.active() {
                continue;
            }
            if line.command == "SETOPTION" {
                let index = parse_i32(line.fields.get(1));
                let value = parse_i32(line.fields.get(2)) >= 1;
                let runtime_ops = self.active_runtime_ops();
                if runtime_ops.is_empty() {
                    self.ops.insert(index, value);
                    builder.header.selected_ops.insert(index, value);
                } else {
                    if value && !self.ops.get(&index.abs()).copied().unwrap_or(false) {
                        self.runtime_aliases.entry(index.abs()).or_insert(runtime_ops.clone());
                    }
                    builder.register_runtime_option_alias(index, value, &runtime_ops);
                }
                continue;
            }
            builder.conditional_ops = self.active_runtime_ops();
            if matches!(builder.header.skin_type, 7 | 15)
                && matches!(line.command.as_str(), "FLIPRESULT" | "DISABLEFLIP")
            {
                let flip = line.command == "FLIPRESULT";
                self.ops.insert(350, !flip);
                self.ops.insert(351, flip);
            }
            builder.apply_play_header_command(line);
            if line.command == "INCLUDE" {
                let include = resolve_include_path(builder, current_path, field(line, 1));
                if include.is_file() {
                    builder.record_loaded_file_dependency(&include);
                    let include_lines = read_csv_lines(&include)?;
                    self.process_lines(&include_lines, &include, builder)?;
                } else {
                    builder.warn(format!("lr2 include not found: {}", include.display()));
                }
                continue;
            }
            builder.execute(line)?;
        }
        builder.conditional_ops.clear();
        Ok(())
    }

    pub(super) fn should_execute(&mut self, line: &CsvLine) -> bool {
        if self.handle_control(line) {
            return false;
        }
        self.active()
    }

    pub(super) fn handle_control(&mut self, line: &CsvLine) -> bool {
        match line.command.as_str() {
            "IF" => {
                let parent_active = self.active();
                let eval = self.eval_if(line);
                let condition = parent_active && eval.matches;
                let runtime_ops = if condition { eval.runtime_ops } else { Vec::new() };
                let branch_taken = condition && runtime_ops.is_empty();
                let runtime_branches = if condition && !runtime_ops.is_empty() {
                    vec![runtime_ops.clone()]
                } else {
                    Vec::new()
                };
                self.stack.push(IfState {
                    parent_active,
                    branch_taken,
                    active: condition,
                    runtime_ops,
                    runtime_branches,
                });
                true
            }
            "ELSEIF" => {
                let Some(mut state) = self.stack.pop() else {
                    return true;
                };
                if !state.parent_active || state.branch_taken {
                    state.active = false;
                    state.runtime_ops.clear();
                } else {
                    let eval = self.eval_if(line);
                    if !eval.matches {
                        state.active = false;
                        state.runtime_ops.clear();
                    } else if state.runtime_branches.is_empty() {
                        state.active = true;
                        state.branch_taken = eval.runtime_ops.is_empty();
                        state.runtime_ops = eval.runtime_ops;
                        if !state.runtime_ops.is_empty() {
                            state.runtime_branches.push(state.runtime_ops.clone());
                        }
                    } else if let Some(mut previous) =
                        negate_runtime_branches(&state.runtime_branches)
                    {
                        let is_static_fallback = eval.runtime_ops.is_empty();
                        previous.extend(eval.runtime_ops.iter().copied());
                        state.active = true;
                        state.branch_taken = is_static_fallback;
                        state.runtime_ops = previous;
                        if !is_static_fallback {
                            state.runtime_branches.push(eval.runtime_ops);
                        }
                    } else {
                        state.active = false;
                        state.runtime_ops.clear();
                    }
                }
                self.stack.push(state);
                true
            }
            "ELSE" => {
                if let Some(state) = self.stack.last_mut() {
                    if !state.parent_active || state.branch_taken {
                        state.active = false;
                        state.runtime_ops.clear();
                    } else if !state.runtime_branches.is_empty() {
                        if let Some(ops) = negate_runtime_branches(&state.runtime_branches) {
                            state.active = true;
                            state.runtime_ops = ops;
                        } else {
                            state.active = false;
                            state.runtime_ops.clear();
                        }
                    } else {
                        state.active = true;
                        state.runtime_ops.clear();
                    }
                    state.branch_taken = true;
                }
                true
            }
            "ENDIF" => {
                self.stack.pop();
                true
            }
            _ => false,
        }
    }

    pub(super) fn active(&self) -> bool {
        self.stack.iter().all(|state| state.active)
    }

    pub(super) fn eval_if(&mut self, line: &CsvLine) -> IfEval {
        let mut runtime_ops = Vec::new();
        let ops = self.ops.clone();
        let dependencies = &mut self.option_dependencies;
        let matches =
            line.fields.iter().skip(1).filter(|field| !field.trim().is_empty()).all(|field| {
                let option = parse_option_token(field);
                let option_id = option.abs();
                if let Some(alias) = self.runtime_aliases.get(&option_id) {
                    if option >= 0 {
                        runtime_ops.extend(alias.iter().copied());
                        true
                    } else if alias.len() == 1 {
                        runtime_ops.push(-alias[0]);
                        true
                    } else {
                        false
                    }
                } else if matches!(option_id, 350 | 351) && ops.contains_key(&option_id) {
                    ops[&option_id] == (option >= 0)
                } else if is_runtime_lr2_option(option_id) {
                    runtime_ops.push(option);
                    true
                } else if let Some(enabled) = ops.get(&option_id).copied() {
                    dependencies.insert(option_id, enabled);
                    if option >= 0 { enabled } else { !enabled }
                } else {
                    dependencies.insert(option_id, false);
                    option < 0
                }
            });
        IfEval { matches, runtime_ops }
    }

    pub(super) fn active_runtime_ops(&self) -> Vec<i32> {
        self.stack
            .iter()
            .filter(|state| state.active)
            .flat_map(|state| state.runtime_ops.iter().copied())
            .collect()
    }
}
