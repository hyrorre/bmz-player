# oraja_helper の named pipe 連携

## 2026-11-20: コース曲リザルトのランプ

コース各曲の `song_result` に生のクリア種別を渡していたため、保存時と同じ
`course_stage_clear_type` を使い、フルコンボ系以外を NoPlay に補正する。
単曲と `song_play_end` は元の結果を維持する。

## 2026-11-20: ED 版送信処理との比較

手元の `../lr2oraja-endlessdream` の HEAD `2f2598b4` の `OrajaHelperClient`、
`NamedPipeSender`、`MusicSelector`、`BMSPlayer`、`MusicResult` と BMZ の送信経路を静的に比較した。
レビューで参照された ED の `3bc4b9b4` は手元に存在せず、その版との完全一致は未検証。

- ED の `song_select` は `scoreBest` / `minBpBest` / `scores` を条件付きで送る。BMZ は送らず、helper の DB・履歴に依存する。
- ED の 9K オプション ID は `OPTION_PMS` で解釈する。BMZ の `optionId` は共通 ID で、特に 7〜9 の意味が一致しない。
- 未指定 seed は ED の `-1` に対し BMZ は `null` を送り得る。BMZ 固有の F-RANDOM / MF-RANDOM の ID も `null`。
- `song_play_end` は ED がメトリクスと判定を送り、BMZ はこれに score・ランプ・BP も追加する。BMZ は確定結果がなければ終了イベントを送らない。
- ED の再送は最大8回で待機間隔を延ばす。BMZ は専用 worker・64件の有界キュー・1件1秒・待機2秒上限で破棄する。
- ED は送信 payload と各試行を JSONL に記録する。BMZ はこれらの専用ダンプを行わず、送信失敗は debug、キュー破棄は種別に応じてログ出力する。
- BMZ は `source: bmz-player` と既定 OFF の設定を追加する。対応 helper は送信元と選択プレイヤーを照合する。
- ED の選曲保留は SongData のオブジェクト単位、BMZ は SHA-256 単位であり、同一ハッシュの別行へ移動したときは挙動が異なり得る。
- ED は FREQ trainer 使用時にリザルトのランプを NoPlay に補正する。BMZ は負の FREQ を送信対象外とし、0以上は BMZ の保存規則に従う。

両者の早期中断・短い FAILED 演出と終了メトリクスの確定タイミング、互換 helper の
実処理・各配置の全モードは実機比較していない。送信前後のタイミングや最終表示の完全一致は未確認。

## 2026-11-20: キュー満杯時のログと設定表示

満杯による選曲通知の破棄は debug、プレイ通知の破棄と worker 切断は warning に記録する。
設定 UI は Windows だけで表示し、他 OS でも設定値の保存・読み込みは維持する。

## 2026-11-20: 選曲通知と配置文字列の ED 互換

選曲が1秒間変わらないときだけ通知し、選択変更・フォルダ移動・画面退出で待機を解除する。
正規・MIRROR は各サイドの配置文字列を空にし、ランダム配置は従来の番号表記を維持する。

## 2026-11-20: コース各曲の通知

通常の手動コースでは各曲の開始・終了と曲ごとのリザルトを送る。
単曲結果が確定した後、コースの次曲または最終リザルトへの分岐前に一度だけ送信する。
コース全体のリザルト、保存無効・自動演奏・リプレイ・練習・Viewerは対象外とする。
FREQの減速でも保存無効となるため、開始・終了・リザルトで同じ保存無効フラグを使う。

## 2026-11-20: 連携の有効化設定

BMZ メニューの連携・録画に本体共通の設定を追加し、既存設定を含めて既定 OFF とした。
OFF ではイベント構築と worker 起動を行わない。無効化時は選曲状態を消去し、
待機中の通知を破棄する。送信中の通知は完了する場合がある。

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
