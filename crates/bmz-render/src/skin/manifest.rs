use super::*;

impl SkinManifest {
    pub fn bundled_default() -> Self {
        Self {
            textures: vec![
                skin_texture_manifest(1, "note.png"),
                skin_texture_manifest(2, "note-blue.png"),
                skin_texture_manifest(3, "note-red.png"),
                skin_texture_manifest(4, "receptor.png"),
                skin_texture_manifest(5, "receptor-blue.png"),
                skin_texture_manifest(6, "receptor-red.png"),
                skin_texture_manifest(7, "judge-line.png"),
                skin_texture_manifest(8, "gauge-frame.png"),
                skin_texture_manifest(9, "gauge-fill.png"),
                skin_texture_manifest(10, "combo-panel.png"),
                skin_texture_manifest(11, "combo-panel-inactive.png"),
                skin_texture_manifest(12, "note-mine.png"),
            ],
            play: SkinPlayManifest {
                note: Some(SkinImageManifest {
                    texture: 1,
                    key_even_texture: Some(2),
                    scratch_texture: Some(3),
                    source_size: None,
                    uv: TextureRegion::default(),
                    scale: SkinImageScale::Stretch,
                    border: None,
                }),
                ln_start: None,
                ln_end: None,
                receptor: Some(SkinImageManifest {
                    texture: 4,
                    key_even_texture: Some(5),
                    scratch_texture: Some(6),
                    source_size: None,
                    uv: TextureRegion::default(),
                    scale: SkinImageScale::Stretch,
                    border: None,
                }),
                judge_line: Some(SkinImageManifest {
                    texture: 7,
                    key_even_texture: None,
                    scratch_texture: None,
                    source_size: None,
                    uv: TextureRegion::default(),
                    scale: SkinImageScale::Stretch,
                    border: None,
                }),
                gauge_frame: Some(SkinImageManifest {
                    texture: 8,
                    key_even_texture: None,
                    scratch_texture: None,
                    source_size: None,
                    uv: TextureRegion::default(),
                    scale: SkinImageScale::NineSlice,
                    border: Some(SkinImageBorder {
                        left: 2.0,
                        right: 2.0,
                        top: 3.0,
                        bottom: 3.0,
                        unit: SkinImageBorderUnit::Pixels,
                    }),
                }),
                gauge_fill: Some(SkinImageManifest {
                    texture: 9,
                    key_even_texture: None,
                    scratch_texture: None,
                    source_size: None,
                    uv: TextureRegion::default(),
                    scale: SkinImageScale::Stretch,
                    border: None,
                }),
                combo_panel: Some(SkinImageManifest {
                    texture: 10,
                    key_even_texture: None,
                    scratch_texture: None,
                    source_size: None,
                    uv: TextureRegion::default(),
                    scale: SkinImageScale::NineSlice,
                    border: Some(SkinImageBorder {
                        left: 4.0,
                        right: 4.0,
                        top: 3.0,
                        bottom: 3.0,
                        unit: SkinImageBorderUnit::Pixels,
                    }),
                }),
                combo_panel_inactive: Some(SkinImageManifest {
                    texture: 11,
                    key_even_texture: None,
                    scratch_texture: None,
                    source_size: None,
                    uv: TextureRegion::default(),
                    scale: SkinImageScale::NineSlice,
                    border: Some(SkinImageBorder {
                        left: 4.0,
                        right: 4.0,
                        top: 3.0,
                        bottom: 3.0,
                        unit: SkinImageBorderUnit::Pixels,
                    }),
                }),
            },
        }
    }

    pub fn resolve_textures(&self, base_dir: &Path) -> Vec<ResolvedSkinTexture> {
        self.textures
            .iter()
            .map(|texture| {
                let path = Path::new(&texture.path);
                let path =
                    if path.is_absolute() { path.to_path_buf() } else { base_dir.join(path) };
                ResolvedSkinTexture { id: TextureId(texture.id), path }
            })
            .collect()
    }

    pub fn with_texture_source_sizes(mut self, base_dir: &Path) -> Self {
        let sizes = self.texture_source_sizes(base_dir);
        fill_image_source_size(&mut self.play.note, &sizes);
        fill_image_source_size(&mut self.play.receptor, &sizes);
        fill_image_source_size(&mut self.play.judge_line, &sizes);
        fill_image_source_size(&mut self.play.gauge_frame, &sizes);
        fill_image_source_size(&mut self.play.gauge_fill, &sizes);
        fill_image_source_size(&mut self.play.combo_panel, &sizes);
        fill_image_source_size(&mut self.play.combo_panel_inactive, &sizes);
        self
    }

    fn texture_source_sizes(&self, base_dir: &Path) -> HashMap<u32, SkinImageSize> {
        self.resolve_textures(base_dir)
            .into_iter()
            .filter_map(|texture| {
                let asset = load_png_rgba(&texture.path).ok()?;
                Some((
                    texture.id.0,
                    SkinImageSize { width: asset.width as f32, height: asset.height as f32 },
                ))
            })
            .collect()
    }

    pub fn play_note_image(&self) -> SkinImageManifest {
        self.play.note.unwrap_or(SkinImageManifest {
            texture: crate::plan::DEFAULT_NOTE_TEXTURE.0,
            key_even_texture: None,
            scratch_texture: None,
            source_size: None,
            uv: TextureRegion::default(),
            scale: SkinImageScale::Stretch,
            border: None,
        })
    }

    /// LN START（ヘッドキャップ）用画像。未設定なら通常ノーツ画像にフォールバック。
    pub fn play_ln_start_image(&self) -> SkinImageManifest {
        self.play.ln_start.unwrap_or_else(|| self.play_note_image())
    }

    /// LN END（テールキャップ）用画像。未設定なら通常ノーツ画像にフォールバック。
    pub fn play_ln_end_image(&self) -> SkinImageManifest {
        self.play.ln_end.unwrap_or_else(|| self.play_note_image())
    }

    pub fn play_receptor_image(&self) -> SkinImageManifest {
        self.play.receptor.unwrap_or(SkinImageManifest {
            texture: crate::plan::DEFAULT_RECEPTOR_TEXTURE.0,
            key_even_texture: None,
            scratch_texture: None,
            source_size: None,
            uv: TextureRegion::default(),
            scale: SkinImageScale::Stretch,
            border: None,
        })
    }

    pub fn play_judge_line_image(&self) -> SkinImageManifest {
        self.play.judge_line.unwrap_or(SkinImageManifest {
            texture: crate::plan::DEFAULT_JUDGE_LINE_TEXTURE.0,
            key_even_texture: None,
            scratch_texture: None,
            source_size: None,
            uv: TextureRegion::default(),
            scale: SkinImageScale::Stretch,
            border: None,
        })
    }

    pub fn play_gauge_frame_image(&self) -> SkinImageManifest {
        self.play.gauge_frame.unwrap_or(SkinImageManifest {
            texture: crate::plan::DEFAULT_GAUGE_FRAME_TEXTURE.0,
            key_even_texture: None,
            scratch_texture: None,
            source_size: None,
            uv: TextureRegion::default(),
            scale: SkinImageScale::Stretch,
            border: None,
        })
    }

    pub fn play_gauge_fill_image(&self) -> SkinImageManifest {
        self.play.gauge_fill.unwrap_or(SkinImageManifest {
            texture: crate::plan::DEFAULT_GAUGE_FILL_TEXTURE.0,
            key_even_texture: None,
            scratch_texture: None,
            source_size: None,
            uv: TextureRegion::default(),
            scale: SkinImageScale::Stretch,
            border: None,
        })
    }

    pub fn play_combo_panel_image(&self, active: bool) -> SkinImageManifest {
        if active { self.play.combo_panel } else { self.play.combo_panel_inactive }.unwrap_or(
            SkinImageManifest {
                texture: if active {
                    crate::plan::DEFAULT_COMBO_PANEL_TEXTURE.0
                } else {
                    crate::plan::DEFAULT_COMBO_PANEL_INACTIVE_TEXTURE.0
                },
                key_even_texture: None,
                scratch_texture: None,
                source_size: None,
                uv: TextureRegion::default(),
                scale: SkinImageScale::Stretch,
                border: None,
            },
        )
    }
}

pub(super) fn skin_texture_manifest(id: u32, path: &str) -> SkinTextureManifest {
    SkinTextureManifest { id, path: path.to_string() }
}

pub fn default_skin_manifest() -> SkinManifest {
    static DEFAULT_SKIN_MANIFEST: OnceLock<SkinManifest> = OnceLock::new();
    DEFAULT_SKIN_MANIFEST
        .get_or_init(|| default_skin_manifest_for_root(&default_skin_root()))
        .clone()
}

pub fn default_skin_manifest_for_root(default_root: &Path) -> SkinManifest {
    SkinManifest::bundled_default().with_texture_source_sizes(default_root)
}

pub(super) fn default_skin_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/skins/default")
}

pub(super) fn fill_image_source_size(
    image: &mut Option<SkinImageManifest>,
    sizes: &HashMap<u32, SkinImageSize>,
) {
    let Some(image) = image else {
        return;
    };
    if image.source_size.is_none() {
        image.source_size = sizes.get(&image.texture).copied();
    }
}

pub fn append_skin_render_items(commands: &mut Vec<DrawCommand>, items: &[SkinRenderItem]) {
    commands.reserve(items.len());
    for item in items {
        append_skin_render_item(commands, item);
    }
}

pub fn append_skin_render_item(commands: &mut Vec<DrawCommand>, item: &SkinRenderItem) {
    match item {
        SkinRenderItem::Ambient { rect, blur, fade_edges, layers } => {
            let mut layer_commands = Vec::new();
            append_skin_render_items(&mut layer_commands, layers);
            commands.push(DrawCommand::Ambient {
                rect: *rect,
                blur: *blur,
                fade_edges: *fade_edges,
                layers: layer_commands,
            });
        }
        SkinRenderItem::Rect { rect, color, .. } => {
            commands.push(DrawCommand::Rect { rect: *rect, color: *color });
        }
        SkinRenderItem::RectBatch { rects, cache } => {
            if !rects.is_empty() {
                commands.push(DrawCommand::RectBatch { rects: Arc::clone(rects), cache: *cache });
            }
        }
        SkinRenderItem::Text { origin, text, style, caret, post_scale, .. } => {
            if !text.is_empty() || caret.is_some() {
                commands.push(DrawCommand::Text {
                    origin: *origin,
                    text: text.clone(),
                    caret: *caret,
                    style: style.clone(),
                    post_scale: *post_scale,
                });
            }
        }
        SkinRenderItem::Image {
            texture,
            rect,
            uv,
            tint,
            blend,
            scale,
            border,
            source_size,
            linear_filter,
        } => {
            append_skin_image_command(
                commands,
                *texture,
                *rect,
                *uv,
                *tint,
                *blend,
                *scale,
                *border,
                *source_size,
                *linear_filter,
            );
        }
        SkinRenderItem::RotatedImage {
            texture,
            rect,
            uv,
            tint,
            blend,
            source_size,
            linear_filter,
            angle_deg,
            center,
            post_scale,
        } => {
            commands.push(DrawCommand::RotatedImage {
                rect: *rect,
                uv: UvRect { x: uv.x, y: uv.y, width: uv.width, height: uv.height },
                source_size: *source_size,
                texture: TextureId(texture.0),
                tint: *tint,
                blend: *blend,
                linear_filter: *linear_filter,
                angle_rad: angle_deg.to_radians(),
                center: *center,
                post_scale: *post_scale,
            });
        }
    }
}

pub(super) fn append_skin_image_command(
    commands: &mut Vec<DrawCommand>,
    texture: SkinTextureId,
    rect: Rect,
    uv: TextureRegion,
    tint: Color,
    blend: BlendMode,
    scale: SkinImageScale,
    border: Option<SkinImageBorder>,
    source_size: Option<SkinImageSize>,
    linear_filter: bool,
) {
    match (scale, border) {
        (SkinImageScale::NineSlice, Some(border)) => {
            append_nine_slice_image_commands(
                commands,
                texture,
                rect,
                uv,
                tint,
                blend,
                border,
                source_size,
                linear_filter,
            );
        }
        _ => commands.push(DrawCommand::Image {
            rect,
            uv: UvRect { x: uv.x, y: uv.y, width: uv.width, height: uv.height },
            source_size,
            texture: TextureId(texture.0),
            tint,
            blend,
            linear_filter,
        }),
    }
}

pub(super) fn append_nine_slice_image_commands(
    commands: &mut Vec<DrawCommand>,
    texture: SkinTextureId,
    rect: Rect,
    uv: TextureRegion,
    tint: Color,
    blend: BlendMode,
    border: SkinImageBorder,
    source_size: Option<SkinImageSize>,
    linear_filter: bool,
) {
    if rect.width <= 0.0 || rect.height <= 0.0 || uv.width <= 0.0 || uv.height <= 0.0 {
        return;
    }

    let Some(border) = border.normalized(source_size) else {
        commands.push(DrawCommand::Image {
            rect,
            uv: UvRect { x: uv.x, y: uv.y, width: uv.width, height: uv.height },
            source_size,
            texture: TextureId(texture.0),
            tint,
            blend,
            linear_filter,
        });
        return;
    };
    let left = border.left.clamp(0.0, 0.5);
    let right = border.right.clamp(0.0, 0.5);
    let top = border.top.clamp(0.0, 0.5);
    let bottom = border.bottom.clamp(0.0, 0.5);
    if left + right >= 1.0 || top + bottom >= 1.0 {
        commands.push(DrawCommand::Image {
            rect,
            uv: UvRect { x: uv.x, y: uv.y, width: uv.width, height: uv.height },
            source_size,
            texture: TextureId(texture.0),
            tint,
            blend,
            linear_filter,
        });
        return;
    }

    let xs = [
        rect.x,
        rect.x + rect.width * left,
        rect.x + rect.width * (1.0 - right),
        rect.x + rect.width,
    ];
    let ys = [
        rect.y,
        rect.y + rect.height * top,
        rect.y + rect.height * (1.0 - bottom),
        rect.y + rect.height,
    ];
    let us = [uv.x, uv.x + uv.width * left, uv.x + uv.width * (1.0 - right), uv.x + uv.width];
    let vs = [uv.y, uv.y + uv.height * top, uv.y + uv.height * (1.0 - bottom), uv.y + uv.height];

    for row in 0..3 {
        for column in 0..3 {
            let piece = Rect {
                x: xs[column],
                y: ys[row],
                width: xs[column + 1] - xs[column],
                height: ys[row + 1] - ys[row],
            };
            let piece_uv = UvRect {
                x: us[column],
                y: vs[row],
                width: us[column + 1] - us[column],
                height: vs[row + 1] - vs[row],
            };
            if piece.width > 0.0
                && piece.height > 0.0
                && piece_uv.width > 0.0
                && piece_uv.height > 0.0
            {
                commands.push(DrawCommand::Image {
                    rect: piece,
                    uv: piece_uv,
                    source_size,
                    texture: TextureId(texture.0),
                    tint,
                    blend,
                    linear_filter,
                });
            }
        }
    }
}
