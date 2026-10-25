use super::*;

pub(super) fn build_sound_table(
    source_path: &Path,
    intermediate: &IntermediateChart,
    warnings: &mut Vec<ImportWarning>,
    check_resource_existence: bool,
) -> SoundTable {
    let mut by_wav_key = HashMap::new();
    let mut assets = Vec::new();
    let mut resource_exists_by_path = HashMap::new();
    let base_dir = source_path.parent().unwrap_or_else(|| Path::new(""));

    for wav in &intermediate.resources.wavs {
        let id = SoundId(assets.len() as u32);
        let path = if wav.path.is_absolute() { wav.path.clone() } else { base_dir.join(&wav.path) };
        if check_resource_existence {
            let exists = *resource_exists_by_path
                .entry(path.clone())
                .or_insert_with(|| sound_asset_exists(&path));
            if !exists {
                warnings.push(ImportWarning::MissingSoundFile { path: path.clone() });
            }
        }
        by_wav_key.insert(wav.key, id);
        assets.push(SoundAssetRef { id, path, slice: wav.slice });
    }

    SoundTable { by_wav_key, assets }
}

pub(super) fn build_bga_table(
    source_path: &Path,
    intermediate: &IntermediateChart,
    warnings: &mut Vec<ImportWarning>,
    check_resource_existence: bool,
) -> BgaTable {
    let mut by_bmp_key = HashMap::new();
    let mut assets = Vec::new();
    let base_dir = source_path.parent().unwrap_or_else(|| Path::new(""));

    let mut bmps = intermediate.resources.bmps.iter().collect::<Vec<_>>();
    bmps.sort_by_key(|bmp| bmp.key);

    for bmp in bmps {
        let id = BgaAssetId(assets.len() as u32);
        let unresolved =
            if bmp.path.is_absolute() { bmp.path.clone() } else { base_dir.join(&bmp.path) };
        let path = crate::bga_asset::resolve_bga_asset_path(base_dir, &bmp.path)
            .unwrap_or_else(|| unresolved.clone());
        if check_resource_existence && !bmz_song_resource::file_exists(&path) {
            warnings.push(ImportWarning::MissingBmpFile { path: path.clone() });
        }
        by_bmp_key.insert(bmp.key, id);
        assets.push(BgaAssetRef { id, kind: crate::bga_asset::bga_asset_kind(&path), path });
    }

    BgaTable { by_bmp_key, assets }
}

pub(super) fn resolve_sound_id(
    wav_key: Option<u16>,
    table: &SoundTable,
    warnings: &mut Vec<ImportWarning>,
) -> Option<SoundId> {
    let key = wav_key?;
    match table.by_wav_key.get(&key).copied() {
        Some(id) => Some(id),
        None => {
            warnings.push(ImportWarning::MissingWavDefinition { key });
            None
        }
    }
}

pub(super) fn materialize_tick_objects(
    intermediate: &IntermediateChart,
) -> Result<Vec<TickObject>, ImportError> {
    let mut out = Vec::new();

    for object in &intermediate.objects {
        let tick = object_to_tick(object, &intermediate.measures)?;
        let kind = match object.kind {
            IntermediateObjectKind::VisibleNote { lane, wav_key } => {
                Some(TickObjectKind::VisibleNote { lane, wav_key })
            }
            IntermediateObjectKind::InvisibleNote { lane, wav_key } => {
                Some(TickObjectKind::InvisibleNote { lane, wav_key })
            }
            IntermediateObjectKind::LongChannelNote { lane, wav_key, mode, explicit_end_sound } => {
                Some(TickObjectKind::LongChannelNote { lane, wav_key, mode, explicit_end_sound })
            }
            IntermediateObjectKind::MineNote { lane, wav_key, damage } => {
                Some(TickObjectKind::MineNote { lane, wav_key, damage })
            }
            IntermediateObjectKind::Bgm { wav_key } => Some(TickObjectKind::Bgm { wav_key }),
            IntermediateObjectKind::Bga { bmp_key, kind } => {
                Some(TickObjectKind::Bga { bmp_key, kind: bga_event_kind(kind) })
            }
            IntermediateObjectKind::SetBpm { .. }
            | IntermediateObjectKind::SetExtendedBpm { .. }
            | IntermediateObjectKind::Stop { .. }
            | IntermediateObjectKind::BmsonStop { .. }
            | IntermediateObjectKind::SetScroll { .. }
            | IntermediateObjectKind::SetSpeed { .. }
            | IntermediateObjectKind::SetJudgeRank { .. }
            | IntermediateObjectKind::SetBgmVolume { .. }
            | IntermediateObjectKind::SetKeyVolume { .. }
            | IntermediateObjectKind::SetText { .. }
            | IntermediateObjectKind::SetBgaOpacity { .. }
            | IntermediateObjectKind::SetBgaArgb { .. }
            | IntermediateObjectKind::BgaKeybound { .. } => None,
        };

        if let Some(kind) = kind {
            out.push(TickObject { tick, kind });
        }
    }

    out.sort_by_key(|object| object.tick);
    Ok(out)
}

pub(super) fn bga_event_kind(kind: IntermediateBgaKind) -> BgaEventKind {
    match kind {
        IntermediateBgaKind::Base => BgaEventKind::Base,
        IntermediateBgaKind::Poor => BgaEventKind::Poor,
        IntermediateBgaKind::Layer => BgaEventKind::Layer,
        IntermediateBgaKind::Layer2 => BgaEventKind::Layer2,
    }
}
