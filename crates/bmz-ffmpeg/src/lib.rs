//! ffmpeg-next の初期化を共通化する薄いクレート。
//!
//! `bmz-audio`（音声デコード）と `bmz-video`（動画 BGA デコード）が同じ
//! 初期化処理を共有するために使う。ffmpeg の初期化はプロセスで一度だけ
//! 行えばよいため、`OnceLock` で結果をキャッシュする。
//!
//! ffmpeg-next 自体の型はそれぞれの利用クレートが直接依存して使う。本
//! クレートは初期化という横断的関心ごとだけを担当する。

use std::sync::OnceLock;

static FFMPEG_INIT: OnceLock<Result<(), String>> = OnceLock::new();

/// プロセスで一度だけ ffmpeg を初期化する。
///
/// 初期化後に ffmpeg のログレベルを `Error` に下げ、libavcodec が出す
/// WARNING/INFO ログ（vorbis の "Could not update timestamps for
/// discarded samples." など）が stderr に表示されないようにする。
///
/// 2 回目以降の呼び出しはキャッシュされた結果を返すだけで副作用はない。
pub fn ensure_init() -> Result<(), String> {
    FFMPEG_INIT
        .get_or_init(|| {
            ffmpeg_next::init().map_err(|e| format!("ffmpeg init failed: {e}"))?;
            ffmpeg_next::log::set_level(ffmpeg_next::log::Level::Error);
            Ok(())
        })
        .clone()
}
