# コース途中の開始前リタイアとリトライ失敗

## 背景と修正範囲

`0fff0ae6`を基点に、コース2曲目以降のロード中・READY中にリタイアすると、
単曲の開始前キャンセルと同じ経路でSelectへ戻り、完了済みコース結果も破棄する問題を修正しました。
コース最終リザルトからのリトライで、譜面の再確認前に結果を消す問題も対象です。

## 変更

- 開始前リタイアの終了先を、完了済みステージの有無で分けました。1曲目開始前はSelect、
  2曲目以降は退出フェード後にFAILEDのコースリザルトへ進みます。
- 未開始曲は演奏済みステージに追加しません。既存の部分コース集約・保存を使い、
  完了済み結果、コース全体のノーツ数、未プレイ分のBPを保持します。
- 保留中の開始を消す処理とコース状態の破棄を分けました。リタイア時も先読み世代を更新し、
  遅着結果が次曲を開始しないようにします。
- コースリトライは、全ステージのsource・メタデータを確認してから旧結果を解放します。
  準備失敗時は結果画面の演出・入力・音声を再初期化し、元の結果とエラーを表示します。
- 曲間リザルトの次曲操作、コースでのクイックリトライ禁止、演奏開始後の中断、
  読み込みエラーでSelectへ戻る既存の動作は維持します。

beatorajaの参照ソースは開始前キャンセルでSelectへ戻ります。
コース途中の結果を保持する変更はBMZの動作です。
現在の契約は[操作](../../docs/controls.md)と[保存条件](../../docs/score-persistence.md)を参照してください。

## 検証

- `cargo check -p bmz-player --locked`、`cargo clippy -p bmz-player --all-targets --locked -- -D warnings`に成功しました。
- 追加したWindows専用のアプリ遷移テストに成功しました。手動 / Autoplay / Replay、
  1曲目 / 2曲目 / 最終曲の開始待ち、Escape / E1+E2長押し / E2+E3 / E4+E2長押しの
  36通りを確認しました。終了先、未開始曲の非加算、試行の保存回数、遅着結果の破棄を検証しています。
  同じテストで、R / KEY5相当のリトライ失敗後の結果保持と、同配置 / 別配置での1曲目再開も確認しました。
- このテストは隔離した一時DBとオフスクリーンGPUを使います。通常のテストではignoredのため、個別に実行しました。

  ```powershell
  cargo test -p bmz-player --locked course_retirement_and_retry_preserve_results_and_cancel_pending_launches --lib -- --ignored --test-threads=1
  ```

- `bun test bmz-ir-web/tests/manual.test.ts`は8件成功しました。6言語のマニュアルと仕様書のPrettier確認も成功しました。
- 制限環境ではASIO SDKの自動取得とテスト用DBの初期化が失敗したため、既存SDKを`CPAL_ASIO_DIR`で指定し、
  通常権限で検証しました。SDKの再取得や恒久的な環境設定変更は行っていません。
- `cargo test -p bmz-player --locked`はライブラリ2,696件成功、失敗0件、44件ignoredでした。
  別途実行した上記の遷移テスト1件も成功しています。startup stackの実機テスト2件はignoredのままです。
- `cargo fmt --check`、`git diff --check`に成功しました。最終検証ログは`.local/course-retire-*.log`です。
- 実入力・実音声・GPU上の演出、macOS / Linuxでの手動確認は未実施です。
