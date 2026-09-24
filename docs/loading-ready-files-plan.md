# LOADINGFILE / READYFILE 実装仕様

Kaleid の `.local/KaleidBeat/documents/format-header.html` のロード演出仕様と、
ユーザー指定の待機・ループ規則に基づく。初期計画を実装内容で更新したもの。
BMS / BMC / PMS のテキストヘッダを対象とし、BMSON 独自フィールドは追加しない。

## 表示と待機

| 状態 | STAGEFILE スロットへ供給する画像 | 待機 |
| --- | --- | --- |
| Select / Decide / Result | 元の STAGEFILE | 従来どおり |
| Play の Loading | LOADINGFILE | 通常ロード条件と GIF 初回1周の両方が完了するまで |
| Play の READY | READYFILE | スキンの `playstart` と GIF 初回1周時間の長い方 |
| 演奏・Play 終了演出中 | READYFILE | 追加待機なし |

- LOADINGFILE / READYFILE は個別に STAGEFILE へフォールバックする。
  READYFILE がない場合に LOADINGFILE を流用しない。
- 静止画は追加待機なし。既存 STAGEFILE の GIF は従来の静止画表示のまま。
- GIF の初回1周は最後のフレームの delay を含む。1フレームの GIF もその delay を待つ。
  delay=0 は 10ms に補正する。
- 初回1周後は GIF のループ指定に従う。指定なしは1回、有限指定 N は初回＋N回、
  無限指定は無限に再生し、有限再生終了後は最終フレームを保持する。
  READY への移行・プレイ開始は2周目以降の完了を待たない。
- 起点は Play の該当状態で画像を供給可能になった時刻。Decide 中の先読み時間は含まない。
  同じ GIF を両方に指定した場合も READY 開始時に0から再生する。
- BGA OFF でも有効。スキンに STAGEFILE 描画がない場合も開始待ちは発生する。
  スキン自身の timer / op / 透明度で非表示になる場合、表示を強制しない。
- 欠損・破損・デコード制限超過・アップロード失敗では警告を出して STAGEFILE に戻す。
  該当画像の追加待機は解除する。STAGEFILE もなければ画像なし。

## 時計とライフサイクル

```text
READY開始 = 音源・BGA・画像準備、既存ロード演出、操作hold、Loading初回1周が完了
READY待ち = max(playstart, READYFILEの初回1周時間)
譜面時刻0 = READY開始 + READY待ち
```

GIF が通常 READY より長い場合、その差分だけ prepared 状態を保持し、その後に既存の
`-playstart` からの音声・gameplay カウントダウンを開始する。READY 音と timer 40 は
READY への切り替え時に開始する。演出待機中も描画・設定調整・中断を処理する。

READY GIF の開始待ちがある通常プレイでは、カウントダウンは1倍速とし、AUTOPLAY /
Replay の要求速度を待機完了後に反映する。GIF は Play の演出時計で進み、譜面の
ノーツ・BGM・BGA・判定・Replay 入力時刻は変更しない。
PRACTICE は既存の速度補正済み開始位置と速度を保持するため、通常 READY 部分の
実時間を保ったまま差分待機だけを加える。

通常開始、直接起動、リトライ、コース次曲は同じ処理を使う。PRACTICE は区間開始ごとに
演出をリセットし、指定した譜面開始位置を保持する。Viewer の seamless 入場・シークは
演出待ちを追加せず、既存の Play 時計に参加する。再読込では変更後の画像を読み直す。

動画出力では全画像を事前に読み込み、Loading の起点を Play 入場0秒とする。
ディスク待ちは出力せず、スキンのロード演出と GIF 1周の長い方を待つ。
READY の計算・フレーム選択はライブと共通。延長分では譜面表示も固定し、
映像と音声は同じ譜面時刻0を使う。

## 実装の配置

- `bmz-chart`: `IntermediateMetadata` / `ChartMetadata` に `loading_file` / `ready_file`。
  RANDOM / IF 解決後のテキストから抽出する。原文の `bms_headers` とは区別する。
  library DB の列追加は不要。動的分岐内のヘッダ変更は既存の制限に従う。
- `bmz-render/src/assets/presentation.rs`: CPU で RGBA 合成済みフレームと累積 delay を生成。
  `image` で透過・部分フレーム・disposal を扱い、`gif` でループ拡張の有無と回数を読む。
- `bmz-player/src/play_presentation.rs`: 画像パス解決、独立フォールバック、再生位置、
  初回1周待ち、フレーム変更時のアップロード。ライブと動画出力で共有する。
- `bmz-player/src/app/presentation.rs`: 非同期読み込み、preload generation と譜面の照合、
  結果の受け取り。中断・別曲移動で古い結果を適用しない。
- `bmz-player/src/app/play_loop_flow.rs`: ロード待ち、READY 切替、追加待機と音声開始。
- `bmz-render`: Play snapshot にだけ画像・寸法の上書きを渡し、runtime source `100` に供給。
  専用テクスチャを使うため、Select / Result の元 STAGEFILE を上書きしない。

パスは譜面フォルダを基準に、空白・日本語・Windows 区切りと既存の同名別拡張子解決に対応。
同じパスを両ヘッダに指定した場合はデコード結果を共有する。同じ譜面オブジェクトの
再利用ではデコード結果を保持し、通常の再入場時に再生位置をリセットする。

上限はファイル64MiB、画像8192×8192、4096フレーム、保持する RGBA 合計256MiB／画像。
GIF のフレーム間で経過時間10秒も検査する。これはフレーム途中を強制中断する期限ではない。
GPU は現在フレームだけ保持し、同寸法なら既存テクスチャを再利用する。

## 検証

自作 fixture の自動テストで次を確認する。

- ヘッダの独立性、未指定、重複、大小文字、日本語・空白、RANDOM 採択枝。
- GIF の可変 delay、最終 delay、1フレーム、0 delay、有限・無限・指定なしループ。
- 透過・部分フレーム・disposal、破損、寸法制限。
- Loading の初回1周、READY での再生位置リセット、リトライ、片側失敗のフォールバック。
- Play の source 100 上書き・サイズ変更と、Play 以外への元画像復帰。
- 動画出力の延長 READY、譜面カウントダウン、終了までの AUTOPLAY。

2026-09-24 の検証:

- `cargo fmt --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets` 成功。
- `cargo test --workspace --no-fail-fast`: 3421成功・4失敗・6無視。
  失敗は既知の `select_lua_skins_decode_with_explicit_library_root_when_available`、
  `wmii_fhd_play_lua_features_when_available`、
  `wmii_fhd_play_stage_draws_follow_scene_modes_when_available`、
  `wmii_beatoraja_branch_next_rank_updates_when_available`。今回追加した13テストは成功。
- macOS の一時プロファイル・自作譜面で通常 AUTOPLAY が Result まで完走。
  GPU を使った動画出力でも Loading 1秒、READY 3秒（スキン指定1秒）の切り替え、
  以降のループを出力フレームから確認。最初の音は5秒で、演出4秒＋既存の譜面先頭余白1秒と一致。

実譜面を使う Windows / Linux での画面確認、全スキンの見え方・大容量 GIF の性能測定は
追加の手動検証事項。IR 送信可否は PR #20 の Kaleid 拡張全体のポリシー策定で扱い、
この表示機能によって BMS-IR / rianIR の送信ルールは変更しない。
