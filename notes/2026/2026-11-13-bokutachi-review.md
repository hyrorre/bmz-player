# Bokutachi PR #86のレビュー対応

対象は[PR #86](https://github.com/hyrorre/bmz-player/pull/86)の
[39879bfdへのレビュー](https://github.com/hyrorre/bmz-player/pull/86#pullrequestreview-5887192391)です。
日時は日本時間です。[現在の仕様](../../docs/bokutachi.md)と
[元のコース実装記録](2026-11-09-bokutachi-courses.md)も参照してください。

## 変更

- 最新main `725844f1`へrebaseしました。Result IRのスクロール領域と高さ制限を維持し、
  mainで追加された`local_submission_notice`の初期化にも追従しました。
- `ineligible`を一括claim・一括処理の未完了件数から除外しました。
  選択したジョブの個別再送だけで再照会し、CLIの`UntilFirstFailure`が同じ譜面を連続照会しないようにしました。
- pollの失敗を、受付済みの登録確認を続ける場合と、登録処理の失敗が確定した場合に分けました。
  後者も自動再POSTせず、個別再送でだけ再開します。固定した本文・時刻・外部IDを再利用し、
  同一アカウントを確認してから古いimport IDを退役済みとして保存します。
  古いreceiptが残っていても、再起動時に退役済みIDへ戻りません。
- 段位条件を満たさない新しいコース結果は、理由をログへ記録してenqueueを省略します。
  旧不適格ジョブの読み込みと、サーバーの未対応コース404の表示は維持します。
- 段位未更新を`not_cleared`として保存しました。結果イベントに型付きの状態を渡し、
  表示の決定から英語メッセージの部分一致を除きました。完了フィルターにも含めます。
- PMSの入力機器別BESTは`source_kind=Local`に限定しました。
  外部プレイヤーのDB取り込み・IRダウンロードに付く既定のKeyboardが、BMZの新しい成績を抑制しません。
  全体BEST、他IR、独立した各指標の集計、今回の履歴を除外する条件は維持します。

## 上流契約と再送の境界

Tachi `adabf2c`の
[poll-status](https://github.com/zkldi/Tachi/blob/adabf2c/typescript/server/src/server/router/api/v1/imports/router.ts)
を再確認しました。完了結果はimport、処理中はongoingを返します。trackerのFAILEDは記録された
statusCodeとmessage、job_queueのfailedは500、trackerもjobもない場合は404です。

登録をやり直せるのは、Tachiの`success=false`を伴うインポートの400系エラー、
120秒経過後の`There is no ongoing import here.`、
500の`An internal service error has occurred with this import.`です。
401/403/408/429は含めません。汎用500は失敗したtrackerと一時的なサービス障害を区別できないため、
GET確認を維持します。受信できなかったPOSTについて、厳密な一度限りの登録保証は追加しません。

## 検証

Debian 13、Rust 1.100.0で検証しました。開発ヘッダーはワークスペース内のsysrootへ配置し、
Cargoのdebug情報とincrementalを無効化しています。
Rust 1.100.0ではテスト実行ファイルが`target/debug/build/.../out`へ配置されるため、
既存の開発用アセット自動探索では同梱スキンを見つけられませんでした。
`BMZ_RESOURCE_DIR=$PWD/data`を指定すると該当するdefault skinテスト7件が成功します。
本PRで無関係な探索ロジックは変更していません。
初回実行時の追加テストの誤った件数期待も、実際のHTTP照会回数と保存状態の検証へ修正しました。

| コマンド | 結果 |
| --- | --- |
| `cargo fmt --check` | 成功 |
| `cargo check --workspace --locked` | 成功 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 成功 |
| `cargo build -p bmz-player --locked` | 成功 |
| `LANG=ja_JP.UTF-8 LANGUAGE=ja LC_ALL=ja_JP.UTF-8 cargo test --workspace --locked --no-fail-fast` | 初回bmz-player 3,159成功・4失敗・53 ignored。他crate・doc-testの失敗なし |
| `BMZ_RESOURCE_DIR=$PWD/data LANG=ja_JP.UTF-8 LANGUAGE=ja LC_ALL=ja_JP.UTF-8 cargo test -p bmz-player --locked bokutachi` | 修正後36成功・失敗なし |
| 同じ環境指定で`cargo test -p bmz-player --locked default_skin` | 7成功・失敗なし |
| `git diff --check` | 成功 |

初回の4失敗は、資源探索による3件と追加テストの期待値1件です。上記の再検証ですべて解消を確認しました。
実装の変更は初回workspace実行前に完了しており、その後のRust差分は追加テストの修正だけです。
`check`とClippyは`bmz-audio`の未使用依存`anyhow`について既存のCargo manifest警告を出しますが、終了コードは0です。
無関係な依存定義は変更していません。

参照スキンは固定commitのRmz-skin `cb08901c`、mz-select `a4ee0e1a`、Luxez-Flat `9e0d42e4`を用意しました。
生ログは環境内の`/tmp/bmz-review-*.log`で、コミット対象には含めません。

追加テストは、一括手動同期、明確な失敗後の個別再送、別アカウントでの再開拒否、
DB再オープンと古いreceipt、判別不能な500でGET維持、段位enqueueの省略、
表示用の状態通知、外部スコアを除いた入力機器別BESTを対象とします。

本番APIへのスコア投稿、GUIの実操作、Windows/macOS実機確認は実施していません。
HTTPモックと一時DBを使用し、実ユーザーのアカウントには投稿していません。
