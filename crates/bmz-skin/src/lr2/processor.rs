use super::*;

pub(super) struct Processor {
    pub(super) ops: HashMap<i32, bool>,
    skin_type: Option<i32>,
    runtime_aliases: HashMap<i32, Lr2Condition>,
    stack: Vec<IfState>,
    pub(super) option_dependencies: BTreeMap<i32, bool>,
    assigned_options: BTreeSet<i32>,
}

fn is_runtime_option(skin_type: Option<i32>, option_id: i32) -> bool {
    is_runtime_lr2_option(option_id)
        || skin_type.is_some_and(|skin_type| is_lr2_scene_runtime_option(skin_type, option_id))
}

/// One `#IF` level. As in LR2, each level is evaluated on its own condition only:
/// an `#IF` nested inside an inactive block still runs when its condition holds,
/// and returning from it with `#ENDIF` restores the enclosing level's state.
#[derive(Debug, Clone)]
struct IfState {
    branch_taken: bool,
    active: bool,
    runtime_ops: Vec<i32>,
    runtime_not_ops: Vec<Lr2Condition>,
    runtime_branches: Vec<Lr2Condition>,
}

impl Processor {
    pub(super) fn new(ops: HashMap<i32, bool>) -> Self {
        Self {
            ops,
            skin_type: None,
            runtime_aliases: HashMap::new(),
            stack: Vec::new(),
            option_dependencies: BTreeMap::new(),
            assigned_options: BTreeSet::new(),
        }
    }

    /// Also keeps the scene's song-dependent LR2 options for draw-time evaluation.
    pub(super) fn for_skin(ops: HashMap<i32, bool>, skin_type: i32) -> Self {
        Self { skin_type: Some(skin_type), ..Self::new(ops) }
    }

    pub(super) fn process_lines(
        &mut self,
        lines: &[CsvLine],
        current_path: &Path,
        builder: &mut CsvBuilder,
    ) -> Result<()> {
        for (command_index, line) in lines.iter().enumerate() {
            tracing::trace!(
                path = %current_path.display(),
                command_index = command_index + 1,
                command = %line.command,
                if_depth = self.stack.len(),
                "lr2 skin command begin"
            );
            if self.handle_control(line) {
                continue;
            }
            if !self.active() {
                continue;
            }
            if line.command == "SETOPTION" {
                let index = parse_i32(line.fields.get(1));
                let value = parse_i32(line.fields.get(2)) >= 1;
                let condition = self.active_runtime_condition();
                if condition.op.is_empty() && condition.not.is_empty() {
                    self.assign_option(index, value);
                    builder.header.selected_ops.insert(index, value);
                } else {
                    if value && !self.ops.get(&index.abs()).copied().unwrap_or(false) {
                        self.runtime_aliases.entry(index.abs()).or_insert(condition.clone());
                    }
                    builder.register_runtime_option_alias(index, value, &condition);
                }
                continue;
            }
            builder.conditional_ops = self.active_runtime_ops();
            builder.conditional_not_ops = self.active_runtime_condition().not;
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
                    builder.warn(format!(
                        "lr2 include not found: {} ({}: command {})",
                        include.display(),
                        current_path.display(),
                        command_index + 1
                    ));
                }
                continue;
            }
            let warning_start = builder.warnings.len();
            builder.execute(line)?;
            for warning in &mut builder.warnings[warning_start..] {
                warning.message = format!(
                    "{} ({}: command {})",
                    warning.message,
                    current_path.display(),
                    command_index + 1
                );
            }
        }
        builder.conditional_ops.clear();
        builder.conditional_not_ops.clear();
        Ok(())
    }

    pub(super) fn should_execute(&mut self, line: &CsvLine) -> bool {
        if self.handle_control(line) {
            return false;
        }
        self.active()
    }

    pub(super) fn assign_option(&mut self, index: i32, value: bool) {
        self.ops.insert(index, value);
        self.assigned_options.insert(index);
    }

    pub(super) fn handle_control(&mut self, line: &CsvLine) -> bool {
        match line.command.as_str() {
            "IF" => {
                let eval = self.eval_if(line);
                let condition = eval.matches;
                let runtime_ops = if condition { eval.runtime_ops } else { Vec::new() };
                let runtime_not_ops = if condition { eval.runtime_not_ops } else { Vec::new() };
                let branch_taken =
                    condition && runtime_ops.is_empty() && runtime_not_ops.is_empty();
                let runtime_branches = if condition && !branch_taken {
                    vec![Lr2Condition { op: runtime_ops.clone(), not: runtime_not_ops.clone() }]
                } else {
                    Vec::new()
                };
                self.stack.push(IfState {
                    branch_taken,
                    active: condition,
                    runtime_ops,
                    runtime_not_ops,
                    runtime_branches,
                });
                true
            }
            "ELSEIF" => {
                let Some(mut state) = self.stack.pop() else {
                    return true;
                };
                if state.branch_taken {
                    state.active = false;
                    state.runtime_ops.clear();
                    state.runtime_not_ops.clear();
                } else {
                    let eval = self.eval_if(line);
                    if !eval.matches {
                        state.active = false;
                        state.runtime_ops.clear();
                        state.runtime_not_ops.clear();
                    } else {
                        let is_static_fallback =
                            eval.runtime_ops.is_empty() && eval.runtime_not_ops.is_empty();
                        let mut previous = negate_runtime_branches(&state.runtime_branches);
                        previous.op.extend(eval.runtime_ops.iter().copied());
                        previous.not.extend(eval.runtime_not_ops.clone());
                        state.active = true;
                        state.branch_taken = is_static_fallback;
                        state.runtime_ops = previous.op;
                        state.runtime_not_ops = previous.not;
                        if !is_static_fallback {
                            state.runtime_branches.push(Lr2Condition {
                                op: eval.runtime_ops,
                                not: eval.runtime_not_ops,
                            });
                        }
                    }
                }
                self.stack.push(state);
                true
            }
            "ELSE" => {
                if let Some(state) = self.stack.last_mut() {
                    if state.branch_taken {
                        state.active = false;
                        state.runtime_ops.clear();
                        state.runtime_not_ops.clear();
                    } else if !state.runtime_branches.is_empty() {
                        let condition = negate_runtime_branches(&state.runtime_branches);
                        state.active = true;
                        state.runtime_ops = condition.op;
                        state.runtime_not_ops = condition.not;
                    } else {
                        state.active = true;
                        state.runtime_ops.clear();
                        state.runtime_not_ops.clear();
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
        self.stack.last().is_none_or(|state| state.active)
    }

    pub(super) fn eval_if(&mut self, line: &CsvLine) -> IfEval {
        let mut runtime_ops = Vec::new();
        let mut runtime_not_ops = Vec::new();
        let ops = self.ops.clone();
        let skin_type = self.skin_type;
        let dependencies = &mut self.option_dependencies;
        let assigned = &self.assigned_options;
        let matches =
            line.fields.iter().skip(1).filter(|field| !field.trim().is_empty()).all(|field| {
                let option = parse_option_token(field);
                let option_id = option.abs();
                if let Some(alias) = self.runtime_aliases.get(&option_id) {
                    if option >= 0 {
                        runtime_ops.extend(alias.op.iter().copied());
                        runtime_not_ops.extend(alias.not.clone());
                        true
                    } else if alias.op.len() == 1 && alias.not.is_empty() {
                        runtime_ops.push(-alias.op[0]);
                        true
                    } else {
                        runtime_not_ops.push(alias.clone());
                        true
                    }
                } else if matches!(option_id, 350 | 351) && ops.contains_key(&option_id) {
                    ops[&option_id] == (option >= 0)
                } else if skin_type.is_some_and(|kind| is_lr2_load_time_option(kind, option_id))
                    && let Some(enabled) = ops.get(&option_id).copied().or_else(|| {
                        // Result is a finished snapshot. An absent property is
                        // false, just as in LR2, rather than an alternative asset
                        // branch that can change the subsequent IMAGE indices.
                        skin_type.filter(|kind| matches!(kind, 7 | 15)).map(|_| false)
                    })
                {
                    if !assigned.contains(&option_id) {
                        dependencies.insert(option_id, enabled);
                    }
                    enabled == (option >= 0)
                } else if is_runtime_option(skin_type, option_id) {
                    runtime_ops.push(option);
                    true
                } else if let Some(enabled) = ops.get(&option_id).copied() {
                    if !assigned.contains(&option_id) {
                        dependencies.insert(option_id, enabled);
                    }
                    if option >= 0 { enabled } else { !enabled }
                } else {
                    if !assigned.contains(&option_id) {
                        dependencies.insert(option_id, false);
                    }
                    option < 0
                }
            });
        IfEval { matches, runtime_ops, runtime_not_ops }
    }

    pub(super) fn active_runtime_ops(&self) -> Vec<i32> {
        self.stack
            .last()
            .filter(|state| state.active)
            .map(|state| state.runtime_ops.clone())
            .unwrap_or_default()
    }

    pub(super) fn active_runtime_condition(&self) -> Lr2Condition {
        self.stack
            .last()
            .filter(|state| state.active)
            .map(|state| Lr2Condition {
                op: state.runtime_ops.clone(),
                not: state.runtime_not_ops.clone(),
            })
            .unwrap_or_default()
    }
}
