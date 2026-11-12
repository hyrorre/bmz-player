use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResultFavoriteKind {
    Song,
    Chart,
}

pub(super) fn result_favorite_kind(control: &ResolvedControl) -> Option<ResultFavoriteKind> {
    // Common E actions retain their priority over scene-specific shortcuts.
    if control.e_action().is_some() {
        return None;
    }
    if control.has_shortcut(InputActionConfig::SelectFavoriteSong) {
        Some(ResultFavoriteKind::Song)
    } else if control.has_shortcut(InputActionConfig::SelectFavoriteChart) {
        Some(ResultFavoriteKind::Chart)
    } else {
        None
    }
}

pub(super) fn result_favorite_hints(
    library_db: &LibraryDatabase,
    finished: &FinishedPlaySession,
    chart_id: Option<i64>,
) -> Result<FavoriteHints> {
    let mut hints = FavoriteHints::new(&finished.summary.title, &finished.summary.artist, "");
    if let Some(chart_id) = chart_id
        && let Some(chart) = library_db.list_charts_by_ids(&[chart_id])?.pop()
        && chart.sha256 == finished.result.chart_sha256
    {
        hints.folder = chart.folder_path;
    }
    // A hash lookup could pick another copy's folder. Never substitute the
    // selected row or a different chart when the played chart is unavailable.
    Ok(hints)
}

impl WinitApp {
    pub(super) fn handle_result_favorite_control(&mut self, input: &ResolvedInput) -> bool {
        let Some(kind) = result_favorite_kind(input.scene()) else {
            return false;
        };
        if input.pressed() && !input.repeat() {
            self.toggle_result_favorite(kind);
        }
        true
    }

    pub(super) fn toggle_result_favorite(&mut self, kind: ResultFavoriteKind) {
        if !matches!(self.view_state(), AppViewState::Result)
            || self.result.finished_course.is_some()
            || self.result.result_exit.is_some()
            || !self.result_input_ready()
        {
            return;
        }
        let Some(finished) = self.result.finished_play.as_ref() else {
            return;
        };
        let sha256 = finished.result.chart_sha256;
        let hints = match result_favorite_hints(
            &self.boot.library_db,
            finished,
            self.play.last_started_chart_id,
        ) {
            Ok(hints) => hints,
            Err(error) => {
                tracing::error!(%error, "failed to resolve result favorite target");
                return;
            }
        };
        let result = match kind {
            ResultFavoriteKind::Song => {
                if hints.folder.is_empty() {
                    return;
                }
                toggle_favorite_song_for_hints(
                    &self.boot.library_db,
                    &mut self.boot.collection_db,
                    sha256,
                    &hints,
                    now_unix_seconds(),
                )
            }
            ResultFavoriteKind::Chart => {
                self.boot.collection_db.toggle_favorite_chart(sha256, &hints, now_unix_seconds())
            }
        };
        match result {
            Ok(enabled) => {
                let toast = match (kind, enabled) {
                    (ResultFavoriteKind::Song, true) => "toast-favorite-song-added",
                    (ResultFavoriteKind::Song, false) => "toast-favorite-song-removed",
                    (ResultFavoriteKind::Chart, true) => "toast-favorite-chart-added",
                    (ResultFavoriteKind::Chart, false) => "toast-favorite-chart-removed",
                };
                match kind {
                    ResultFavoriteKind::Song => self.result.result_favorite_song = enabled,
                    ResultFavoriteKind::Chart => self.result.result_favorite_chart = enabled,
                }
                self.play_system_sound(crate::system_sound::SoundType::OptionChange);
                let text = Localizer::new(self.boot.profile_config.ui.locale());
                self.show_left_overlay_toast(text.text(toast));
                tracing::info!(?kind, enabled, title = %hints.title, "favorite toggled from result");
            }
            Err(error) => tracing::error!(%error, ?kind, "failed to toggle favorite from result"),
        }
    }
}
