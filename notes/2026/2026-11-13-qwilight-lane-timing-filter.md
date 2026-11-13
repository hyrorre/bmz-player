# Qwilightのレーン別FAST/SLOWと表示フィルター

## 原因と変更

`mainJudgmentMeter`の演出は、FAST/SLOW表示設定を適用する前の`SkinRuntimeEvent`を参照していた。
通常表示と同じ、`apply_fast_slow_display_filter`適用後の`DisplayJudgement`を参照するように変更した。
表示時刻も、入力オフセット補正前の表示用時刻に揃う。設定や第三者スキンのファイルは変更しない。

ユーザーの指定に従い、表示対象外の判定では前の演出を解除しない。
開始時刻も更新せず、前の演出はそのままスキンのフレーム数・FPSに従って0へ戻る。

| 条件 | 新しいレーン別演出 |
|---|---|
| AutoのPGREAT | 開始しない |
| 閾値モードのPGREAT、誤差の絶対値が閾値未満 | 開始しない |
| 閾値モードのPGREAT、誤差の絶対値が閾値以上 | 誤差の符号に応じて開始 |
| GREAT／GOOD／BAD | モード・閾値にかかわらず誤差の符号に応じて開始 |
| 見逃し・空POOR | 開始しない（従来どおり） |
| 表示対象の誤差0us | 中央フレーム |

閾値0msは全PGREATが表示対象。通常プレイとリプレイで条件を共通にする。
レーンごとに最後に表示した判定時刻を保持し、過去の表示履歴で演出を巻き戻さない。
再試行などで時刻が戻った場合は演出をリセットする。

現在の仕様は[スキン仕様](../../docs/skin.md#qwilightのプレイスキン)と
[設定](../../docs/settings.md)を参照。

## 検証

- Rendererの回帰テストで、対象外判定による演出の継続・非延長、レーンの独立性、過去の履歴の再取得、再試行を確認した。
- 合成Qwilightスキンを読み込み、BMZの表示フィルターから描画計画のフレーム選択まで確認した。通常プレイ／リプレイ、Auto、閾値0ms、5msの境界前後、PGREAT以外の計38条件を含む。
- `cargo fmt --check`、`cargo check --workspace --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`が成功。
- `cargo test --workspace --locked --no-fail-fast`が成功。検査ログはGit管理外の`.local/qwilight-lane-filter-{check,clippy,tests}.log`に保存した。
- 実ウィンドウでの目視・操作確認は未実施。
