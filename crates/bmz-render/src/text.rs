use crate::plan::{Color, DrawCommand, Point, Rect, TextAlign, TextLayer, TextOverflow, TextStyle};

pub const GLYPH_WIDTH: usize = 3;
pub const GLYPH_HEIGHT: usize = 5;

#[derive(Debug, Clone, Copy)]
pub struct BitmapTextStyle {
    pub x: f32,
    pub y: f32,
    pub cell: f32,
    pub color: Color,
}

#[derive(Debug, Default)]
pub struct TextRenderer;

impl TextRenderer {
    pub fn push_text(&self, commands: &mut Vec<DrawCommand>, text: &str, style: BitmapTextStyle) {
        push_text_command(commands, text, style);
    }
}

pub fn push_text_command(commands: &mut Vec<DrawCommand>, text: &str, style: BitmapTextStyle) {
    if text.is_empty() {
        return;
    }

    commands.push(DrawCommand::Text {
        origin: Point { x: style.x, y: style.y },
        text: text.to_string(),
        style: TextStyle {
            size: style.cell * GLYPH_HEIGHT as f32,
            color: style.color,
            layer: TextLayer::Ui,
            align: TextAlign::Left,
            max_width: 0.0,
            overflow: TextOverflow::Overflow,
        },
    });
}

pub fn push_bitmap_text(commands: &mut Vec<DrawCommand>, text: &str, style: BitmapTextStyle) {
    let mut cursor_x = style.x;
    for ch in text.chars() {
        if ch == ' ' {
            cursor_x += style.cell * (GLYPH_WIDTH as f32 + 1.0);
            continue;
        }

        let glyph = glyph_bits(ch);
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..GLYPH_WIDTH {
                if bits & (1 << (GLYPH_WIDTH - 1 - col)) == 0 {
                    continue;
                }
                commands.push(DrawCommand::Rect {
                    rect: Rect {
                        x: cursor_x + col as f32 * style.cell,
                        y: style.y + row as f32 * style.cell,
                        width: style.cell * 0.86,
                        height: style.cell * 0.86,
                    },
                    color: style.color,
                });
            }
        }
        cursor_x += style.cell * (GLYPH_WIDTH as f32 + 1.0);
    }
}

fn glyph_bits(ch: char) -> [u8; GLYPH_HEIGHT] {
    match ch.to_ascii_uppercase() {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b001, 0b010, 0b010],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        'A' => [0b010, 0b101, 0b111, 0b101, 0b101],
        'B' => [0b110, 0b101, 0b110, 0b101, 0b110],
        'C' => [0b111, 0b100, 0b100, 0b100, 0b111],
        'D' => [0b110, 0b101, 0b101, 0b101, 0b110],
        'E' => [0b111, 0b100, 0b110, 0b100, 0b111],
        'F' => [0b111, 0b100, 0b110, 0b100, 0b100],
        'G' => [0b111, 0b100, 0b101, 0b101, 0b111],
        'H' => [0b101, 0b101, 0b111, 0b101, 0b101],
        'I' => [0b111, 0b010, 0b010, 0b010, 0b111],
        'J' => [0b001, 0b001, 0b001, 0b101, 0b111],
        'K' => [0b101, 0b101, 0b110, 0b101, 0b101],
        'L' => [0b100, 0b100, 0b100, 0b100, 0b111],
        'M' => [0b101, 0b111, 0b111, 0b101, 0b101],
        'N' => [0b101, 0b111, 0b111, 0b111, 0b101],
        'O' => [0b111, 0b101, 0b101, 0b101, 0b111],
        'P' => [0b111, 0b101, 0b111, 0b100, 0b100],
        'Q' => [0b111, 0b101, 0b101, 0b111, 0b001],
        'R' => [0b111, 0b101, 0b111, 0b110, 0b101],
        'S' => [0b111, 0b100, 0b111, 0b001, 0b111],
        'T' => [0b111, 0b010, 0b010, 0b010, 0b010],
        'U' => [0b101, 0b101, 0b101, 0b101, 0b111],
        'V' => [0b101, 0b101, 0b101, 0b101, 0b010],
        'W' => [0b101, 0b101, 0b111, 0b111, 0b101],
        'X' => [0b101, 0b101, 0b010, 0b101, 0b101],
        'Y' => [0b101, 0b101, 0b010, 0b010, 0b010],
        'Z' => [0b111, 0b001, 0b010, 0b100, 0b111],
        '-' => [0b000, 0b000, 0b111, 0b000, 0b000],
        '+' => [0b000, 0b010, 0b111, 0b010, 0b000],
        '.' => [0b000, 0b000, 0b000, 0b000, 0b010],
        '/' => [0b001, 0b001, 0b010, 0b100, 0b100],
        ':' => [0b000, 0b010, 0b000, 0b010, 0b000],
        '%' => [0b101, 0b001, 0b010, 0b100, 0b101],
        _ => [0b111, 0b001, 0b010, 0b000, 0b010],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitmap_text_pushes_rects_for_letters() {
        let mut commands = Vec::new();

        push_bitmap_text(
            &mut commands,
            "A1",
            BitmapTextStyle { x: 0.0, y: 0.0, cell: 0.01, color: Color::rgb(1.0, 1.0, 1.0) },
        );

        assert!(!commands.is_empty());
    }

    #[test]
    fn spaces_advance_without_drawing() {
        let mut commands = Vec::new();

        push_bitmap_text(
            &mut commands,
            " ",
            BitmapTextStyle { x: 0.0, y: 0.0, cell: 0.01, color: Color::rgb(1.0, 1.0, 1.0) },
        );

        assert!(commands.is_empty());
    }

    #[test]
    fn text_renderer_emits_text_command() {
        let mut commands = Vec::new();
        let renderer = TextRenderer;

        renderer.push_text(
            &mut commands,
            "OK",
            BitmapTextStyle { x: 0.0, y: 0.0, cell: 0.01, color: Color::rgb(1.0, 1.0, 1.0) },
        );

        assert!(matches!(commands.first(), Some(DrawCommand::Text { text, .. }) if text == "OK"));
    }
}
