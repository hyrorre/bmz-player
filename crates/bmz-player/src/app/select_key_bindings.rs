//! アナログスクロールの蓄積。入力の解決は [`super::input_resolve`] が行う。

/// アナログスクロールバッファへ delta を蓄積する。
/// suppress 中は idle (200ms 以上の tick 途切れ) を観測するまで delta を捨てる。
/// idle 後の最初の delta から通常蓄積に戻る。
pub(super) fn update_analog_scroll_buffer(
    buffer: &mut i32,
    suppress: &mut bool,
    idle: bool,
    delta: i32,
) {
    if *suppress {
        if !idle {
            *buffer = 0;
            return;
        }
        *suppress = false;
    }
    if idle {
        *buffer = 0;
    }
    *buffer += delta;
}

/// バッファから ticks_per_scroll ごとの移動数を取り出す。端数はバッファに残す。
pub(super) fn take_analog_scroll_steps(buffer: &mut i32, ticks_per_scroll: i32) -> i32 {
    let steps = *buffer / ticks_per_scroll;
    *buffer %= ticks_per_scroll;
    steps
}
