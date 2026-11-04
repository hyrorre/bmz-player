# 単曲スコアの独立ベストと既存DB修復

## 原因と変更

`upsert_score_best`はEX改善または同EXのBP / CB / comboタイブレーク成立時に、
代表プレイの数値で独立ベストまで上書きしていた。またタイブレークの比較対象が
`score_best`の集約済みBP / CB / comboだったため、代表プレイを正しく比較できなかった。

数値のMIN / MAX更新、代表履歴更新、ランプ更新を分離した。代表比較は
`best_score_history_id`の実際の履歴を使い、同値なら既存の判定内訳・ghost等を保つ。
代表履歴の欠損時は集約値による同EX比較を避け、EX改善時に代表を設定する。
clear-onlyの中立行は最初の通常プレイでBP / CBを初期化し、EX=0でも履歴を設定する。

migration v33は既存のtransaction / user_version管理を使い、ScoreKeyごとの履歴から
MIN(bp) / MIN(cb) / MAX(max_combo)だけを修復する。他列と履歴には書き込まない。
履歴のないキーは保全する。数値履歴はNOT NULLで、0も有効な記録として扱う。

## 関連経路の確認

- beatoraja / LR2 / LR2orajaの単曲importは`insert_score`経由で共通修正が適用される。
  外部DBのminbpを代表プレイに流用せず、判定内訳から通常履歴を作る既存仕様は維持する。
- IR downloadは`insert_score_with_source`経由で同じ更新処理を使う。
  sourceによる重複防止、IR identityの採番、履歴の照合は変更しない。
- `reconcile_imported_score`は数値を変更せず、代表履歴IDに限定してdevice / 空ghostを
  補完するため変更不要。
- cleanupの`rebuild_score_aggregates`は既に独立MIN / MAXと実際の履歴による辞書順を
  分離していた。保存結果との一致を、実際のcleanup経路を通すテストで検証する。
- コースstageは通常と同じFull保存を通る。コース自体のベストやサーバーは変更しない。
- Selectは`best_scores_for_charts`、Resultは保存前後のBestScoreSummaryを使い、
  スキンへも独立したBEST値が渡る。代表履歴由来の配置情報も既存仕様を維持する。
- `should_send_ir_score(UpdateScore)`は保存前のEX / lamp / BP / CB / comboのいずれかの
  改善を判定する既存仕様を維持する。修復はローカルDBの3列だけで、enqueueを行わない。

現在の仕様と既存ユーザー向け説明は[スコア保存仕様](../../docs/score-persistence.md)を参照。

## 検証

Debian 13、Rust stable 1.99.1、FFmpeg等のネイティブ開発依存を入れた検証用コンテナを使用。
検証ログは作業環境の`/workspace/bmz-*.log`に置き、コミットしない。
画面の実機操作と外部IRへの実送信は未実施。

- `cargo fmt --all -- --check`: 成功。
- `cargo check -p bmz-player --locked`: 成功。
- `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`: 成功。
- `cargo test -p bmz-player --locked --lib storage::score_db`: 71件成功（追加9件を含む）。
- `cargo test -p bmz-player --locked`: libで2818件成功、30件失敗、41件ignore。
  失敗の内訳は日本語を前提としたassertが4件、未取得submoduleの存在assertが25件、
  rootでは読み取り不可にならないディレクトリ検査が1件。スコア関連の失敗はない。
- 日本語ロケールでlibの同一テストバイナリを実行し、上記スキン依存のprefixと
  rootで成立しない権限検査を除外: 2822件成功、失敗0件、36件ignore、31件filter。
  filterには当初失敗した25件のほか、同prefix内のignore 5件と権限検査1件を含む。
- 日本語・非rootで権限検査の成功を確認。この実行ではD-Busの接続確立で9件が
  `BrokenPipe`となったが、rootでの上記2822件にはこれら9件の成功も含む。
- `storage::migration`を同じlibテストバイナリで単独実行: 11件成功。
  v33の修復・保全・区分分離・再実行・rollbackのテストはscore DBの71件に含む。
- `cargo test -p bmz-player --locked --bins` / `--doc`: 成功（どちらも0件）。

submodule取得を試みたが、容量上限でcheckoutに失敗したため取得物を削除した。
そのため第三者スキンの実アセットによる検証は未実施。check / Clippyも一度容量不足で
中断したが、キャッシュと取得物の整理後に上記コマンドを再実行して成功を確認した。
これら環境依存の失敗を今回の修正に起因する失敗とは扱わず、無関係なコードは変更しない。

自己レビューでは、migrationのSET列が3項目に限られること、通常保存が代表比較前の
履歴値を使うこと、各UPDATEの全ScoreKey条件、ghost等を代表変更時のみ更新することを確認した。
