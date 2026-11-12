use super::*;

pub(super) fn scene_kind(scene: &AppSceneSnapshot) -> AppSceneKind {
    match scene {
        AppSceneSnapshot::Select(_) => AppSceneKind::Select,
        AppSceneSnapshot::Decide(_) => AppSceneKind::Decide,
        AppSceneSnapshot::Play(_) => AppSceneKind::Play,
        AppSceneSnapshot::Result(_) => AppSceneKind::Result,
    }
}

pub(super) fn window_title_for_scene(scene_kind: AppSceneKind) -> &'static str {
    match scene_kind {
        AppSceneKind::Select => "bmz-player - Select",
        AppSceneKind::Decide => "bmz-player - Decide",
        AppSceneKind::Play => "bmz-player - Play",
        AppSceneKind::Result => "bmz-player - Result",
    }
}

pub(super) fn discord_key_mode_label(key_mode: KeyMode) -> String {
    let value = key_mode.as_str().strip_suffix('K').unwrap_or(key_mode.as_str());
    format!("{value}Keys")
}

pub(super) fn discord_join_metadata(first: &str, second: &str, separator: &str) -> Option<String> {
    let first = first.trim();
    let second = second.trim();
    match (first.is_empty(), second.is_empty()) {
        (true, true) => None,
        (false, true) => Some(first.to_string()),
        (true, false) => Some(second.to_string()),
        (false, false) => Some(format!("{first}{separator}{second}")),
    }
}

pub(super) fn physical_key_name(physical_key: PhysicalKey) -> Option<String> {
    use bmz_gameplay::input::backend::PhysicalControl;
    match physical_key_to_control(physical_key)? {
        PhysicalControl::KeyboardKey(name) => Some(name),
        _ => None,
    }
}

pub(super) fn result_panel_for_control(control: &ResolvedControl) -> Option<i32> {
    match control.standard()? {
        StandardKey::ArrowLeft => Some(2),
        StandardKey::ArrowRight => Some(1),
        _ => None,
    }
}

/// E1 + 数字キー1〜4 (テンキーを含む) のリプレイスロット。
pub(super) fn digit_to_replay_slot(control: &ResolvedControl) -> Option<u8> {
    match control.standard()? {
        StandardKey::Digit(digit @ 1..=4) => Some(digit - 1),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PrimaryIrPageIdentity {
    Chart {
        md5: Option<String>,
        sha256: Option<String>,
    },
    Course {
        canonical_hash: Option<String>,
        rian_hash_v1: Option<String>,
        bms_ir_course_key: Option<String>,
    },
}

pub(super) fn primary_ir_page_url(
    provider: &crate::config::profile_config::IrProviderConfig,
    identity: &PrimaryIrPageIdentity,
) -> Result<String> {
    match identity {
        PrimaryIrPageIdentity::Chart { md5, sha256 } => {
            if crate::ir::bms_ir::is_bms_ir_config(provider) {
                if let Some(md5) = md5.as_deref().filter(|value| !value.trim().is_empty()) {
                    crate::ir::bms_ir::chart_page_url(&provider.base_url, md5)
                } else {
                    let Some(sha256) = sha256.as_deref().filter(|value| !value.trim().is_empty())
                    else {
                        anyhow::bail!("chart hash is unavailable for the primary IR provider");
                    };
                    crate::ir::bms_ir::chart_search_page_url(&provider.base_url, sha256)
                }
            } else if crate::ir::rian_ir::is_rian_ir_config(provider) {
                let Some(sha256) = sha256.as_deref().filter(|value| !value.trim().is_empty())
                else {
                    anyhow::bail!("chart SHA-256 is unavailable for the primary IR provider");
                };
                crate::ir::rian_ir::chart_page_url(&provider.base_url, sha256)
            } else {
                let Some(sha256) = sha256.as_deref().filter(|value| !value.trim().is_empty())
                else {
                    anyhow::bail!("chart SHA-256 is unavailable for the primary IR provider");
                };
                Ok(format!("{}/charts/{sha256}", provider.base_url.trim_end_matches('/')))
            }
        }
        PrimaryIrPageIdentity::Course { canonical_hash, rian_hash_v1, bms_ir_course_key } => {
            let hash = if crate::ir::bms_ir::is_bms_ir_config(provider) {
                bms_ir_course_key.as_deref()
            } else if crate::ir::rian_ir::is_rian_ir_config(provider) {
                rian_hash_v1.as_deref()
            } else {
                canonical_hash.as_deref()
            };
            let Some(hash) = hash else {
                anyhow::bail!("course hash is unavailable for the primary IR provider");
            };
            if crate::ir::bms_ir::is_bms_ir_config(provider) {
                crate::ir::bms_ir::course_page_url(&provider.base_url, hash)
            } else if crate::ir::rian_ir::is_rian_ir_config(provider) {
                crate::ir::rian_ir::course_page_url(&provider.base_url, hash)
            } else {
                Ok(format!("{}/courses/{hash}", provider.base_url.trim_end_matches('/')))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TargetCycle {
    Previous,
    Next,
}

/// TARGET 変更。キーボード既定の↑で次・↓で前、スクラッチ上で次・下で前。
/// 9K は KEY8 / KEY9 (beatoraja POPN_9K と同じ)。
pub(super) fn target_cycle_from_input(control: &ResolvedControl) -> Option<TargetCycle> {
    match control.standard() {
        Some(StandardKey::ArrowUp) => return Some(TargetCycle::Next),
        Some(StandardKey::ArrowDown) => return Some(TargetCycle::Previous),
        _ => {}
    }
    if control.is_scratch_up() || control.nine_key_target() == Some(1) {
        Some(TargetCycle::Next)
    } else if control.is_scratch_down() || control.nine_key_target() == Some(-1) {
        Some(TargetCycle::Previous)
    } else {
        None
    }
}
