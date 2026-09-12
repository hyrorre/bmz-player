#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCommand {
    pub command: Command,
    /// `active_profile`を書き換えない、CLI呼び出し全体のprofile上書き。
    pub profile_id: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Export(super::VideoExportOptions),
    Run(AppOptions),
    Table(TableCommand),
    Songs(SongsCommand),
    Course(CourseCommand),
    Replay(ReplayCommand),
    Ir(IrCommand),
    Profile(ProfileCommand),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayCommand {
    Import { path: String, overwrite: bool, controller: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrCommand {
    /// `ir login --id X [--password Y] [--base-url URL] [--provider NAME]`.
    /// For BMS-IR, `--id` is the numeric player ID and `--password` is the game token.
    Login { email: String, password: Option<String>, base_url: Option<String>, provider: String },
    /// `ir logout [--provider NAME]`
    Logout { provider: String },
    /// `ir status`
    Status,
    /// `ir ranking <SHA256> [--ln-policy P] [--scope S] [--limit N]`
    Ranking { sha256: String, ln_policy: String, scope: String, limit: u32 },
    /// `ir sync` — pending のスコアジョブを送信する。
    Sync,
    /// `ir upload-local [--dry-run] [--limit N] [--sync] [--all]` — local score.db history を IR に投入する。
    UploadLocal {
        provider: Option<String>,
        limit: u32,
        dry_run: bool,
        sync: bool,
        all: bool,
        resend: bool,
        include_course_stages: bool,
        include_replay: bool,
    },
    /// `ir download-scores [--dry-run] [--limit N]` — IR の自分のスコアを score.db に取り込む。
    DownloadScores { provider: Option<String>, limit: u32, dry_run: bool },
    /// `ir attest-submitted [--provider KEY] [--all]` — 既送信scoreへ後付け署名する。
    AttestSubmitted { provider: Option<String>, sync: bool, all: bool },
    /// `ir cleanup-imported [--provider KEY] [--apply]` — 再import済みの旧 Local 履歴と
    /// それに対応する local backfill IR score を削除し、集計を再構築する。
    CleanupImported { provider: Option<String>, apply: bool },
    /// `ir cleanup-duplicate <HISTORY_ID> [--provider KEY] --apply` — duplicate IR history を削除する。
    CleanupDuplicate { history_id: i64, provider: Option<String>, apply: bool },
    /// `ir rivals [add <PLAYER_ID> | remove <PLAYER_ID>]`
    Rivals { action: Option<RivalAction> },
    /// `ir device-key [rotate]` — 署名鍵の表示 / ローテーション。
    DeviceKey { rotate: bool },
    /// `ir replay <SCORE_ID>` — IR リプレイをダウンロードする。
    Replay { score_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RivalAction {
    Add { player_id: String },
    Remove { player_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableCommand {
    Add { url: String },
    List,
    Fetch { url: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SongsCommand {
    Add { path: String, recursive: bool, enabled: bool },
    List,
    Load { target: Option<String>, use_everything: Option<bool> },
    Reload { target: Option<String>, use_everything: Option<bool> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CourseCommand {
    Import {
        path: String,
    },
    List,
    /// `course history <COURSE_ID> [--limit N]` — print recent attempts.
    History {
        course_id: i64,
        limit: u32,
    },
    /// `course attempt <SCORE_ID>` — print per-chart breakdown of a single attempt.
    Attempt {
        score_id: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileCommand {
    List,
    Current,
    Use { id: String },
    Create { id: String, display_name: Option<String>, activate: bool },
    Copy { source_id: String, target_id: String, display_name: Option<String>, activate: bool },
}
use super::AppOptions;
