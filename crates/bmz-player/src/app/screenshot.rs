use super::*;

// Stay below common 255-byte filename limits, including the extension and suffix.
const MAX_FILENAME_BYTES: usize = 240;

#[derive(Default)]
pub(super) struct ScreenshotRuntime {
    pub(super) pending: Option<ManualScreenshotRequest>,
    /// Names handed to the asynchronous renderer but not yet present on disk.
    reserved_paths: HashSet<PathBuf>,
}

pub(super) struct ManualScreenshotRequest {
    dir: PathBuf,
    stamp: String,
    copy_to_clipboard: bool,
}

struct ScreenshotName {
    body: String,
    clear_type: Option<ClearType>,
}

impl ScreenshotRuntime {
    fn next_path(&mut self, dir: &Path, stamp: &str, name: &ScreenshotName) -> PathBuf {
        // Completed files are covered by the existence check below. Keep names
        // reserved while GPU readback / PNG encoding has not created them yet.
        self.reserved_paths.retain(|path| !path.exists());
        for index in 0_u64.. {
            let path = dir.join(screenshot_filename(stamp, name, index));
            if !path.exists() && self.reserved_paths.insert(path.clone()) {
                return path;
            }
        }
        unreachable!("screenshot filename sequence exhausted")
    }
}

impl WinitApp {
    pub(super) fn request_manual_screenshot(&mut self) {
        let copy_to_clipboard = self.boot.app_config.screenshot.copy_to_clipboard;
        self.screenshots.pending = Some(ManualScreenshotRequest {
            dir: screenshot_dir(
                &self.boot.app_config.screenshot.dir,
                &self.boot.app_paths.data_dir,
            ),
            stamp: screenshot_timestamp(now_unix_millis()),
            copy_to_clipboard,
        });
        let text = Localizer::new(self.boot.profile_config.ui.locale());
        let message = text.text(if copy_to_clipboard {
            "screenshot-saved-clipboard"
        } else {
            "screenshot-saved"
        });
        // Both the app-side request and renderer request suppress this toast
        // while the frame to be captured is being built.
        self.show_left_overlay_toast(message);
        self.notify_obs_save_recording(crate::obs::ObsRecordingSaveReason::OnScreenshot);
    }

    pub(super) fn submit_manual_screenshot(&mut self, scene: &AppSceneSnapshot) {
        let Some(request) = self.screenshots.pending.take() else {
            return;
        };
        // Course Decide replaces the displayed title with the course name;
        // filenames still identify the actual stage's chart.
        let decide_chart = self.play.pending_decide.as_ref().map(|decide| &decide.snapshot);
        let name = screenshot_name(scene, decide_chart);
        let path = self.screenshots.next_path(&request.dir, &request.stamp, &name);
        if request.copy_to_clipboard {
            self.renderer.request_screenshot_with_clipboard(path.clone());
            #[cfg(target_os = "linux")]
            if let Some(clipboard) = &self.ui.wayland_clipboard {
                self.renderer
                    .request_screenshot_with_clipboard_handler(path.clone(), clipboard.clone());
            }
        } else {
            self.renderer.request_screenshot(path.clone());
        }
        tracing::info!(
            path = %path.display(),
            copy_to_clipboard = request.copy_to_clipboard,
            "manual screenshot requested"
        );
    }
}

fn screenshot_name(
    scene: &AppSceneSnapshot,
    decide_chart: Option<&RenderSnapshot>,
) -> ScreenshotName {
    let (body, clear_type) = match scene {
        AppSceneSnapshot::Select(snapshot) => {
            let folder = sanitize_component(&snapshot.current_folder);
            let body = if folder.is_empty() {
                "Music_Select".to_string()
            } else {
                format!("Music_Select_{folder}")
            };
            (body, None)
        }
        AppSceneSnapshot::Decide(snapshot) => {
            let chart = decide_chart.unwrap_or(snapshot);
            (format!("Decide_{}", screenshot_title(&chart.title, &chart.subtitle)), None)
        }
        AppSceneSnapshot::Play(snapshot) => {
            (format!("Play_{}", screenshot_title(&snapshot.title, &snapshot.subtitle)), None)
        }
        AppSceneSnapshot::Result(snapshot) => {
            (screenshot_title(&snapshot.title, &snapshot.subtitle), Some(snapshot.clear_type))
        }
    };
    ScreenshotName { body, clear_type }
}

fn screenshot_title(title: &str, subtitle: &str) -> String {
    let parts = [sanitize_component(title), sanitize_component(subtitle)];
    let title = parts
        .iter()
        .filter(|part| !part.is_empty())
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("_");
    if title.is_empty() { "Untitled".to_string() } else { title }
}

fn sanitize_component(value: &str) -> String {
    let value: String = value
        .chars()
        .map(|ch| match ch {
            '\\' => '￥',
            '/' => '／',
            ':' => '：',
            '*' => '＊',
            '?' => '？',
            '"' => '”',
            '<' => '＜',
            '>' => '＞',
            '|' => '｜',
            ch if ch.is_control() => ' ',
            ch => ch,
        })
        .collect();
    value.trim().trim_matches('.').trim().to_string()
}

fn screenshot_filename(stamp: &str, name: &ScreenshotName, index: u64) -> String {
    let mut suffix =
        name.clear_type.map(|clear| format!("_{}", clear.as_str())).unwrap_or_default();
    if index > 0 {
        suffix.push_str(&format!("_{index}"));
    }
    suffix.push_str(".png");
    let max_body_len = MAX_FILENAME_BYTES.saturating_sub(stamp.len() + 1 + suffix.len());
    let mut end = name.body.len().min(max_body_len);
    while !name.body.is_char_boundary(end) {
        end -= 1;
    }
    let body = name.body[..end].trim_end_matches([' ', '.']);
    format!("{stamp}_{body}{suffix}")
}

fn screenshot_timestamp(millis: u128) -> String {
    let seconds = i64::try_from(millis / 1000).unwrap_or(0);
    let date = crate::datetime::local_datetime(seconds);
    format!(
        "{:04}{:02}{:02}_{:02}{:02}{:02}_{:03}",
        date.year,
        date.month,
        date.day,
        date.hour,
        date.minute,
        date.second,
        millis % 1000,
    )
}

#[cfg(test)]
#[path = "screenshot/tests.rs"]
mod tests;
