# RESULT BGMとサウンドセット（Discussion #29）

## 要件と実装

既存のBGMフォルダーへRESULT音源を追加する連動方式だけを実装する。
独立抽選、profileの新モード、フォルダーの移行、音源用設定ファイルは追加しない。
仕様は[サウンドセット](../../docs/system-sound.md)、操作案内は[controls](../../docs/controls.md)にまとめた。

- サウンドセット内に`clear` / `fail` / `a` / `aa` / `aaa`を追加できる。
- 補完用SEセット・既定音源にも`a` / `aa` / `aaa`を追加できる。
  同ランクBGM、汎用clear BGM、同ランクSE、clear SEの順で選ぶ。FAILEDはfail BGM、fail SEの順。
- Result表示の既存rank optionを使い、FAILED優先、ランク完全一致、汎用BGM、従来SEの順で選ぶ。
- 新しいRESULT音種を既存SoundIdの後ろへ追加し、既存SEの分類とIDを維持する。
- 新RESULT BGMは`.loop`付きならループ、通常名なら単発。再生方式を準備結果からmanagerへ渡す。
- SEをサウンドセット、従来SEセット、既定音源の順で補完し、破損した候補は次へ進める。
  RESULT入口音は配置先で区別し、サウンドセット内ではBGM、SEセット・既定音源では単発SEとして扱う。
- Selectを離れた後は適用済みセットを保持する。後着のセット準備結果による切り替えを防ぐ。
- Result終了フェード、画面遷移、コースRESULTへの移行で新BGMを停止する。
- 設定UIと6言語の説明を更新し、保存キーと既存のフォルダー構成を維持する。
- Playの動画出力にもSE補完を適用し、使用しないRESULT BGMはロードしない。

## 回帰テスト

- 既存21種類と追加3種類のSE / RESULT BGM上書きとSE / defaultへの補完。
- 拡張子の大小文字、Windowsの従来のファイル名大小文字扱い、`.loop`版優先、RESULT BGMの探索範囲、SEのループ禁止。
- 壊れた上書きと`.loop`音源の補完、解析対象がBGMだけであること。
- 単発 / ループの実PCM出力、停止・フェード、BGM停止でSEが止まらないこと。
- BGM正規化と音量の実行中変更。
- ResultのA / AA / AAA境界、FAILED、ノーツ数0、ランク欠落時に下位ランクへ落とさないこと。
- ランク別SEの選択、汎用BGMの優先、FAILED時のランクSE抑制。
- 後着セットの適用条件と、全画面でのRESULT BGM停止対象。

## 検証

2026-10-09（JST）、Windowsで以下を確認した。

- `cargo fmt --check`: 成功。
- `cargo check -p bmz-player --locked`: 成功。
- `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`: 成功。
- `cargo test -p bmz-player --locked --no-fail-fast`: 2,316件成功、20件除外、失敗なし。
- `cargo test -p bmz-audio -p bmz-ffmpeg --locked`: bmz-audio 128件成功、3件除外、失敗なし。
  bmz-ffmpegと両crateのdoc testはテスト0件で成功。
- `cargo build -p bmz-player --locked`: 成功。
- DX12・サンプル譜面の自動プレイで、RESULT BGM付きセットとランク別SE補完セットをそれぞれ起動。
  音声出力開始、セット適用、Resultの120フレーム描画、正常終了を両方で確認した。

最初のsandbox内の全体テストでは、保存・ダウンロード・ディレクトリリンクなどの失敗と
ダウンロードテストの待ちが発生した。対象テストプロセスだけを停止し、通常権限で再実行して全件成功を確認した。
incremental cacheのhard linkを作れずコピーへ切り替える環境由来のwarningは残る。

起動確認では既存データに触れず、`.local/validation/2026-10-08-result-soundset/runtime-bgm`と
`runtime-se`に専用profile・DB・検証音源を作った。スクリプトは同ディレクトリの`run-smoke.ps1`、
実行ログはそれぞれ`logs/bmz-player.2026-10-08.log`、最終テストログは`.local/result-soundset-test-final.log`。
起動確認のmaster音量は0とし、手動聴取は行っていない。単発・ループ・停止・フェードは実PCMの回帰テストで確認した。
macOS / Linuxでの実機確認と、実音源を使った手動の聴取・リトライ・コース遷移確認は未実施。

## 2026-10-09 レビュー指摘の修正

対象は`22a1047d`のレビューで判明した2件。ユーザー指定により、正規化とファイル探索を
それぞれ`gpt-6-luna`のサブエージェントで修正し、親側で統合レビューと検証を行った。

- ファイル名はOSやファイルシステムに依存せずASCIIの大小文字を区別しない。
  `select` / `clear`によるセット検出、通常音源、`.loop`、拡張子に同じ規則を使う。
  大小文字違いが共存する場合は同拡張子内で正式な小文字名、残りはファイル名順とする。
  拡張子とループ版の優先順位は維持する。
- 正規化OFFで読み込んだセットをPlay / Result中にONへ変更した場合、
  セットの抽選・差し替えとは別に、現在のBGMの解析結果を反映する。
  SEの再読み込みやBGMの停止・再開を行わず、解析済みゲインだけを更新する。

追加解析は通常セットのロードと別のpending・世代で管理する。セット適用とprofile切替で古い解析を無効化し、
ロード時の実パス・ファイルサイズ・更新時刻と一致しない結果も破棄する。
音量反映は既存の設定同期を使い、Selectプレビュー中のフェードを維持する。

検証（Windows）:

- `cargo fmt --check`、対象crateの`check` / all-targets Clippy（`-D warnings`）/ `build`: 成功。
- `cargo test -p bmz-player --locked --no-fail-fast`: 2,319件成功、21件除外、失敗なし。
- `system_sound_normalization_poll_preserves_set_load_and_rejects_stale_results`:
  Winitのイベントループを使うためWindows限定・通常suiteでは除外し、`--ignored`指定の単独実行で成功。
  Play / Result中の解析適用、通常セットpending保持、古い世代とmanager不在の結果破棄を確認した。
- PCM回帰でゲイン更新後も次のサンプル位置から再生が継続することを確認した。
- 全体テスト後に追加した`disabled_prepare_can_analyze_only_loaded_bgms_and_reject_changed_sources`も
  単独実行で成功。実音源をOFFでロードしてからBGMだけを解析・適用し、SEの除外とファイル差替時の拒否を確認した。
- `cargo test -p bmz-audio -p bmz-ffmpeg --locked`: bmz-audio 128件成功、3件除外。
  bmz-ffmpegとdoc testも成功。
- 専用profile・DBへ`SELECT.WAV` / `Clear.LOOP.Wav`等を配置したWindows/DX12の起動確認が成功。
  音源のロード、音声出力開始、Resultの120フレーム描画と正常終了を確認した。master音量は0。

全テストのログは`.local/validation/2026-10-09-result-soundset-review/test.log`、
起動確認は同ディレクトリの`run-smoke.ps1`と`runtime/logs/bmz-player.2026-10-09.log`に保存した。
手動聴取とmacOS / Linuxの実機確認は未実施。
