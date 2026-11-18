# oraja_helper の named pipe 連携

- 作業日: 2026-11-18
- BMZ ブランチ: `feat/oraja-helper-pipe`
- helper ブランチ: `feat/bmz-player-pipe`（隣接する `../oraja_helper`）
- 現在の仕様: [oraja_helper 連携](../../docs/oraja-helper.md)
- ユーザー指定によりコミットせず、両リポジトリへ差分を残す。

## 参照と実装

`../lr2oraja-endlessdream/core/src/bms/player/beatoraja/external/OrajaHelperClient.java`
と `stream/NamedPipeSender.java`、`play/BMSPlayer.java`、
`../oraja_helper/named_pipe_receiver.py` と `oraja_helper.pyw` を確認した。
既存の4イベント `song_select` / `song_play` / `song_play_end` / `song_result` を使い、
BMZ からは `source: "bmz-player"` を追加する。

選曲は選択 SHA-256 と画面退出を使って重複送信を抑える。
プレイ開始は preload 後の `install_active_play`、終了は Play 退出と quick retry の
active session 破棄直前、正式結果は単曲 Result の設置時に送る。
終了時の判定は描画 snapshot ではなく `RuntimeResult` / `FinishedPlaySession` を使う。
レーン配置は BMZ の source-per-destination 配列を beatoraja の番号へ変換する。

pipe の I/O は最大64件のキューを持つ専用 worker で実行する。
受信側が1件ごとに閉じるため、busy に加えて pipe の再作成待ちも扱う。
1イベントは最大1秒、キューに2秒以上残ったものは破棄し、ゲーム進行を待たせない。

helper 側では BMZ モードを除外していた初期化・受信条件を変更し、
source と選択中の player mode を照合する。
Play→Play の quick retry でも新しい試行の時間・終了メトリクス状態をリセットする。
既存の DB 監視、近接重複排除、統計反映の経路は利用を続ける。
helper リポジトリの既存の `config.py` / `settings.py` / `oraja_helper.pyw` の変更は保持した。

## skills とサブモジュール

開始時にステージ済み差分はなかった。
`.claude/skills` 3件は Windows から辿れない Linux 形式の reparse point のため
削除表示になっていた。ユーザーの依頼に従い、該当リンクだけを除去して
Git の既存の `core.symlinks=false` に従って checkout し、Git 差分を解消した。
Windows の checkout ではリンク先文字列を持つファイルになる。

スキン3件に作業中の変更がないことを確認し、`git submodule update --remote --checkout`
で追跡先を取得した。すべて開始時の参照と一致しており、gitlink の変更はない。

- Luxez-Flat: `7f113ed4cb90ae9ba9b6223a9bd49a18b36e6999`
- Rmz-skin: `cb08901c80020f3ee65b201b797edea25db14c58`
- mz-select: `eb95b4f7d46fc4ca90929e30353fe6d74f9146ff`

## 検証

Windows で対象 crate の check と all-targets Clippy（`-D warnings`）を実施。
通常の並列 `cargo test` は Windows のページングファイル不足でコンパイルに失敗した。
`-j 1` の全テストでは 3,308件成功、92件ignored、2件失敗。
失敗は一時 named pipe 作成の拒否と、既存動画テスト
`skin_video_hidden_frames_keep_playback_time_without_uploading_or_reopening` の
`first video frame timed out` だった。

その後の切り分けでは、pipe 作成拒否は sandbox 外でも再現した。
テストが再作成のたびに `first_pipe_instance(true)` を要求していたことが原因で、
helper 本体と同じ通常の再接続条件へ修正した。最終の
`cargo test -p bmz-player --locked -j 1 --lib oraja_helper -- --nocapture` は
sandbox 外で4件すべて成功し、4イベントの再接続・pipe不在期間からの接続・
未起動時の打ち切りを確認した。動画テストも単独で再実行して成功した。
全件の再実行は行っていない。

helper 側は既存の `.codex-venv/bin/python -B -m unittest test_pipe_events.py` を
WSL 経由で実行し、送信元の振り分けと Play→Play の試行リセットの2件が成功した。
ユーザー管理の `.venv` は変更していない。

実際の helper GUI と OBS 表示、物理入力による完走・quick retry、
macOS / Linux ビルドは未確認。
