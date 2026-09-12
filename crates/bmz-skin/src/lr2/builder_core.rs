use super::*;

impl<'a> CsvBuilder<'a> {
    pub(super) fn new(path: &'a Path, header: Header, files: &'a BTreeMap<String, String>) -> Self {
        let skin_root = infer_skin_root(path);
        let skin_file_dir = path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
        let remap_single_play_2p_lanes = matches!(header.skin_type, 0 | 1 | 3 | 4 | 12 | 13)
            && header.selected_ops.get(&901).copied().unwrap_or(false);
        Self {
            skin_root,
            skin_file_dir,
            skin_file_dir_name: path
                .parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                .map(str::to_string),
            header,
            files,
            warnings: Vec::new(),
            sources: Vec::new(),
            source_paths: Vec::new(),
            fonts: Vec::new(),
            lr2font_ids: Vec::new(),
            images: Vec::new(),
            imagesets: Vec::new(),
            lr2_imagesets: Vec::new(),
            values: Vec::new(),
            texts: Vec::new(),
            sliders: Vec::new(),
            graphs: Vec::new(),
            judge_graphs: Vec::new(),
            bpm_graphs: Vec::new(),
            timing_visualizers: Vec::new(),
            special_destination_sizes: HashMap::new(),
            hidden_covers: Vec::new(),
            gauge: None,
            gauges: Vec::new(),
            note: NoteState::default(),
            judges: Vec::new(),
            bga: None,
            destinations: Vec::new(),
            current: None,
            conditional_ops: Vec::new(),
            runtime_option_aliases: HashMap::new(),
            // Unspecified BGA stretch inherits the profile, as in beatoraja.
            stretch: -1,
            lr2_gauge_id: None,
            lr2_gauge_add_x: 0,
            lr2_gauge_add_y: 0,
            current_has_destination: false,
            note_marker_inserted: false,
            next_id: 0,
            remap_single_play_2p_lanes,
            file_dependencies: BTreeSet::new(),
            loaded_file_dependencies: BTreeMap::new(),
        }
    }

    pub(super) fn load_time_option_dependencies(&self) -> BTreeMap<i32, bool> {
        let mut dependencies = BTreeMap::new();
        if matches!(self.header.skin_type, 0 | 1 | 3 | 4 | 12 | 13) {
            dependencies.insert(901, self.header.selected_ops.get(&901).copied().unwrap_or(false));
        }
        if self.header.selected_ops.contains_key(&981) {
            dependencies.insert(981, self.header.selected_ops.get(&981).copied().unwrap_or(false));
        }
        dependencies
    }

    pub(super) fn internal_enabled_options(&self) -> Vec<i32> {
        let property_options = self
            .header
            .options
            .iter()
            .flat_map(|option| (0..option.items.len()).map(move |index| option.base + index as i32))
            .collect::<BTreeSet<_>>();
        let mut options = self
            .header
            .selected_ops
            .iter()
            .filter_map(|(&op, &enabled)| {
                (enabled && !property_options.contains(&op)).then_some(op)
            })
            .collect::<Vec<_>>();
        options.sort_unstable();
        options
    }

    pub(super) fn record_loaded_file_dependency(&mut self, path: &Path) {
        let Ok(metadata) = fs::metadata(path) else {
            return;
        };
        let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.loaded_file_dependencies.insert(
            path,
            SkinLoadedFileDependency { modified: metadata.modified().ok(), len: metadata.len() },
        );
    }

    pub(super) fn execute(&mut self, line: &CsvLine) -> Result<()> {
        match line.command.as_str() {
            "IMAGE" => self.add_source(field(line, 1)),
            "FONT" => self.add_system_font(line),
            "LR2FONT" => self.add_lr2_font(field(line, 1)),
            "SRC_IMAGE" | "SRC_BUTTON" => self.add_image(line),
            "IMAGESET" => self.add_imageset_source(line),
            "SRC_IMAGESET" => self.add_imageset(line),
            "DST_IMAGE" | "DST_BUTTON" => self.add_destination(line),
            "SRC_NUMBER" => self.add_number(line),
            "DST_NUMBER" => self.add_destination(line),
            "SRC_TEXT" => self.add_text(line),
            "DST_TEXT" => self.add_destination(line),
            "SRC_SLIDER" => self.add_slider(line, false),
            "SRC_SLIDER_REFNUMBER" => self.add_slider(line, true),
            "DST_SLIDER" => self.add_destination(line),
            "SRC_BARGRAPH" => self.add_graph(line, false),
            "SRC_BARGRAPH_REFNUMBER" => self.add_graph(line, true),
            "DST_BARGRAPH" => self.add_destination(line),
            "SRC_NOTECHART_1P" => self.add_note_chart(line),
            "DST_NOTECHART_1P" => self.add_destination(line),
            "SRC_BPMCHART" => self.add_bpm_chart(line),
            "DST_BPMCHART" => self.add_destination(line),
            "SRC_TIMING_1P" => self.add_timing_visualizer(line),
            "DST_TIMING_1P" => self.add_destination(line),
            "SRC_GROOVEGAUGE" | "SRC_GROOVEGAUGE_EX" => self.add_gauge(line),
            "DST_GROOVEGAUGE" => self.add_destination(line),
            "SRC_LINE" => self.add_line_source(line),
            "DST_LINE" => self.add_line_destination(line),
            "SRC_JUDGELINE" => self.add_image(line),
            "DST_JUDGELINE" => self.add_destination_with_default_offsets(line, &[LR2_OFFSET_LIFT]),
            "SRC_BGA" => self.add_bga(),
            "DST_BGA" => self.add_destination(line),
            "SRC_NOTE" | "SRC_AUTO_NOTE" => self.add_note_source(line, NoteSlot::Note),
            "SRC_LN_START" | "SRC_AUTO_LN_START" => self.add_note_source(line, NoteSlot::LnStart),
            "SRC_LN_END" | "SRC_AUTO_LN_END" => self.add_note_source(line, NoteSlot::LnEnd),
            "SRC_LN_BODY" | "SRC_AUTO_LN_BODY" => {
                // beatoraja registers the inactive LN body without animation, then
                // keeps the animated variant for the currently-held LN body.
                self.add_note_source_with_animation(line, NoteSlot::LnBody, false);
                self.add_note_source(line, NoteSlot::LnBodyActive);
            }
            "SRC_LN_BODY_INACTIVE" => self.add_note_source(line, NoteSlot::LnBody),
            "SRC_LN_BODY_ACTIVE" => self.add_note_source(line, NoteSlot::LnBodyActive),
            "SRC_HCN_START" => self.add_note_source(line, NoteSlot::HcnStart),
            "SRC_HCN_END" => self.add_note_source(line, NoteSlot::HcnEnd),
            "SRC_HCN_BODY" => {
                self.add_note_source(line, NoteSlot::HcnBody);
                self.add_note_source(line, NoteSlot::HcnActive);
            }
            "SRC_HCN_BODY_INACTIVE" => self.add_note_source(line, NoteSlot::HcnBody),
            "SRC_HCN_BODY_ACTIVE" => self.add_note_source(line, NoteSlot::HcnActive),
            "SRC_HCN_DAMAGE" => self.add_note_source(line, NoteSlot::HcnDamage),
            "SRC_HCN_REACTIVE" => self.add_note_source(line, NoteSlot::HcnReactive),
            "SRC_MINE" | "SRC_AUTO_MINE" => self.add_note_source(line, NoteSlot::Mine),
            "DST_NOTE" => self.add_note_destination(line),
            "DST_NOTE2" => self.note.dst2 = Some(parse_i32(line.fields.get(1))),
            "DST_NOTE_EXPANSION_RATE" => {
                self.note.expansion_rate =
                    Some([parse_i32(line.fields.get(1)), parse_i32(line.fields.get(2))]);
            }
            "SRC_NOWJUDGE_1P" => self.add_judge_image(line, 0),
            "DST_NOWJUDGE_1P" => self.add_judge_image_destination(line, 0),
            "SRC_NOWJUDGE_2P" => self.add_judge_image(line, 1),
            "DST_NOWJUDGE_2P" => self.add_judge_image_destination(line, 1),
            "SRC_NOWJUDGE_3P" => self.add_judge_image(line, 2),
            "DST_NOWJUDGE_3P" => self.add_judge_image_destination(line, 2),
            "SRC_NOWCOMBO_1P" => self.add_judge_number(line, 0),
            "DST_NOWCOMBO_1P" => self.add_judge_number_destination(line, 0),
            "SRC_NOWCOMBO_2P" => self.add_judge_number(line, 1),
            "DST_NOWCOMBO_2P" => self.add_judge_number_destination(line, 1),
            "SRC_NOWCOMBO_3P" => self.add_judge_number(line, 2),
            "DST_NOWCOMBO_3P" => self.add_judge_number_destination(line, 2),
            "SRC_HIDDEN" => self.add_hidden_cover(line),
            "DST_HIDDEN" => self.add_destination(line),
            "SRC_LIFT" => self.add_lift_cover(line),
            "DST_LIFT" => self.add_destination(line),
            "STARTINPUT" | "SCENETIME" | "JUDGETIMER" => {}
            "STRETCH" => self.stretch = parse_i32(line.fields.get(1)),
            "FADEOUT" | "CLOSE" | "LOADSTART" | "LOADEND" | "PLAYSTART" | "FINISHMARGIN" => {}
            "TRANSCLOLR" | "SCRATCHSIDE" | "ENDOFHEADER" => {}
            other if other.starts_with("DST_") || other.starts_with("SRC_") => {
                self.warn(format!("unsupported lr2 csv command: #{other}"));
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn apply_play_header_command(&mut self, line: &CsvLine) {
        match line.command.as_str() {
            "FADEOUT" => self.header.fadeout = parse_i32(line.fields.get(1)),
            "STARTINPUT" => self.header.input = parse_i32(line.fields.get(1)),
            "SCENETIME" => self.header.scene = parse_i32(line.fields.get(1)),
            "CLOSE" => self.header.close = parse_i32(line.fields.get(1)),
            "LOADSTART" => self.header.loadstart = parse_i32(line.fields.get(1)),
            "LOADEND" => self.header.loadend = parse_i32(line.fields.get(1)),
            "PLAYSTART" => self.header.playstart = parse_i32(line.fields.get(1)),
            "JUDGETIMER" => self.header.judgetimer = parse_i32(line.fields.get(1)),
            "FINISHMARGIN" => self.header.finishmargin = parse_i32(line.fields.get(1)),
            _ => {}
        }
    }
}
