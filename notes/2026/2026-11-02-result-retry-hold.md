# リザルト退出時のKEY5 / KEY7 hold判定

## 目的と仕様

KEY5 / KEY7を押した瞬間の動作をKEY1 / KEY3に揃え、実際にリザルト画面を退出する瞬間のhold状態でリトライを決めます。
操作の仕様は[controls.md](../../docs/controls.md#リザルト画面)に記載しています。

- KEY5だけをholdしていれば別配置、KEY7だけまたは両方をholdしていれば同じ配置でリトライします。
- 退出演出中のKEY5 / KEY7押下は、KEY1 / KEY3と同じスキップ要求です。timer 2のアニメーション終端と最終フレーム表示を待ちます。
- Enter / Escapeやスキンの時間経過による退出でもhold判定を行います。
- Rによる同配置リトライ、コース曲間・フォルダ内Autoplayの次曲移動は各操作を実行します。

## 実装

変更前もレーンキーによる退出はholdでリトライを判定していました。ただしKEY5 / KEY7は退出演出のスキップ対象外で、Enter / Escape / 自動退出はholdを参照していませんでした。

`app/result_support.rs`のスキップ対象にKEY5 / KEY7を加えました。
`app/result_flow/ending.rs`で通常退出を単曲またはコースのhold判定に通し、遷移直前に物理入力から押下状態を再構築します。
複数の割り当てのうち1つだけを離した場合や、focus loss / backend再同期による状態の消去も反映します。

## 検証

Windowsで以下が成功しました。

- `cargo fmt --check`
- `cargo check -p bmz-player --locked`
- `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`
- `cargo test -p bmz-player --locked --lib app::tests::result::`: 59成功、3 ignore。スキップ対象レーンとholdによる配置を含みます。
- `cargo test -p bmz-player --locked --lib app::tests::course::`: 18成功。
- `cargo test -p bmz-player --locked --lib result_retry_uses_holds_at_exit_after_taps_skips_and_physical_resets -- --ignored --test-threads=1`: 1成功。独立したWinit event loopとoffscreen GPUで、短押し、退出演出中のスキップ、最終フレームでのrelease、Enterによる退出からのholdリトライ、複数入力とfocus lossを確認しました。音声出力を用意しない環境なので、リトライの読み込み開始と配置オプションまでを確認し、音声再生の開始は検証していません。
- `cargo test -p bmz-player --locked --lib course_retirement_and_retry_preserve_results_and_cancel_pending_launches -- --ignored --test-threads=1`: 1成功。通常退出からholdでコースリトライする経路と、失敗時の結果復元を確認しました。
- `bun test bmz-ir-web/tests/manual.test.ts`: 8成功。

全件の`cargo test -p bmz-player --locked -- --test-threads=2`は、既存の`profile_switch_keeps_rendering_old_skin_until_commit_and_on_failure`で起動するWindowsゲームパッドbackend（gilrs-core）のスレッドが`SendError`でpanicし、プロセスが終了しました。
このテストを除外した実行でも、既存のIR mock HTTP通信テスト`later_page_failure_keeps_and_reports_pages_already_imported`と`banned_account_stops_the_batch_without_repeating_refresh`が60秒以上停止したため中断しました。全件成功とは扱っていません。
詳細ログはGit管理外の`.local/result-retry-*.log`です。

- 実際のコントローラー操作、表示ウィンドウでの手動確認、macOS / Linuxでの確認は未実施です。
