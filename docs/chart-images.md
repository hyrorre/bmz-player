# 譜面の画像（BGA・STAGEFILE・BANNER・BACKBMP）

BMS / BMSONの`#BMP`（BGA）、`#STAGEFILE`、`#BANNER`、`#BACKBMP`で読み込む画像の形式と、
アニメーション画像の再生規則です。スキン画像（`source.path`）の扱いは
[skin.md](skin.md#image-sources)を参照してください。

## 形式と拡張子の補完

画像はPNG / BMP / JPEG / GIF / TGA / WebPを読み込みます。BGAは動画
（mp4 / wmv / m4v / webm / mpg / mpeg / m1v / m2v / avi、BMZ追加のmkv / mov）も使えます。

- 宣言どおりのファイルがあれば、そのまま使います（beatoraja互換）。`.webp`も同様です。
- 宣言したファイルが無い場合は、同じ名前の別拡張子を探します。BGAは動画、画像
  （jpg / jpeg / gif / bmp / png / tga）、最後にWebPの順です。STAGEFILE / BANNER /
  BACKBMPは画像、WebPの順です。
- WebPを補完候補に入れるのはBMZ拡張です。beatorajaは宣言された`.webp`だけを読みます。

## アニメーション画像（BMZ拡張）

2フレーム以上のanimated GIF / WebP / APNGは、ファイルのフレーム時間どおりに再生します。
beatorajaはanimated WebPをスプライトシート全体の1枚として表示し、GIF / APNGは先頭
フレームだけを表示します。

- 10ms未満のフレーム時間は10msとして扱います（GIFのdelay 0を含む）。
- ループ回数は総再生回数に揃えます。GIFはNETSCAPE拡張なしが1回、指定Nが初回＋N回、
  0が無限です。WebPのloop countとAPNGの`num_plays`はNが総再生回数、0が無限です。
- 有限回の再生が終わったら最終フレームを保持します。
- 1フレームの画像と、既定画像以外に1フレームしか持たないAPNGは静止画です。16bit APNGは
  既定画像を表示します。
- 上限はファイル64MiB、canvas 8192×8192、4096フレーム、展開後のRGBAが1画像256MiB、
  decode 10秒です。超えた画像は警告を出して先頭フレームの静止画にします。

### BGA

- Base / Layer / Layer2はBGAイベントの時刻、POORはそのPOORを表示させたBAD / POOR判定の
  時刻を起点に、譜面時刻で再生します。Viewerのseekやリトライでも同じ起点から計算します。
- 小さい画像の256×256 paddingは全フレームに適用します。Layerの黒抜きは静止画と同じです。
- フレームはCPUに保持し、表示フレームが変わった時だけtextureを書き換えます。保持量は
  譜面ごとに合計512MiBまでで、超えた画像は先頭フレームの静止画です。
- 1つの画像は1つのtextureを使います。同じ画像を複数のレイヤーで同時に使う場合は、
  POOR、Base、Layer、Layer2の順で優先するレイヤーの時刻で表示します。#SWBGAの
  キー連動レイヤーは、最後に更新したフレームを表示します。

### STAGEFILE・BANNER・BACKBMP

Select / Decide / Resultと、Play中のBANNER / BACKBMPは、表示され始めた時刻から再生します。
シーンが変わると先頭から再生し直します。待機は追加しません。Selectは展開したフレームを
1画像64MiB、合計128MiBまでcacheし、古いものから静止画に戻します。

PlayのSTAGEFILE（runtime source `100`）はKaleid beatのLOADINGFILE / READYFILEに近い
規則で再生し、ロードとREADYを待ちます。

| 状態 | 表示 | 待機 |
|---|---|---|
| Loading | 画像を供給できるようになった時刻から再生 | 通常のロード条件に加え、初回1周が終わるまで |
| READY | READY開始時に先頭から再生し直す | スキンの`playstart`と初回1周の長い方 |
| 演奏中・終了演出 | READYからの再生を続ける | なし |

- 初回1周は最終フレームの時間を含みます。2周目以降はファイルのループ指定に従い、
  READY移行とプレイ開始は2周目以降を待ちません。
- Decide中の先読み時間は含めません。静止画のSTAGEFILEは従来どおり待機しません。
- BGA OFFでも有効です。スキンにSTAGEFILEの描画が無くても待ちます。スキン自身の
  timer / op / 透明度で非表示になる場合も、表示は強制しません。
- 待機中もEscape等の中断操作を受け付けます（[controls.md](controls.md#中断とfailed)）。
- READYを延長する間、通常プレイのカウントダウンは等速にし、AUTOPLAY / Replayの
  再生速度は延長後に戻します。Practiceは既存の開始位置と速度を保ち、延長分だけ待ちます。
- Viewerのseamless入場とseekは待機を追加せず、既存のPlay時計に合わせて表示します。
- Play中はSTAGEFILE専用のtextureを使い、Select / Decide / ResultのSTAGEFILEは変えません。

## 動画出力

[CLI動画出力](video-export.md)でも同じ再生規則を使います。BGAは譜面時刻、BANNER /
BACKBMPはPlay入場0秒、PlayのSTAGEFILEはLoadingをPlay入場0秒として計算します。
動画出力のBGAは従来どおり256×256 paddingを行いません。

実装は[decode](../crates/bmz-render/src/assets/animated.rs)、
[BGA](../crates/bmz-player/src/animated_bga.rs)、
[メタ画像](../crates/bmz-player/src/app/meta_image_animation.rs)、
[PlayのSTAGEFILE](../crates/bmz-player/src/play_presentation.rs)です。
