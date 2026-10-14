use super::*;

/// Canvas sizes that LR2 skins draw full-screen backgrounds and panels at.
const LR2_INFERABLE_RESOLUTIONS: [(u32, u32); 4] =
    [(640, 480), (1280, 720), (1920, 1080), (3840, 2160)];
const LR2_RESOLUTION_INCLUDE_DEPTH: usize = 16;

impl<'a> CsvBuilder<'a> {
    /// Infers the canvas of a skin that omits `#RESOLUTION`.
    ///
    /// HD themes such as EndlessCirculation and W-mix HD were written for LR2 HD builds
    /// and rarely declare their resolution. They draw backgrounds and panels as
    /// `0,0,W,H` destinations, so the most frequent full-screen size identifies the
    /// canvas. Skins without such destinations keep the LR2 default of 640x480.
    pub(super) fn infer_lr2_resolution(
        &mut self,
        lines: &[CsvLine],
        path: &Path,
        ops: &HashMap<i32, bool>,
    ) -> Option<(u32, u32)> {
        let mut counts = [0_usize; LR2_INFERABLE_RESOLUTIONS.len()];
        let mut processor = Processor::for_skin(ops.clone(), self.header.skin_type);
        self.count_full_screen_destinations(lines, path, &mut processor, &mut counts, 0);
        let (index, count) = counts
            .iter()
            .copied()
            .enumerate()
            .fold((0, 0), |best, entry| if entry.1 > best.1 { entry } else { best });
        (count > 0).then_some(LR2_INFERABLE_RESOLUTIONS[index])
    }

    fn count_full_screen_destinations(
        &mut self,
        lines: &[CsvLine],
        current_path: &Path,
        processor: &mut Processor,
        counts: &mut [usize; LR2_INFERABLE_RESOLUTIONS.len()],
        depth: usize,
    ) {
        for line in lines {
            if !processor.should_execute(line) {
                continue;
            }
            if line.command == "SETOPTION" {
                let condition = processor.active_runtime_condition();
                if condition.op.is_empty() && condition.not.is_empty() {
                    processor.assign_option(
                        parse_i32(line.fields.get(1)),
                        parse_i32(line.fields.get(2)) >= 1,
                    );
                }
                continue;
            }
            if line.command == "INCLUDE" {
                if depth >= LR2_RESOLUTION_INCLUDE_DEPTH {
                    continue;
                }
                let include = resolve_include_path(self, current_path, field(line, 1));
                let Ok(include_lines) = read_csv_lines(&include) else {
                    continue;
                };
                self.record_loaded_file_dependency(&include);
                let parent_stack = processor.begin_include_file();
                self.count_full_screen_destinations(
                    &include_lines,
                    &include,
                    processor,
                    counts,
                    depth + 1,
                );
                processor.end_include_file(parent_stack);
                continue;
            }
            if !line.command.starts_with("DST_")
                || parse_i32(line.fields.get(3)) != 0
                || parse_i32(line.fields.get(4)) != 0
            {
                continue;
            }
            let size = (parse_i32(line.fields.get(5)), parse_i32(line.fields.get(6)));
            if let Some(index) =
                LR2_INFERABLE_RESOLUTIONS.iter().position(|&(w, h)| (w as i32, h as i32) == size)
            {
                counts[index] += 1;
            }
        }
    }
}
