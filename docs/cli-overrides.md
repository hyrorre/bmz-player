# CLI起動時オプション

通常プレイ・オートプレイ・Viewer・動画出力で共通のプレイ設定を指定できます。
設定ファイルへの保存は行いません。未指定項目にはprofile設定を使います。

```powershell
bmz-player "chart.bms" --window-mode windowed --window-size 1280x720 --arrange random --gauge hard --gas off
bmz-player "chart.bms" -a --window-mode borderless --gauge ex-hard --gas select-to-under --gas-bottom easy
bmz-player "chart.bms" -P --arrange mirror --hispeed 3
bmz-player export video "chart.bms" -o out.mp4 --arrange random --gauge hard --gas best-clear --seed 42
bmz-player "chart.bms" --arrange random --arrange-2p mirror --double-option flip
bmz-player monitors list
bmz-player "chart.bms" --gauge hard --print-effective-options
```

## 書式

- `--gauge hard`と`--gauge=hard`の両方に対応。選択値は大小文字を区別しません。
- 共通オプションと`--profile`はサブコマンドの前後どちらにも置けます。
- 同じ項目を重ねて指定したら後勝ち。不明な値や範囲外はエラーです。
- プレイを伴わない`songs`・`table`などには指定できません。
- `--print-effective-options`は設定値と明示したCLI上書きをJSON表示して終了します。起動・動画生成はしません。リプレイ記録とコース制約の適用前の設定確認です。

## 適用期間

| 対象 | PATHなし | PATHあり | Selectへ戻った後 |
|---|---|---|---|
| 画面設定 | 適用 | 適用 | 維持 |
| プレイ設定 | 警告して無視 | 指定譜面へ適用 | 破棄してprofile値へ戻す |

`--boot-play-sample`と`--boot-course`も再生対象の明示として扱います。
リトライとResultでは維持し、コースでは全ステージで維持します。Decideキャンセルやプレイ開始失敗でSelectへ戻った場合も破棄します。
Viewerの再生要求はプレイ設定を毎回置き換えます。停止・同じ要求の再開では維持します。
画面設定はそのプロセスで維持し、後のViewer要求で明示した画面設定だけ変更します。
Viewer IPCはv3です。更新前の常駐Viewerは終了してから新版を起動してください。

## 指定値

| オプション | 値 |
|---|---|
| `--window-mode` | `windowed` / `borderless` / `exclusive` |
| `--window-size` | `WIDTHxHEIGHT`。物理ピクセル、各辺1～16384。実効モードがwindowedの場合のみ |
| `--monitor` | `primary`または`monitors list`に表示されたID全体を引用符で囲んで指定 |
| `--arrange` / `--arrange-2p` | `off` / `mirror` / `random` / `r-random` / `s-random` / `spiral` / `h-random` / `all-scratch` / `random-ex` / `s-random-ex` / `f-random` / `mf-random` |
| `--double-option` | `off` / `flip` / `battle` / `battle-auto-scratch` |
| `--gauge` | `assist-easy` / `easy` / `normal` / `hard` / `ex-hard` / `hazard` |
| `--gas` | `off` / `continue` / `hard-to-groove` / `best-clear` / `select-to-under` |
| `--gas-bottom` | `assist-easy` / `easy` / `normal` |
| `--hispeed` | `0.01`～`20.0`。指定中はClassic・Floating無効の固定倍率 |
| `--hs-fix` | `off` / `start-bpm` / `min-bpm` / `max-bpm` / `main-bpm` |
| `--auto-scratch` | `on` / `off`。onはスクラッチレーンを自動演奏し、スコア・リプレイ保存を無効化 |
| `--bga` | `on` / `auto` / `off` |
| `--guide-se` | `on` / `off` |
| `--seed` | 符号なし64bit整数。P1は下位24bit、P2は次の24bit、BMS分岐seedは指定値 |

P1配置はP2にコピーしません。`-B`のG-BATTLEと`--double-option battle`は別の設定です。
コースの必須制約は引き続き適用されます。
リプレイでは表示設定だけ変更可能です。配置・seed・ゲージ・GAS・アシストの明示指定はエラーにします。
動画出力には画面設定を指定できません。解像度とFPSは従来の`--resolution`・`--fps`を使います。
