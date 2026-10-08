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
