use std::any::Any;

use font_kit::error::SelectionError;
use font_kit::family_handle::FamilyHandle;
use font_kit::properties::Stretch;
use ttf_parser::{Face, name_id};

use super::*;

/// Read collection indices and metadata from the font bytes. DirectWrite's
/// synthetic style enumeration is not the TTC face order consumed by ab_glyph.
struct BundledFace {
    font: ResolvedFont,
    families: Vec<String>,
    properties: Properties,
}

pub(super) struct BundledFonts(Vec<BundledFace>);

impl BundledFonts {
    pub(super) fn new(roots: &[PathBuf]) -> Self {
        let mut faces = Vec::new();
        for root in roots {
            for entry in walkdir::WalkDir::new(root).sort_by_file_name().into_iter().flatten() {
                if !entry.file_type().is_file() {
                    continue;
                }
                let Ok(bytes) = std::fs::read(entry.path()) else { continue };
                for font_index in 0..ttf_parser::fonts_in_collection(&bytes).unwrap_or(1) {
                    let Ok(face) = Face::parse(&bytes, font_index) else { continue };
                    let families = face
                        .names()
                        .into_iter()
                        .filter(|name| {
                            matches!(name.name_id, name_id::FAMILY | name_id::TYPOGRAPHIC_FAMILY)
                        })
                        .filter_map(|name| name.to_string())
                        .collect();
                    faces.push(BundledFace {
                        font: ResolvedFont {
                            path: Some(entry.path().to_path_buf()),
                            memory: None,
                            font_index,
                        },
                        families,
                        properties: face_properties(&face),
                    });
                }
            }
        }
        Self(faces)
    }

    pub(super) fn resolve(&self, coverage: FontCoverage) -> Option<ResolvedFont> {
        resolve_font_for_coverage_from_source(self, coverage)
    }
}

impl BundledFace {
    fn handle(&self) -> Handle {
        Handle::from_path(self.font.path.clone().unwrap(), self.font.font_index)
    }
}

impl Source for BundledFonts {
    fn all_fonts(&self) -> Result<Vec<Handle>, SelectionError> {
        Ok(self.0.iter().map(BundledFace::handle).collect())
    }

    fn all_families(&self) -> Result<Vec<String>, SelectionError> {
        let mut families: Vec<_> = self.0.iter().flat_map(|face| face.families.clone()).collect();
        families.sort();
        families.dedup();
        Ok(families)
    }

    fn select_family_by_name(&self, family: &str) -> Result<FamilyHandle, SelectionError> {
        let handles: Vec<_> = self
            .0
            .iter()
            .filter(|face| face.families.iter().any(|name| name.eq_ignore_ascii_case(family)))
            .map(BundledFace::handle)
            .collect();
        if handles.is_empty() {
            return Err(SelectionError::NotFound);
        }
        Ok(FamilyHandle::from_font_handles(handles.into_iter()))
    }

    fn select_descriptions_in_family(
        &self,
        family: &FamilyHandle,
    ) -> Result<Vec<Properties>, SelectionError> {
        family
            .fonts()
            .iter()
            .map(|handle| {
                let font = handle_to_resolved(handle).ok_or(SelectionError::NotFound)?;
                self.0
                    .iter()
                    .find(|face| face.font == font)
                    .map(|face| face.properties)
                    .ok_or(SelectionError::NotFound)
            })
            .collect()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn Any {
        self
    }
}

fn face_properties(face: &Face<'_>) -> Properties {
    // OpenType usWidthClass (1..=9) -> CSS stretch ratio.
    let widths = [0.5, 0.625, 0.75, 0.875, 1.0, 1.125, 1.25, 1.5, 2.0];
    Properties {
        style: if face.is_oblique() {
            Style::Oblique
        } else if face.is_italic() {
            Style::Italic
        } else {
            Style::Normal
        },
        weight: Weight(f32::from(face.weight().to_number())),
        stretch: Stretch(widths[usize::from(face.width().to_number()) - 1]),
    }
}
