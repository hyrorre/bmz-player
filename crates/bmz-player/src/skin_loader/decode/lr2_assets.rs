use crate::skin_loader::*;

pub(in crate::skin_loader) fn lr2_builtin_source_asset(path: &str) -> Option<RgbaImageAsset> {
    if path == "bmz://lr2/judgedetail" {
        return Some(lr2_judge_detail_asset());
    }
    let pixel = match path {
        "bmz://lr2/black" => [0, 0, 0, 255],
        "bmz://lr2/white" => [255, 255, 255, 255],
        // BACKBMP itself is drawn by the play snapshot path.  Keep a transparent
        // source so LR2 CSV objects using IMAGE_BACKBMP can be decoded without
        // failing texture resolution when the chart has no backbmp.
        "bmz://lr2/backbmp" | "bmz://lr2/empty" => [0, 0, 0, 0],
        _ => return None,
    };
    Some(RgbaImageAsset { width: 1, height: 1, pixels: pixel.to_vec() })
}

pub(in crate::skin_loader) fn lr2_judge_detail_asset() -> RgbaImageAsset {
    const WIDTH: u32 = 120;
    const HEIGHT: u32 = 100;
    let mut pixels = vec![0; (WIDTH * HEIGHT * 4) as usize];
    draw_lr2_bitmap_text(&mut pixels, WIDTH, 5, 5, "EARLY", [255, 255, 255, 255]);
    draw_lr2_bitmap_text(&mut pixels, WIDTH, 59, 5, "LATE", [255, 255, 255, 255]);
    for (pair, color) in [[255, 255, 255, 255], [255, 192, 64, 255]].into_iter().enumerate() {
        for row in 0..2 {
            let y = 20 + pair as u32 * 40 + row * 20;
            for digit in 0..10 {
                draw_lr2_bitmap_glyph(
                    &mut pixels,
                    WIDTH,
                    digit as u32 * 10 + 2,
                    y + 5,
                    char::from(b'0' + digit as u8),
                    color,
                );
            }
            draw_lr2_bitmap_glyph(
                &mut pixels,
                WIDTH,
                112,
                y + 5,
                if row == 0 { '+' } else { '-' },
                color,
            );
        }
    }
    RgbaImageAsset { width: WIDTH, height: HEIGHT, pixels }
}

pub(in crate::skin_loader) fn draw_lr2_bitmap_text(
    pixels: &mut [u8],
    width: u32,
    x: u32,
    y: u32,
    text: &str,
    color: [u8; 4],
) {
    for (index, character) in text.chars().enumerate() {
        draw_lr2_bitmap_glyph(pixels, width, x + index as u32 * 8, y, character, color);
    }
}

pub(in crate::skin_loader) fn draw_lr2_bitmap_glyph(
    pixels: &mut [u8],
    width: u32,
    x: u32,
    y: u32,
    character: char,
    color: [u8; 4],
) {
    let rows = lr2_bitmap_glyph(character);
    for (row, bits) in rows.into_iter().enumerate() {
        for column in 0..3 {
            if bits & (1 << (2 - column)) == 0 {
                continue;
            }
            for dy in 0..2 {
                for dx in 0..2 {
                    let px = x + column * 2 + dx;
                    let py = y + row as u32 * 2 + dy;
                    let offset = ((py * width + px) * 4) as usize;
                    if let Some(target) = pixels.get_mut(offset..offset + 4) {
                        target.copy_from_slice(&color);
                    }
                }
            }
        }
    }
}

pub(in crate::skin_loader) fn lr2_bitmap_glyph(character: char) -> [u8; 5] {
    match character {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b010, 0b010, 0b010],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        'A' => [0b010, 0b101, 0b111, 0b101, 0b101],
        'E' => [0b111, 0b100, 0b110, 0b100, 0b111],
        'L' => [0b100, 0b100, 0b100, 0b100, 0b111],
        'R' => [0b110, 0b101, 0b110, 0b101, 0b101],
        'T' => [0b111, 0b010, 0b010, 0b010, 0b010],
        'Y' => [0b101, 0b101, 0b010, 0b010, 0b010],
        '+' => [0b000, 0b010, 0b111, 0b010, 0b000],
        '-' => [0b000, 0b000, 0b111, 0b000, 0b000],
        _ => [0; 5],
    }
}
