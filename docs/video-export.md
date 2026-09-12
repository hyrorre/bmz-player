# CLI 動画出力

`export video` は Play スキンへの入場から退場までを、実時間から独立してフレーム単位で生成します。入場演出、READY、演奏、終了演出、フェードアウトを含みます。Decide／Result は含みません。

```powershell
bmz-player export video "D:\BMS\song\chart.bms" -o "out.mp4"
bmz-player --profile default export video "chart.bms" -o "replay.mp4" --replay-slot 1
bmz-player export video "chart.bms" -o "out.mp4" --resolution 2560x1440 --fps 120
bmz-player export video "chart.bms" -o "out.mp4" --fps 60000/1001 --seed 42
```

## 必要環境

- ウィンドウや音声出力デバイスは不要です。wgpu 対応 GPU は必要です。
- 外部 FFmpeg の `libx264` と `aac` エンコーダーを使用します。PATH に置くか、`--ffmpeg "C:\tools\ffmpeg.exe"` で指定してください。開始前にエンコーダーの有無を検査します。
- 出力先の親フォルダは事前に用意してください。生成中は同じフォルダの一時ディレクトリへ圧縮映像とステレオ f32 PCM（約23MB/分）を保存します。正常終了・通常のエラーでは一時ファイルを削除します。プロセス強制終了時は `.bmz-export-*` が残る場合があります。

## オプション

| オプション | 既定値・意味 |
|---|---|
| `-o` / `--output` | 必須。出力先 `.mp4` |
| `--profile ID` | 現在のprofile。読み込みだけで、active_profileは変更しません |
| `--resolution WIDTHxHEIGHT` | `1920x1080`。各辺2～8192の偶数、GPUの上限以内 |
| `--fps N[/D]` | `60`。1～240fps、有理数指定可 |
| `--replay-slot 1..4` | 未指定ならオートプレイ。現在のScoreKeyの保存スロットを使用 |
| `--seed N` | オートプレイのBMS分岐・配置の固定。未指定なら生成して記録 |
| `--ffmpeg PATH` | `ffmpeg` |
| `--overwrite` | 既存の動画・メタデータを上書き。未指定ではエラー |

スキン、配置、ハイスピード、ゲージ、音量などはprofileから取得します。リプレイ時は保存された配置・分岐・入力を優先し、`--seed`は併用できません。再現に必要なBMS分岐記録が不足するリプレイはエラーにします。物理入力向けオフセットと自動入力補正は無効にし、ノーツの表示オフセットは保持します。

## 時刻・終了条件

- 入場を0秒とし、メディアは生成前に読み込み済みとします。実機のロード待ち時間は動画へ含めません。
- READYは `max(loadstart,0) + max(loadend,0)` ミリ秒後に始まり、さらに `max(playstart,0)` ミリ秒後が譜面時刻0です。
- 通常のセッション終了条件を使用し、終了後は `finishmargin` とフルコンボ演出の残り時間の長い方を待ちます。
- フェードアウトは `fadeout` とtimer 2のアニメーション長の長い方です。両方0なら通常プレイと同じ500msです。FAILEDはtimer 3と`close`で退場します。
- 映像はフレーム番号から有理数で時刻を計算します。音声・判定は48kHzのサンプル時計で進み、映像FPSや処理待ちに依存しません。
- 退場時刻未満の映像フレームをすべて生成し、最後のフレーム境界まで音声を出力します。退場後の端数は無音です。映像との終端差は1音声サンプル未満です。
- BGAは対象時刻の表示画像が確定するまで待ちます。遅いGPUやエンコーダーでは生成時間が延び、フレームを捨てません。

## 成果物と保存

`out.mp4` は H.264（CRF 18、medium、yuv420p）＋AAC（48kHz、ステレオ、320kbps）です。`out.export.json` に譜面hash、seed・分岐、リプレイ、表示・音声設定、スキンパス、バージョン、フレーム数、音声サンプル数を保存します。IR認証設定は含めません。

library DBはメモリ内に作り、必要な既存score DBは読み取り専用で開きます。スコア・リプレイ・IRの保存処理やネットワーク取得は実行しません。IR／ライバルのオンライン目標は取得しません。

同じ入力・設定からの再現を目的とし、GPU・フォント・スキンの変更やエンコーダーバージョンをまたぐ画素／ファイルの完全一致は保証しません。スキン自身が呼び出し回数に応じて状態を変えるLua callbackは、FPSによって演出が異なる場合があります。

FFmpegの入出力仕様: [rawvideo・MP4](https://ffmpeg.org/ffmpeg-formats.html)、[libx264・AAC](https://ffmpeg.org/ffmpeg-codecs.html)。

## 開発時検証

```powershell
cargo test -p bmz-player video_export
cargo test -p bmz-video offline_selection
$env:BMZ_TEST_FFMPEG = 'C:\tools\ffmpeg.exe'
cargo test -p bmz-player generates_complete_mp4 -- --ignored --nocapture
```
