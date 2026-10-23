use super::*;

impl SkinPlacement {
    pub(super) fn resolve(&self, elapsed_ms: i32) -> ResolvedPlacement {
        let Some(frame) = self.animation.sample(elapsed_ms) else {
            return ResolvedPlacement { rect: self.rect, alpha: self.alpha, blend: self.blend };
        };

        ResolvedPlacement { rect: frame.rect, alpha: self.alpha * frame.alpha, blend: self.blend }
    }
}

impl Animation {
    pub fn none() -> Self {
        Self { keyframes: Vec::new() }
    }

    fn sample(&self, elapsed_ms: i32) -> Option<Keyframe> {
        self.keyframes
            .iter()
            .filter(|frame| frame.time_ms <= elapsed_ms)
            .max_by_key(|frame| frame.time_ms)
            .copied()
    }
}

impl TextStyle {
    pub(super) fn with_alpha(self, alpha: f32) -> Self {
        Self {
            color: self.color.with_alpha(self.color.a * alpha),
            outline: self.outline.map(|outline| TextOutline {
                color: outline.color.with_alpha(outline.color.a * alpha),
                ..outline
            }),
            shadow: self.shadow.map(|shadow| TextShadow {
                color: shadow.color.with_alpha(shadow.color.a * alpha),
                ..shadow
            }),
            gradient: self.gradient.map(|gradient| TextGradient {
                color: gradient.color.with_alpha(gradient.color.a * alpha),
                ..gradient
            }),
            ..self
        }
    }
}

impl Color {
    pub(super) fn with_alpha(self, alpha: f32) -> Self {
        Self { a: alpha.clamp(0.0, 1.0), ..self }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ResolvedPlacement {
    pub(super) rect: Rect,
    pub(super) alpha: f32,
    pub(super) blend: BlendMode,
}

pub(super) fn format_number(value: i64, digits: u8) -> String {
    if digits == 0 {
        value.to_string()
    } else {
        format!("{:0width$}", value.max(0), width = digits as usize)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NumberPadding {
    None,
    Zero,
    Blank,
}

impl NumberPadding {
    pub(super) fn is_zero_padding(self) -> bool {
        matches!(self, Self::Zero)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SignedNumberRowOrder {
    PositiveFirst,
    #[cfg(test)]
    NegativeFirst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SignedNumberRender {
    Unsigned,
    Signed(SignedNumberRowOrder),
}

pub(super) fn number_padding(value: &SkinValueDef) -> NumberPadding {
    if value.zeropadding == 2 || value.padding == 2 {
        return NumberPadding::Blank;
    }
    if value.zeropadding != 0 || value.padding != 0 {
        return NumberPadding::Zero;
    }
    let image_cells = value.divx.max(1).saturating_mul(value.divy.max(1));
    if !value_layout_is_signed(value) && image_cells % 10 != 0 {
        return NumberPadding::Blank;
    }
    NumberPadding::None
}

pub(super) fn lr2_number_frame_cells(value: &SkinValueDef) -> Option<i32> {
    let cells = value.divx.max(1).saturating_mul(value.divy.max(1));
    // LR2's later format checks take precedence when a sheet is divisible by
    // several glyph counts (for example 110 cells is an eleven-cell font).
    [24, 11, 10].into_iter().find(|stride| cells % stride == 0)
}

/// The source frame shared by every glyph, also used by the number cache.
pub(super) fn lr2_number_animation_frame(value: &SkinValueDef, elapsed_ms: i32) -> i32 {
    if value.cycle <= 0 {
        return 0;
    }
    let stride = lr2_number_frame_cells(value).unwrap_or(1);
    let frames = value.divx.max(1).saturating_mul(value.divy.max(1)) / stride;
    (i64::from(elapsed_ms.rem_euclid(value.cycle)) * i64::from(frames) / i64::from(value.cycle))
        as i32
}

/// LR2's glyph indices are linear in the source grid, including blanks and
/// the sign cell. MAX denotes an undrawn space in a ten-cell font.
pub(super) fn lr2_number_digits(value: &SkinValueDef, number: i64, align: i32) -> Option<Vec<u8>> {
    let stride = lr2_number_frame_cells(value)?;
    let signed = stride == 24;
    if number < 0 && !signed {
        return None;
    }
    let width = value.digit.saturating_sub(i32::from(signed)).max(0) as usize;
    if width == 0 {
        return None;
    }
    let text = number.unsigned_abs().to_string();
    let text = if text.len() > width {
        if align == 0 { &text[text.len() - width..] } else { &text[..width] }
    } else {
        text.as_str()
    };
    // LR2 only selects the negative numeric row for a single 24-cell sheet;
    // animated sheets keep the common numeric row and select a minus sign.
    let offset = if number < 0 && value.divx.saturating_mul(value.divy) == 24 { 12 } else { 0 };
    let blank = if stride == 10 { u8::MAX } else { 10 + offset };
    let mut digits = Vec::with_capacity(value.digit.max(0) as usize);
    if signed {
        digits.push(if number < 0 { 23 } else { 11 });
    }
    let blanks = width.saturating_sub(text.len());
    if align == 0 {
        digits.extend(std::iter::repeat_n(blank, blanks));
    }
    digits.extend(text.bytes().map(|digit| digit - b'0' + offset));
    if align != 0 {
        digits.extend(std::iter::repeat_n(blank, blanks));
    }
    Some(digits)
}

pub(super) fn signed_value_padding(_value: &SkinValueDef, padding: NumberPadding) -> NumberPadding {
    // beatoraja の SkinNumber は、mimage がある符号付き数値でも
    // `zeropadding` をそのまま適用する。差分 ref だけ設定を無視すると、
    // digit 枠の先頭に符号を固定できず、スキンごとの桁レイアウトが崩れる。
    padding
}

pub(super) fn display_number_digits(
    value: i64,
    max_digits: usize,
    padding: NumberPadding,
) -> Vec<u8> {
    let value = value.saturating_abs();
    let mut text = if padding.is_zero_padding() && max_digits > 0 {
        format!("{value:0width$}", width = max_digits)
    } else {
        value.to_string()
    };
    if max_digits > 0 && text.len() > max_digits {
        text = text[text.len() - max_digits..].to_string();
    }
    let mut digits: Vec<u8> =
        text.bytes().filter(|byte| byte.is_ascii_digit()).map(|byte| byte - b'0').collect();
    if matches!(padding, NumberPadding::Blank) && max_digits > digits.len() {
        let mut padded = vec![10; max_digits - digits.len()];
        padded.extend(digits);
        digits = padded;
    }
    digits
}

/// 符号付き数値（beatoraja の mimage 慣習）用に、divx 列のテクスチャセル index を返す。
///
/// レイアウト (`divx`>=12, `divy`>=2):
/// - 各行は `[0,1,2,3,4,5,6,7,8,9, blank, sign]`
/// - 行0: 正数用 (sign cell = `+`)
/// - 行1: 負数用 (sign cell = `-`)
///
/// 返り値の各 byte は `digit_index % divx` が列、`digit_index / divx` が行になる。
/// 先頭要素は符号セル (index 11)、続けて絶対値の右寄せ桁が並ぶ。
pub(super) fn display_signed_number_digits(
    value: i64,
    max_digits: usize,
    padding: NumberPadding,
    divx: u32,
) -> Vec<u8> {
    display_signed_number_digits_with_row_order(
        value,
        max_digits,
        padding,
        divx,
        SignedNumberRowOrder::PositiveFirst,
    )
}

pub(super) fn display_signed_number_digits_with_row_order(
    value: i64,
    max_digits: usize,
    padding: NumberPadding,
    divx: u32,
    row_order: SignedNumberRowOrder,
) -> Vec<u8> {
    if max_digits == 0 {
        return Vec::new();
    }
    let negative_row = match row_order {
        SignedNumberRowOrder::PositiveFirst => divx as u8,
        #[cfg(test)]
        SignedNumberRowOrder::NegativeFirst => 0,
    };
    let positive_row = match row_order {
        SignedNumberRowOrder::PositiveFirst => 0,
        #[cfg(test)]
        SignedNumberRowOrder::NegativeFirst => divx as u8,
    };
    let row_offset = if value < 0 { negative_row } else { positive_row };
    let abs = value.unsigned_abs();
    let abs_text = abs.to_string();
    let numeric_width = max_digits.saturating_sub(1);
    let trimmed = if matches!(padding, NumberPadding::None) {
        if abs_text.len() > max_digits {
            abs_text[abs_text.len() - max_digits..].to_string()
        } else {
            abs_text
        }
    } else if abs_text.len() > numeric_width {
        abs_text[abs_text.len() - numeric_width..].to_string()
    } else {
        abs_text
    };
    let sign_visible = !matches!(padding, NumberPadding::None) || trimmed.len() < max_digits;
    let mut digits = Vec::with_capacity(max_digits);
    if sign_visible {
        // beatoraja の `keta` (`digit`) は符号セルを含む総枠数。
        digits.push(11u8 + row_offset);
    }
    if sign_visible && numeric_width > trimmed.len() {
        let fill = match padding {
            NumberPadding::Zero => 0,
            NumberPadding::Blank => 10,
            NumberPadding::None => u8::MAX,
        };
        if fill != u8::MAX {
            digits.extend(std::iter::repeat_n(fill + row_offset, numeric_width - trimmed.len()));
        }
    }
    for byte in trimmed.bytes() {
        if byte.is_ascii_digit() {
            digits.push((byte - b'0') + row_offset);
        }
    }
    digits
}

/// `ref_id` が符号付き表示を要求する Result 系 ref か。
/// beatoraja の `NUMBER_DIFF_*` 系のうち、正負を持つ差分を対象とする。
#[cfg(test)]
pub(super) fn ref_id_is_signed(ref_id: i32) -> bool {
    matches!(ref_id, 152 | 153 | 172 | 175 | 178 | SKIN_REF_BMZ_SCORE_GRADE_NEAREST_DIFF)
}

#[cfg(test)]
pub(super) fn value_ref_is_signed_for_state(ref_id: i32, state: &SkinDrawState) -> bool {
    ref_id_is_signed(ref_id)
        || (ref_id == 12 && state.select_screen && state.select_option_panel == 3)
}

/// beatoraja `JsonSkinObjectLoader` は value 画像のセル数 (`divx*divy`) が
/// 24 の倍数のとき +側/-側の別 image (mimage) を持つ符号付き数値として扱う。
/// ref に依らず画像レイアウトで決まる (例: Starseeker の ±ms 表示 ref=525, 12x2)。
pub(super) fn value_layout_is_signed(value: &SkinValueDef) -> bool {
    let cells = value.divx.max(1).saturating_mul(value.divy.max(1));
    cells >= 24 && cells % 24 == 0
}

pub(super) fn signed_number_render_for_value(
    value: &SkinValueDef,
    _state: &SkinDrawState,
) -> SignedNumberRender {
    // beatoraja chooses SkinNumber's positive/negative image sets solely from
    // the 24-cell layout. A signed ref rendered with an 11-cell digit sheet
    // uses its absolute value; the skin supplies a separate minus label.
    if value_layout_is_signed(value) {
        SignedNumberRender::Signed(signed_number_row_order_for_value(value, _state))
    } else {
        SignedNumberRender::Unsigned
    }
}

pub(super) fn signed_number_row_order_for_value(
    _value: &SkinValueDef,
    _state: &SkinDrawState,
) -> SignedNumberRowOrder {
    SignedNumberRowOrder::PositiveFirst
}
