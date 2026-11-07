# G-BATTLEで相手の判定が自分の判定表示へ混ざる問題

## 報告と再現

- WMIIの通常7KEYS用`play7ac.luaskin`で、コンボブレイク後もPGREATと大きなコンボが表示される報告。
- 提供ログはv0.5.1で、対象プレイはG-BATTLE。提供された同一譜面のリプレイは重複を除いて5件。
- 登録ライブラリのSHA-256照合で譜面をPourqaa [SP INSANE]と特定した。元ファイルのハッシュも一致した。
- 保存時刻2026-11-07 15:04:44 JSTのリプレイは、譜面時刻124.577秒で画像のEX 1457と
  PG 488 / GR 481 / GD 116 / BD 17 / PR 57 / MS 52に一致した。
  WMIIのPRはPOOR+EMPTY POOR、MSはBAD+POOR。その時点の自分のコンボは1、相手は1136。
- 同じリプレイの30.730秒では、自分のコンボが0なのに相手のPGREAT 160を描いていた。
  ローカルのWMII素材とオフスクリーンGPUでも確認した。画像の配置・カスタム素材設定の完全一致は検証していない。

## 原因と修正

対戦表示は5K/7Kの譜面を内部で10K/14Kへ広げ、相手の判定も表示履歴へ渡す。
判定領域数をスキンの`judge.index`だけから求めていたため、通常SPスキンの1領域へ
両プレイヤーの判定が入り、相手の判定で自分の判定文字・コンボ・FAST/SLOW・timerが更新された。

`bmz-render`のPlay描画で、5K→10K / 7K→14Kの対戦表示は2領域を確保する。
通常のスナップショット評価と、表示履歴が消えた後も値を保持するruntimeの両方が、同じ領域数を使う。
元譜面のキーモードが未設定のスナップショットでも、対戦相手を持つ10K/14Kは左右を分ける。

通常SP、対戦相手のいないDP、元から10K/14Kの譜面、9KEYSはスキン定義の領域を維持する。
専用スキンの`judge.index=1`とtimer 47/447には相手の状態を渡す。
判定エンジン、採点、ゲージ、保存用リプレイ、第三者製スキンは変更しない。

仕様は[Battle Session Skin](../../docs/skin.md#battle-session-skin)を参照。

## 検証

- 回帰テストは外部素材不要。修正前に相手コンボの混入で失敗することを確認した。
- 5K/7Kの通常スキンへのフォールバック、コンボブレイク、表示履歴の期限切れ、
  相手だけの入力によるtimer再始動の防止、自分の次のGREAT 1を確認した。
- 対戦専用の左右表示、SP、DPの共通1領域・左右2領域、元からDPの譜面に相手がいる場合、
  9KEYSの3領域、FAST/SLOW・タイミング差・判定timerを確認した。
- `cargo fmt --all --check`、`cargo check -p bmz-render --locked --offline`、
  `cargo clippy -p bmz-render --all-targets --locked --offline -- -D warnings`が成功。
- `cargo test -p bmz-render --locked --offline`は1006成功・11 ignored。
- 利用側の`bmz-player`は`battle`の62件、`screens::play_snapshot`の127件が成功。
  後者はCPUベンチマーク1件がignored。
- 提供されたG-BATTLE完走リプレイ2件で、コンボ0時の相手数字の描画を検査し、混入がないことを確認。
  最終EX・最大コンボ・判定数は修正前と一致した（EX 2007 / max combo 221、EX 1959 / max combo 170）。
- WMIIの30.730秒のオフスクリーンGPU画像は、修正後にPOOR・コンボ数字なしとなった。
- 実素材のCPU描画計画でも、WMIIの通常SP（`play7ac.luaskin`）、通常DP（`play14ac.luaskin`）、
  G-BATTLE版（`FHDPLAY_AC_gBattle.lr2skin`）、LR2 STANDARD（`play_7_battle.lr2skin`）を確認した。
  通常SPと1領域のG-BATTLE版は自分の判定、通常DPと左右2領域のLR2 STANDARDは左右それぞれの判定画像を描画した。
- Windowsの実ウィンドウ操作・手動プレイ、音声出力、macOS/Linuxは未確認。

個別ログ、提供リプレイ、プローブのソースとGPU画像はローカルの
`.local/wmii-report-20261107/`に保存する。これらと外部スキンはコミットしない。
