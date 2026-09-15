use super::*;

#[derive(Debug, Clone)]
pub(super) enum UpdatePrompt {
    Available(UpdateCandidate),
    Downloading(UpdateCandidate, Arc<crate::update::DownloadProgress>),
    Ready(UpdateCandidate),
    Preparing(UpdateCandidate),
    Error { message: String, candidate: Option<UpdateCandidate> },
    UpToDate,
}

impl UpdatePrompt {
    pub(super) fn candidate(&self) -> Option<&UpdateCandidate> {
        match self {
            Self::Available(candidate)
            | Self::Downloading(candidate, _)
            | Self::Ready(candidate)
            | Self::Preparing(candidate) => Some(candidate),
            Self::Error { candidate, .. } => candidate.as_ref(),
            Self::UpToDate => None,
        }
    }

    pub(super) fn candidate_version(&self) -> Option<&str> {
        self.candidate().map(|candidate| candidate.version.as_str())
    }

    pub(super) fn as_dialog(&self) -> UpdateDialog<'_> {
        match self {
            Self::Available(candidate) => UpdateDialog::Available(candidate),
            Self::Downloading(candidate, progress) => {
                UpdateDialog::Downloading(candidate, progress)
            }
            Self::Ready(candidate) => UpdateDialog::Ready(candidate),
            Self::Preparing(candidate) => UpdateDialog::Preparing(candidate),
            Self::Error { message, candidate } => {
                UpdateDialog::Error { message, candidate: candidate.as_ref() }
            }
            Self::UpToDate => UpdateDialog::UpToDate,
        }
    }
}

/// 左上へ短時間表示するトースト。
pub(super) struct LeftOverlayToast {
    pub(super) message: String,
    pub(super) shown_at: Instant,
}

pub(super) const LEFT_OVERLAY_TOAST_DURATION: Duration = Duration::from_secs(2);
