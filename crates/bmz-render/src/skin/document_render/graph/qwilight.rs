use crate::skin::*;

pub(in crate::skin::document_render) fn qwilight_visualizer_items(
    document: &SkinDocument,
    visualizer: &SkinQwilightVisualizerDef,
    destination: &SkinDestinationDef,
    frame: ResolvedSkinFrame,
    state: &SkinDrawState,
    graphs: Option<&crate::snapshot::PlayNoteGraphs>,
) -> Vec<SkinRenderItem> {
    let rect = normalize_skin_frame_rect(frame, document.w, document.h);
    if rect.width <= 0.0 || rect.height <= 0.0 {
        return Vec::new();
    }
    let mut items = Vec::new();
    let mut push = |x: f32, y: f32, w: f32, h: f32, color: Color| {
        if w > 0.0 && h > 0.0 {
            items.push(SkinRenderItem::Rect {
                rect: Rect {
                    x: rect.x + x * rect.width,
                    y: rect.y + y * rect.height,
                    width: w * rect.width,
                    height: h * rect.height,
                },
                color: color.with_alpha(color.a * frame.a as f32 / 255.0),
                blend: skin_blend_mode(destination.blend),
            });
        }
    };
    match visualizer.kind {
        QwilightVisualizerKind::Spectrum => {
            for (i, value) in state.audio_spectrum.iter().enumerate() {
                let height = if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 };
                push(
                    i as f32 / 32.0,
                    1.0 - height,
                    1.0 / 32.0,
                    height,
                    Color::rgba(1.0, 1.0, 1.0, 0.1),
                );
            }
        }
        QwilightVisualizerKind::Timing => {
            let areas = timing_judge_areas(state);
            let low = areas[3].late_ms.min(-1.0);
            let high = areas[3].early_ms.max(1.0);
            let w = (visualizer.line_width.max(1) as f32 / frame.w.abs().max(1) as f32).min(1.0);
            let h = (visualizer.line_height.max(1) as f32 / frame.h.abs().max(1) as f32).min(1.0);
            for &(ms, age, judge) in &state.qwilight_timing {
                if !(0..800).contains(&age) || !ms.is_finite() {
                    continue;
                }
                let position = ((ms - low) / (high - low)).clamp(0.0, 1.0);
                let (x, y) = match visualizer.system {
                    0 => (0.0, (1.0 - position) * (1.0 - h)),
                    1 => (0.0, position * (1.0 - h)),
                    2 => ((1.0 - position) * (1.0 - w), 0.0),
                    _ => (position * (1.0 - w), 0.0),
                };
                push(x, y, w, h, judge_color(judge).with_alpha(1.0 - age as f32 / 800.0));
            }
        }
        QwilightVisualizerKind::Judgments => {
            if let Some(graphs) = graphs {
                // Reuse BMZ's judged-note history, resampled to Qwilight's 100 columns.
                // Unjudged notes (bucket 0) must not appear as successful inputs.
                let mut bins = [[0_u64; 5]; 100];
                for (i, bucket) in graphs.judge.iter().enumerate() {
                    let bin = i * 100 / graphs.judge.len().max(1);
                    for (to, from) in bins[bin.min(99)].iter_mut().zip(&bucket.values[1..]) {
                        *to += u64::from(*from);
                    }
                }
                let max =
                    bins.iter().map(|b| b.iter().sum::<u64>()).max().unwrap_or(0).max(1) as f32;
                for (i, bin) in bins.iter().enumerate() {
                    let mut y = 1.0;
                    for (judge, count) in bin.iter().enumerate().rev() {
                        let h = *count as f32 / max;
                        y -= h;
                        push(i as f32 / 100.0, y, 0.01, h, judge_color(judge));
                    }
                }
            }
        }
    }
    items
}

fn judge_color(index: usize) -> Color {
    match index {
        0 => Color::rgba(0.0, 1.0, 1.0, 1.0),
        1 => Color::rgba(1.0, 1.0, 1.0, 1.0),
        2 => Color::rgba(1.0, 1.0, 0.0, 1.0),
        3 => Color::rgba(0.0, 1.0, 0.0, 1.0),
        4 => Color::rgba(1.0, 0.5, 0.0, 1.0),
        _ => Color::rgba(1.0, 0.0, 0.0, 1.0),
    }
}
