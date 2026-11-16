use super::*;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq)]
enum FieldValue {
    Rank(usize),
    Favorite(usize),
    Number(i64),
    Rate(u32, u32),
}

fn field_value(
    field: SongListField,
    row: &SelectRowSnapshot,
    rival_selected: bool,
) -> Option<FieldValue> {
    use SongListField::*;
    if row.is_folder || row.kind != SelectRowKind::Song {
        return None;
    }
    if field == Favorite {
        return Some(FieldValue::Favorite(
            usize::from(row.favorite_chart) + 2 * usize::from(row.favorite_song),
        ));
    }
    if !row.in_library {
        return None;
    }
    if !row.numeric_score_available && field != Rivalscorerank {
        return None;
    }
    let grade = || ScoreGradeFacts::new(row.ex_score?, row.total_notes);
    Some(match field {
        Scorerank => FieldValue::Rank(grade()?.current_index),
        Nextrank => FieldValue::Rank(grade()?.next_index),
        Nearestrank => FieldValue::Rank(grade()?.nearest_index),
        Nextrankdiff => FieldValue::Number(grade()?.next_diff),
        Nearestrankdiff => FieldValue::Number(grade()?.nearest_diff),
        Exscore => FieldValue::Number(i64::from(row.ex_score?)),
        Scorerate => {
            if row.total_notes == 0 {
                return None;
            }
            FieldValue::Rate(row.ex_score?, row.total_notes)
        }
        Minbp => FieldValue::Number(i64::from(row.bp?)),
        Mincb => FieldValue::Number(i64::from(row.cb?)),
        Maxcombo => FieldValue::Number(i64::from(row.max_combo?)),
        Rivalscorerank => {
            if !rival_selected {
                return None;
            }
            FieldValue::Rank(
                ScoreGradeFacts::new(row.rival_ex_score?, row.total_notes)?.current_index,
            )
        }
        Rivaldiff => {
            if !rival_selected {
                return None;
            }
            FieldValue::Number(i64::from(row.ex_score?) - i64::from(row.rival_ex_score?))
        }
        Favorite => unreachable!(),
    })
}

impl FieldValue {
    fn state(self) -> Option<usize> {
        match self {
            Self::Rank(i) | Self::Favorite(i) => Some(i),
            _ => None,
        }
    }

    fn formatted(self, format: &SongListFieldFormat, labels: &[String]) -> String {
        use std::fmt::Write;
        let mut output = String::with_capacity(32 + format.prefix.len() + format.suffix.len());
        output.push_str(&format.prefix);
        match self {
            Self::Rank(index) => output.push_str(SCORE_GRADE_LABELS[index]),
            Self::Favorite(index) => output.push_str(
                labels
                    .get(index)
                    .map(String::as_str)
                    .unwrap_or(["", "CHART", "SONG", "CHART+SONG"][index]),
            ),
            Self::Number(number) => {
                if format.show_plus && number >= 0 {
                    output.push('+');
                }
                write!(output, "{number}").unwrap();
            }
            Self::Rate(score, notes) => {
                let decimals = format.decimals.unwrap_or(2).min(6);
                let scale = 10_u64.pow(u32::from(decimals));
                let scaled = u64::from(score) * 100 * scale / (u64::from(notes) * 2);
                if format.show_plus {
                    output.push('+');
                }
                write!(output, "{}", scaled / scale).unwrap();
                if decimals > 0 {
                    write!(output, ".{:0width$}", scaled % scale, width = usize::from(decimals))
                        .unwrap();
                }
            }
        }
        if format.percent.unwrap_or(matches!(self, Self::Rate(..))) {
            output.push('%');
        }
        output.push_str(&format.suffix);
        output
    }
}

/// Explicit glyph maps are for custom sheets; 24-cell sheets additionally
/// retain the conventional positive/negative twelve-cell halves.
fn digit_cells(
    value: &SkinValueDef,
    glyphs: &SongListDigitGlyphs,
    content: &str,
) -> Option<Vec<u32>> {
    let cells = value.divx.max(1).checked_mul(value.divy.max(1))? as u32;
    let stride = if value.lr2_number { lr2_number_frame_cells(value)? as u32 } else { cells };
    let signed = stride == 24;
    let negative = content.starts_with('-');
    let offset = if signed && negative && glyphs.minus.is_none() { 12 } else { 0 };
    let mut result = Vec::with_capacity(content.len());
    for ch in content.chars() {
        let cell = match ch {
            '0'..='9' => ch as u32 - '0' as u32 + offset,
            '+' => glyphs.plus.or(signed.then_some(11))?,
            '-' => glyphs.minus.or(signed.then_some(23))?,
            '.' => glyphs.decimal?,
            '%' => glyphs.percent?,
            _ => return None,
        };
        if cell >= stride {
            return None;
        }
        result.push(cell);
    }
    let width = value.digit.max(0) as usize;
    // Never discard a sign, decimal or high digit to fit the declared width.
    if result.len() > width {
        return None;
    }
    let padding = number_padding(value);
    if matches!(padding, NumberPadding::Zero) {
        let sign = usize::from(content.starts_with(['+', '-']));
        result.splice(sign..sign, std::iter::repeat_n(offset, width - result.len()));
    }
    Some(result)
}

pub(super) fn render_fields(
    document: &SkinDocument,
    songlist: &SkinSongListDef,
    row: &SelectRowSnapshot,
    origin: (i32, i32),
    rival_selected: bool,
    sources: &HashMap<String, SkinDocumentTexture>,
    images: &SkinImageLookup<'_>,
    options: &[i32],
    state: &SkinDrawState,
) -> Vec<SkinRenderItem> {
    let mut items = Vec::new();
    for &field in &songlist.score_order {
        let Some(value) = field_value(field, row, rival_selected) else {
            continue;
        };
        for entry in songlist.score_fields(field) {
            let dst = &entry.destination;
            if !destination_ops_match(dst, options, state)
                || !eval_skin_draw_condition(&dst.draw, state)
            {
                continue;
            }
            let Some(elapsed) = destination_timer_elapsed_ms(dst, state) else {
                continue;
            };
            let Some(mut frame) = resolve_destination_frame(dst, elapsed, options, state) else {
                continue;
            };
            frame.x += origin.0;
            frame.y += origin.1;
            apply_skin_offset_to_frame(dst, &mut frame, state, false);
            let clip = frame.take_clip(document.w, document.h);
            let resolved = match &entry.render {
                SongListFieldRender::Image { images: states } => {
                    let Some(id) = value.state().and_then(|index| states.get(index)) else {
                        continue;
                    };
                    let Some(image) = skin_image_for_destination_id(id, images) else {
                        continue;
                    };
                    let Some(source) = resolve_document_source(sources, &image.src) else {
                        continue;
                    };
                    let uv = skin_image_texture_region_for_destination(
                        image,
                        dst,
                        source.source_size,
                        state,
                        skin_image_pixel_rect(image),
                    );
                    let (rect, uv) = stretch_skin_image_geometry(
                        dst.stretch,
                        normalize_skin_frame_rect(frame, document.w, document.h),
                        uv,
                        source.source_size,
                        document.w,
                        document.h,
                    );
                    vec![skin_image_item_for_frame(
                        source.texture,
                        rect,
                        uv,
                        frame,
                        dst.center,
                        skin_blend_mode(dst.blend),
                        Some(source.source_size),
                        dst.filter != 0,
                    )]
                }
                SongListFieldRender::Text { text, labels } => {
                    let mut content = value.formatted(&entry.format, labels);
                    if !text.prefix.is_empty() {
                        content.insert_str(0, &text.prefix);
                    }
                    content.push_str(&text.suffix);
                    document
                        .text_content_render_item(
                            text,
                            frame,
                            Some(state),
                            &SkinTextState::default(),
                            content,
                        )
                        .into_iter()
                        .collect()
                }
                SongListFieldRender::Digits { digits, glyphs } => {
                    if value.state().is_some() {
                        continue;
                    }
                    let content = value.formatted(&entry.format, &[]);
                    let Some(cells) = digit_cells(digits, glyphs, &content) else {
                        continue;
                    };
                    let Some(source) = sources.get(&digits.src) else {
                        continue;
                    };
                    let Some(animation_time) = skin_timer_elapsed_ms(digits.timer, state) else {
                        continue;
                    };
                    super::number_cells::render(
                        document,
                        digits,
                        &cells,
                        super::number_cells::Layout {
                            max_digits: digits.digit.max(0) as usize,
                            align: digits.align,
                            center: dst.center,
                            blend: skin_blend_mode(dst.blend),
                            filter: dst.filter != 0,
                            hidden_cell: None,
                        },
                        ResolvedSkinFrame::default(),
                        frame,
                        if digits.timer.is_some() { animation_time } else { elapsed },
                        source,
                    )
                }
            };
            items.extend(wrap_skin_destination_clip(resolved, clip));
        }
    }
    items
}
