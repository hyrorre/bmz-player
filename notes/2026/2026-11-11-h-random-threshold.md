# H-RANDOMの閾値を125msへそろえる

対象commit: `65af7edd`の次。仕様は [スコア保存のH-RANDOMの閾値](../../docs/score-persistence.md#h-randomの閾値) を参照。

## 経緯

「H-RANDOMの効きが弱く、S-RANDOMと変わらない」という報告を調べた。

- 補正本体（`time_based_shuffle`）と設定の伝播は正しかった。
- 通常プレイでは`h_random_threshold_ms`が常に`None`で、`unwrap_or(100)`により100ms固定だった。
  変更する設定は無い。
- beatorajaは`Randomizer`でH_RANDOM / ALL_SCRの鍵盤閾値を`ceil(15000 / hranThresholdBPM)`とし、
  既定は120なので125ms。`ModeModifier`（7K→9K）も同じ125ms。
  BMZのbeatoraja replay取込と7K→9K変換はすでに125msを使っていて、通常プレイと食い違っていた。
- BMZのS-RANDOM（LM120Hz補正）は約66ms以内の連続を避ける。
  そのためH-RANDOMとの違いは間隔67〜100msの範囲だけになり、報告の「変わらない」と合う。

## 変更

- `DEFAULT_H_RANDOM_THRESHOLD_MS = 125`を追加。新しいプレイ（`play_start_options`、練習、
  `PlaySessionOptions::default`、seedだけのIRバトル対象）で明示して記録する。
- `None`の扱い（`unwrap_or(100)`）は過去のreplay再現のため残し、`LEGACY_H_RANDOM_THRESHOLD_MS`と名付けた。
- 7K→6Kの縦連回避を固定100msからH-RANDOMの閾値へ変更（`None`なら125ms）。
  7K→6Kはreplay・スコアを保存しないので、過去データの再現への影響は無い。
- NORMAL配置の`AppliedArrange`も閾値を保持するようにした。7K→6K（ALL-SCRはNORMALへ寄せる）の
  リトライや結果からも閾値を引き継ぐため。
- リザルトとプレイ中のSAME ARRANGEリトライで、適用済みの閾値を引き継ぐ。

## 類似補正の確認

| 箇所 | 閾値 | 判断 |
|---|---|---|
| H-RANDOM / ALL-SCRの鍵盤 | H-RANDOMの閾値 | 新規プレイは125msへ変更 |
| ALL-SCRのスクラッチ | 40ms | beatorajaの`SRAN_THRESHOLD`と同じ。変更しない |
| S-RANDOM / S-RANDOM-EX | 40ms（旧方式） / LM120Hz補正 | S-RANDOMの仕様。変更しない |
| 7K→9Kのスクラッチ振り分け | H-RANDOMの閾値（`None`なら125ms） | 既にbeatorajaと同じ |
| 7K→6KのScratch / KEY4移動 | 固定100ms | H-RANDOMの閾値へ変更 |
| EXTRA NOTE | `max(125ms, 30000ms / BPM)` | BPM連動の別仕様（[extra-note.md](../../docs/extra-note.md)）。変更しない |

## 検証

- `cargo fmt --check`、`cargo clippy -p bmz-player --all-targets --locked -- -D warnings`、
  `cargo test -p bmz-player --locked`が通過。
- 追加テスト: 110ms間隔の3和音でH-RANDOM（125ms）が閾値以内の縦連を作らないこと、
  閾値の記録が無い場合は100msのまま110msの縦連を許すこと、7K→6Kが閾値に従い境界も避けること、
  SAME / DIFFERENTリトライの閾値の引き継ぎ（ignored指定のWinitテストを単独で実行）。
- 実機でのプレイ感の確認は未実施。
