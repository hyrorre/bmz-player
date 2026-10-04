# 正の入力オフセットで判定表示が点滅する問題

## 原因

入力変換は打鍵時刻に `input_offset_us` を加算し、JudgeEngineはその時刻を
`JudgementEvent.time` に保持する。従来は表示用snapshotにも同じ時刻を渡していた。
正の入力オフセットでは判定表示の開始が未来になり、新しい判定でタイマーが負になる。
通常のtime=0開始のskin destinationは開始前に非表示になるため、打鍵ごとに
前の判定が消え、オフセット相当の時間が経つと再表示されていた。

修正前の現行コードをビルドして入力変換とskin描画評価を呼び出し、+20msでは
打鍵直後・5ms後・19ms後に画像がなく、20ms後に復帰することを確認した。
0msと-20msでは同じ非表示期間は発生しなかった。特定の外部スキンに依存しない。

## 修正

- `DisplayJudgementEvent` に表示専用の `display_time` を追加。
- 人間入力に伴う判定は、入力オフセット補正前の打鍵時刻で演出を開始する。
  処理時の現在時刻は使わず、入力配送や描画停止による遅延を維持する。
- 見逃し・autoplay・replay・対戦相手には元の判定時刻を使う。
- 判定画像・コンボ・ボムを同じ表示時刻へ接続し、表示履歴の800ms保持期間も
  表示時刻で計算する。採点履歴と表示履歴の件数がずれても採点時刻へ戻さない。
- `JudgementEvent`、判定種別・FAST/SLOW、結果保存・replay、キー音、
  順序付き `SkinRuntimeEvent` の内容・時刻は変更しない。

現在の時計と表示履歴の契約は [gameplay-runtime.md](../../docs/gameplay-runtime.md) を参照。

## 検証

回帰テストは外部アセットを使わず、合成譜面と最小JSONスキンで実行する。

- ±20ms / 0msで2回連続打鍵し、判定画像・コンボ・ボムが表示され続けること。
- 打鍵後0 / 5 / 19 / 20 / 30msで画像のアニメーション位置が進むこと。
- 入力配送が50ms遅れても、元の打鍵時刻が保持されること。
- ±500msでも表示履歴が打鍵から800msで期限切れになり、採点時刻へ戻らないこと。
- 判定種別・delta・結果時刻・EX・replayの記録時刻を保持すること。
- LN/CN/HCNのreleaseと、見逃し・autoplay・replayの表示時刻を区別すること。

Windowsで次の検証を実施し、すべて成功した。

- `cargo fmt --check`
- `cargo check --workspace --locked`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- `cargo test --workspace --locked --no-fail-fast`

追加した5テストはすべて成功。対象crateはbmz-gameplayが235成功、bmz-playerが
2114成功・6 ignored、bmz-renderが667成功・4 ignored。既存の外部アセット依存テストの
成功件数を、この不具合の外部スキン互換確認の根拠にはしていない。
検証ログはGit管理外の `.local/input-offset-workspace-{check,clippy,test}.log` に保存。

報告された外部スキンでのGPU描画・実打鍵による目視確認は未実施。
