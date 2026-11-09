# 書庫確認中のダウンロード一覧のpanic

## 条件と原因

作業開始時に取得した`origin/main`は`015942f402d551ca3b2a0cc62591a80c175f1881`でした。
HTTPの「取得した書庫を展開せず登録する」を有効にすると、転送後の`song_archive::inspect()`中は
タスクが`DownloadState::Validating`になります。状態の判別値は8ですが、一覧の集計配列は8要素で、
状態を直接添字に使うため、この状態を集計すると範囲外アクセスでpanicします。

一覧は通常150msごとに再集計するため、確認中の更新タイミングによって発生します。
単曲・一括取得のどちらにも影響し、フィルタ適用前に集計するため「完了」の表示中でも発生します。
報告者の設定とログは未確認であり、確認できた条件付きクラッシュを修正するものです。

## 修正と回帰テスト

状態の数値を添字にする配列を、待機・実行中・完了・失敗・キャンセルの名前付き集計へ置き換えます。
ワイルドカードのない`match`で全状態を分類し、状態追加時の集計側の対応漏れをコンパイルエラーにします。
終了件数は完了・失敗・キャンセルの合計だけとし、Validatingは実行中に含めます。

`download_task_list.rs`のテストは`ChartDownloadManager`へタスクを登録して状態を設定し、
`egui::Context::run_ui()`から実際の`build_task_list()`を呼びます。
通信、実書庫、GPU、実ウィンドウは使わず、一覧の再集計と描画を検証します。
状態遷移・フィルタ変更時は既存の`refreshed = None`を使い、sleepを使いません。
新規タスク追加では件数変更による自動再集計も確認します。

取得方法、並列数、保存方式、キャンセル・再試行、DB登録の仕様は変更しません。
関連仕様は[譜面ダウンロード](../../docs/chart-downloads.md)、
開発手順は[development.md](../../docs/development.md)を参照してください。

## 検証

検証環境はDebian 13 / x86_64、Rust 1.99.1、既定featuresです。
RustとLinux開発用パッケージは作業領域へ配置し、システムへのインストールは行っていません。
ビルド容量を抑えるためdev/testのdebug情報を無効にしています。

修正前に次のUI回帰テストを追加し、正常描画を期待して実行しました。

```bash
cargo test -p bmz-player --locked --lib ui::download_task_list::tests::validating_task_renders_in_download_list -- --exact --nocapture
```

実際の`build_task_list()`内で`index out of bounds: the len is 8 but the index is 8`となり、
1件失敗しました。集計修正後に同じコマンドで1件成功し、`Checking archive`ラベルの描画まで確認しました。

`cargo test -p bmz-player --locked --lib ui::download_task_list::tests -- --nocapture`は5件成功しました。
新規4件でValidatingの実UI描画、全9状態・全6フィルタ、
`Downloading → Validating → Scanning → Completed`、新規追加による再集計を確認します。
全状態が1件ずつの場合、総数9、待機1、実行中5、完了1、失敗1、キャンセル1、終了件数3です。
既存の大量一覧の仮想化テストも成功しています。

`cargo fmt --check`、`cargo check -p bmz-player --locked`、
`cargo clippy -p bmz-player --all-targets --locked -- -D warnings`は成功しました。

`cargo test -p bmz-player --locked`の初回実行は2,959件成功、29件失敗、42件ignoreでした。
失敗はOSの英語既定言語に対して日本語を期待する既存テスト4件と、未取得の同梱スキンを必要とする25件です。
無関係なソースは変更せず、[開発手順](../../docs/development.md#同梱アセット)に従って3つのスキンsubmoduleを
固定commitへ初期化し、`LANGUAGE=ja cargo test -p bmz-player --locked`で再実行しました。
再実行は2,988件成功、失敗0件、42件ignoreで、他のテストターゲットとdoc-testも成功しました。
GPU等を必要としてignoreされるテストや、同梱外の素材がない場合に早期returnするテストの
実素材を使った確認は未実施です。

実サービスからのダウンロード、実アプリのウィンドウ操作、Windows / macOSでの実行は未実施です。
報告にある最小再現コードの再実行も行わず、今回は実際の一覧UIを通る自動テストで検証します。

## 2026-11-08 レビュー後の追加修正: 登録スレッドの作成失敗

周辺経路のレビューで、`app/background_jobs/song_scan.rs`が登録用スレッドを
`spawn(...).expect("failed to spawn song scan thread")`で開始していることを確認しました。
ダウンロード後の登録もこの経路を通るため、OSのリソース不足等でスレッドを作成できないと
呼び出し元がpanicします。元のmainにも存在し、Validatingの集計とは別の条件付き不具合です。
報告者がこの条件に該当したかは未確認です。

レビュー時には、子プロセスだけに`RLIMIT_NPROC=0`を設定し、同じ`spawn/expect`の最小コードが
終了コード101でpanicすることを確認しました。実アプリを起動した再現ではありません。

追加修正では、スキャン開始を既存の`SongScanRuntime`の小さなメソッドへまとめ、
スレッド作成エラーを通常の登録エラーとして返します。登録キューから取り出し済みの対象タスクを
`finish_scan`で失敗にし、原因をエラー欄へ記録して進捗表示を解除します。
起動に成功した場合だけ`pending`を保持し、呼び出し元は既存のスキャン失敗通知を表示します。
待機中の別スキャン、対象外のダウンロード、保存済みのファイルは維持します。
登録対象なしと判定した失敗にはしないため、保存済みの取得物がある場合の再試行は既存の再スキャン経路を使います。

新しい2件の回帰テストでは、OS全体の制限を変更せず、実際に利用する開始処理へ
`WouldBlock`エラーを返すスレッド作成関数を渡します。通信・実アーカイブ・GPU・実ウィンドウは不要です。

```bash
cargo test -p bmz-player --locked --lib app::background_jobs::song_scan::tests -- --nocapture
```

`expect`を残して開始処理を抽出した段階では、正常なエラー処理を期待する2件とも
`failed to spawn song scan thread`でpanicしました。エラー処理を修正した後、同じ2件が成功しました。
複数の登録タスクの失敗化・終了時刻・エラー原因、進捗とpendingの解除、待機キューの保持、
対象外タスクへの非干渉、手動再試行、失敗後の次のスレッド起動と結果受信を確認します。
ダウンロードを伴わないライブラリスキャンの開始失敗も確認します。

追加修正後に`cargo fmt --check`、`cargo check -p bmz-player --locked`、
`cargo clippy -p bmz-player --all-targets --locked -- -D warnings`が成功しました。
`LANGUAGE=ja cargo test -p bmz-player --locked`は2,990件成功、失敗0件、42件ignoreで、
他のテストターゲットとdoc-testも成功しました。元の一覧UIテスト5件と、保存済み書庫の
再スキャン・削除後の再取得を確認する既存テストも成功しています。
実サービス・実アプリの手動操作・実アプリでのOSリソース不足・Windows / macOSの確認は未実施です。

## 2026-11-09 再レビュー対応: 状態分類の共通化とスキャン開始の保留

一覧の実行中分類が集計とフィルタに重複していたため、全状態を網羅する
`DownloadBucket::from_state()`へまとめました。1回の状態取得・分類結果を集計とフィルタで共有し、
状態追加時にこの分類を更新すれば両方へ反映されます。全9状態・全6フィルタを実UIで検証するテストを維持します。

スレッド起動失敗後に後続スキャンが更新ごとに失敗していく動作について、保留する方式への変更依頼を受けました。
11月8日に追加した「対象タスクを即座に失敗にする」動作を置き換え、対象タスクはScanningのまま保持します。
失敗したスキャンをFIFOの先頭へ戻し、5秒の再開期限までキューを取り出しません。
新規要求も同じFIFOへ追加し、選曲のメンテナンス許可・実行中スキャン・再開期限の確認を必ず通します。
スキャン開始に成功するまで登録要求を保持するため、保留中に合流した同一書庫の譜面も次の開始時に含まれます。

起動失敗が続く間の通知は初回だけとし、開始成功時に通知の抑制と再開期限を解除します。
全6言語へ「保留し、指定秒数後に再試行する」通知を追加しました。
実行開始後のDBエラーや通信エラーは引き続き失敗として処理し、自動では再試行しません。
現行仕様は[譜面ダウンロード](../../docs/chart-downloads.md)へ反映しています。

保留を期待する回帰テストを先に追加し、変更前には期待するScanningに対してFailedになることを確認しました。
その後、時間を引数で進める検証へ拡張しました。実時間のsleepやOS全体のリソース制限は使いません。
複数タスクと保存済みファイルの保持、5秒未満の反復更新、後続・新規要求のFIFO、後着タスクの登録引継ぎ、
開始成功後の登録完了、実行中の二重起動防止、連続失敗時の期限更新と通知抑制、復旧後の通知再開を確認します。
ダウンロードを伴わないライブラリスキャンのscope・対象・forceも保留中に維持することを検証します。

変更後の保留・再開テスト2件と一覧UIテスト5件が成功しました。
`cargo fmt --check`、`cargo check -p bmz-player --locked`、
`cargo clippy -p bmz-player --all-targets --locked -- -D warnings`も成功しました。
`LANGUAGE=ja cargo test -p bmz-player --locked`は2,990件成功、失敗0件、42件ignoreで、
他のテストターゲットとdoc-testも成功しました。スキャンの合流、保存済み書庫の再試行、
全言語の翻訳キーと引数の整合性を検証する既存テストも成功しています。
実サービス・実アプリの手動操作・実アプリでの連続したOSリソース不足・Windows / macOSは未確認です。

## 2026-11-09 追加レビュー対応: 起動試行の上限

Scanningのキャンセルは実行中のDB登録を保護するため無効であり、開始を無期限に保留すると
リソース不足が続く間、手動再試行もできないことを確認しました。開始失敗回数を
`QueuedSongScan`ごとに保持し、初回を含め最大5回、5秒以上の間隔で開始を試みます。
5回目も失敗すると、後着分を含む当該スキャンの対象だけを既存の`finish_scan`でFailedへ移し、
登録要求を取り除きます。失敗回数とOSの原因はタスクのエラー欄、ログ、失敗通知に残します。
保存済みのファイルと取得結果は維持し、手動再試行では既存の再スキャン経路を使います。

要求の上限到達後も共通の5秒待機は維持し、後続が次のフレームから失敗し続ける動作を防ぎます。
開始成功時だけこの待機と保留通知の抑制を解除します。新しいスキャン要求の失敗回数は0から始まり、
保留中の同じ書庫へタスクが合流しても既存要求の回数はリセットしません。
実行中のScanningをキャンセルできない仕様や、DBエラーの処理は変更しません。

ライブラリ全体のスキャンでは、保存した設定を公開してからスレッドを開始する順序を保ちます。
`save_app_config`と`publish_song_scope`は各試行で繰り返しますが、今回の上限によって
起動失敗に伴う無期限の反復を解消します。設定保存のタイミングの変更は行いません。

上限の回帰テストを先に追加し、変更前は5回失敗後も先頭要求が残るため失敗しました。
修正後に同じコマンドで成功しています。

```bash
cargo test -p bmz-player --locked spawn_failure_limit_releases_library_scan_but_keeps_cooldown
```

エラー注入と引数の時刻を使い、実時間のsleepやOS全体のリソース制限に依存しません。
5回目の成功と失敗、上限後の5秒待機、次の要求の独立した上限、後着タスクも含む失敗処理、
無関係な登録要求とファイルの保持を検証します。保存済み書庫と展開済みフォルダの手動再試行は、
通信を使わず完了結果を通常の受信経路へ渡すmanagerのテストで確認します。

変更後のスキャン開始テスト4件、保存済みファイルからの手動再試行テスト1件、一覧UIテスト5件が
対象を指定した実行ですべて成功しました。`cargo fmt --check`、`cargo check -p bmz-player --locked`、
`cargo clippy -p bmz-player --all-targets --locked -- -D warnings`も成功しました。
`LANGUAGE=ja cargo test -p bmz-player --locked`は2,993件成功、失敗0件、42件ignoreで、
他のテストターゲットとdoc-testも成功しました。環境・同梱素材の条件は前節と同じです。
実サービス、実アプリの手動操作・OSリソース不足、Windows / macOSは未確認です。

## ローカル検証ログ

[記録の規約](../README.md)に従い、保存先はここへ集約します。以下は作業環境だけの一時ファイルで、
リポジトリには含まれず、共有も恒久保存もされません。再現コマンドと結果は各節に記載しています。

| 検証 | ローカル保存先 |
|---|---|
| 一覧UIの修正前後・全件 | `/tmp/bmz-regression-{before,after}.log`、`/tmp/bmz-download-list-tests.log` |
| スレッド作成失敗の修正前後・crate検証 | `/tmp/bmz-scan-spawn-{before,after,fmt,check,clippy,test}.log` |
| 保留方式の修正前後 | `/tmp/bmz-scan-hold-{before,after}.log` |
| 分類共通化のUI・保留方式のcrate検証 | `/tmp/bmz-classification-ui.log`、`/tmp/bmz-review-followup-{fmt,check,clippy,test}.log` |
| 起動上限の修正前後・回帰テスト・crate検証 | `/tmp/bmz-scan-limit-{before,after,focused,fmt,check,clippy,test}.log` |
