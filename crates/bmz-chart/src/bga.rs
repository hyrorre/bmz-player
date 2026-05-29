use bmz_core::time::TimeUs;

use crate::model::{BgaArgbEvent, BgaEventKind, BgaOpacityEvent};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct BgaTint {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl BgaTint {
    pub const OPAQUE: Self = Self { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
}

/// 指定レイヤー・時刻の BGA 不透明度 (0xFF = 不透明)。
pub fn bga_opacity_at_time(events: &[BgaOpacityEvent], layer: BgaEventKind, now: TimeUs) -> u8 {
    let mut value = 0xFF;
    for event in events {
        if event.layer == layer && event.time <= now {
            value = event.opacity;
        }
    }
    value
}

/// 指定レイヤー・時刻の BGA ARGB。未指定時は白・不透明。
pub fn bga_argb_at_time(
    events: &[BgaArgbEvent],
    layer: BgaEventKind,
    now: TimeUs,
) -> (u8, u8, u8, u8) {
    let mut argb = (0xFF, 0xFF, 0xFF, 0xFF);
    for event in events {
        if event.layer == layer && event.time <= now {
            argb = (event.alpha, event.red, event.green, event.blue);
        }
    }
    argb
}

/// opacity チャネルと ARGB チャネルを合成した描画 tint を返す。
pub fn bga_tint_at_time(
    opacity_events: &[BgaOpacityEvent],
    argb_events: &[BgaArgbEvent],
    layer: BgaEventKind,
    now: TimeUs,
) -> BgaTint {
    let opacity = bga_opacity_at_time(opacity_events, layer, now);
    let (alpha, red, green, blue) = bga_argb_at_time(argb_events, layer, now);
    BgaTint {
        r: red as f32 / 255.0,
        g: green as f32 / 255.0,
        b: blue as f32 / 255.0,
        a: (opacity as f32 / 255.0) * (alpha as f32 / 255.0),
    }
}

#[cfg(test)]
mod tests {
    use bmz_core::time::{ChartTick, TimeUs};

    use super::*;
    use crate::model::{BgaArgbEvent, BgaOpacityEvent};

    #[test]
    fn bga_tint_combines_opacity_and_argb() {
        let opacity = vec![BgaOpacityEvent {
            tick: ChartTick(0),
            time: TimeUs(1_000_000),
            layer: BgaEventKind::Base,
            opacity: 128,
        }];
        let argb = vec![BgaArgbEvent {
            tick: ChartTick(0),
            time: TimeUs(1_000_000),
            layer: BgaEventKind::Base,
            alpha: 255,
            red: 255,
            green: 0,
            blue: 0,
        }];
        let tint = bga_tint_at_time(&opacity, &argb, BgaEventKind::Base, TimeUs(2_000_000));
        assert!((tint.r - 1.0).abs() < 0.01);
        assert!((tint.g - 0.0).abs() < 0.01);
        assert!((tint.a - 128.0 / 255.0).abs() < 0.01);
    }
}
