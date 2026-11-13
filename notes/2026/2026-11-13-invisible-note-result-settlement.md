# 不可視ノーツによる結果確定とCLEAR演出の遅れ

## 原因と精査

`b6b139a7`でCLEARを結果確定へ接続した後も、一部の譜面では開始が遅かった。
登録済みの「うらドラみたいにスカートめくるなっ！ [6K UE Ron]」では、
最終可視ノーツが129.893617秒、末尾の不可視ノーツが134.042553秒。
結果確定が不可視を含む`chart.end_time`を使用し、約4.149秒余計に待っていた。

- 不可視ノーツは通常判定・空POOR・地雷・HCNゲージの対象ではなく、fallbackキー音の切替に使う。
  採点確定後も`present_inputs_after_scoring_closed`からそのキー音を鳴らせる。
- 地雷は通常ノーツを処理済みでもゲージを変える。除外してはいけない。
- LN/CN/HCNは終端まで含める。特に見逃し・早離しHCNはノーツの判定が済んでも終端までゲージが変わる。
- 空POOR等の最大SLOW窓、負の入力オフセット、リプレイの処理済みtick、独立した対戦相手の確定待ちは維持する。
- 保存は採点を閉じた後の同一状態からscore・ゲージ履歴・判定詳細・replay・コース引継ぎを取得する。
  早期公開した結果を退出時に取り直さない既存の経路を維持する。保存可否・IR条件は変更しない。

## 修正

`JudgeEngine::result_end_time`で不可視だけを除く判定対象とLNペア終端の最大時刻を求め、
本人・独立した対戦相手の自然確定に使用する。判定対象が無い譜面のfallbackは従来どおり。
既存の`ChartIndex`へロード時に格納し、別の譜面や`Arc::make_mut`後は再計算する。
`chart.end_time`を変更しないため、表示上の曲長・BGM・不可視キー音・通常の退出待機は維持する。

現行条件は[gameplay-runtime.md](../../docs/gameplay-runtime.md#採点の確定とresultのゲージ履歴)、
CLEARとの接続は[skin.md](../../docs/skin.md#qwilightのプレイスキン)を参照。

## 検証

Windowsで合成譜面の結果確定テストを実施し、負オフセット付き空POOR、未処理リプレイtick、
後方の地雷、LN/CN/HCN終端、対戦側のtick、確定後のスコア不変と不可視キー音を確認。
実譜面は`BMZ_RESULT_SETTLEMENT_CHART`を指定したignoredテストで再importして採点した。
最初のノーツを見逃した後にオートで進め、通常クリアの自然確定を130.094597秒に確認した。
判定受付200msと更新間隔1msを含み、末尾不可視の134.042553秒を待たない。

gameplay workerの結果公開から終了まで採点結果・全ゲージ終端が不変であることを確認。
直接所有するGameSessionの非同期保存でも、末尾不可視より前にscore履歴を保存し、
保存用のゲージ終端・引継ぎ値が確定時の値と一致することを検査した。

- `cargo fmt --all --check`、`git diff --check`が成功。
- `cargo check --workspace --locked`が成功。
- `cargo clippy --workspace --all-targets --locked -- -D warnings`が成功。
- `cargo test --workspace --locked --no-fail-fast -- --test-threads=4`が成功。
  gameplay 279成功・2 ignore、player 3129成功・82 ignore、render 1038成功・25 ignore、skin 425成功・5 ignore。
  上記の実譜面テストはignored分を別途指定して成功。
- `cargo build -p bmz-player --locked`が成功。

ログはGit管理外の`.local/invisible-settlement-*.log`。
ビルドキャッシュのhard link警告は環境由来で、コードの警告抑制は行っていない。

実ウィンドウでのプレイ、キー音の聴感、実サービスへのIR送信、他OSは未確認。
