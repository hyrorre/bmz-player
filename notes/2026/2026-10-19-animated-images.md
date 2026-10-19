# WebP・APNG・アニメGIFの対応

## 背景

beatoraja master（exch-bms2/beatoraja、2026-06-30 `389aa5e` "PixmapResourcePool : support
animated webp (test)"、`f5faa5e`）がWebP静止画とanimated WebPに対応した。

- `PixmapResourcePool.loadPictureResource`は`.webp`をFFmpegの`FFmpegFrameGrabber`で全フレーム
  decodeし、`toSpriteSheetPixmap`で1枚のスプライトシートにする。格子は`getSpriteSheetGrid`が
  フレーム数を割り切る配置のうち幅と高さの差が最小のものを選び、上限は`GL_MAX_TEXTURE_SIZE`
  （取得失敗時4096）。フレーム時間は使わない。
- フレーム数・格子は`PixmapResource`に保持されるが、利用箇所は無い。スキンは`divx` / `divy` /
  `cycle`で動かす前提になる。BGA・STAGEFILE等もシート全体を1枚の画像として表示する。
- BGAの`pic_extension`にwebpは無く、宣言どおりに存在する`.webp`だけを読む。
- APNG / アニメGIFはlibGDX / ImageIOが既定画像・先頭フレームだけを読む。

BMZは`image` crateの`webp` featureが無効で、拡張子のgateでも`.webp`を拒否していた。
APNG / GIFは既定画像・先頭フレームのみ表示していた。

## 決めた仕様

ユーザーと次のとおり合意した。

| 用途 | WebP / APNG / GIF（2フレーム以上） |
|---|---|
| スキン画像source | beatorajaと同じ格子のスプライトシート。GIF / APNGのシート化はBMZ独自 |
| BGA | フレーム時間で再生（BMZ拡張） |
| BANNER / BACKBMP、Select / Decide / ResultのSTAGEFILE | 表示開始からフレーム時間で再生 |
| PlayのSTAGEFILE | Loadingは初回1周、READYは`playstart`と初回1周の長い方を待つ |

PlayのSTAGEFILEは、未マージのPR #20 commit `0404fb6`（Kaleid LOADINGFILE / READYFILE）の
待機・ループ規則と実装を移植した。LOADINGFILE / READYFILEヘッダ自体は追加していないが、
`PresentationImages`のloading / ready 2スロットはそのまま残した。

判断した点:

- 格子の上限は当初16384px固定としたが、レビューで4096 / 8192pxのGPUに収まる画像まで
  読み込めなくなる点を指摘された（2026-10-20追記）。decodeとシート生成を分け、uploadする
  デバイスの`max_texture_dimension_2d`で格子を決めるように変更した。GPU生成前にinstallした
  スキンは、デバイス接続時にシートを組み立ててsourceサイズを更新する。
- delayは10ms未満を10msとする（0404fbの規則）。ループは総再生回数に揃える。GIFは
  `image`の`loop_count`がNETSCAPE拡張なし（`Finite(0)`）を無限に潰すため、`gif` crateで読む。
- 1フレームの画像は静止画経路のままとし、STAGEFILEの待機も追加しない。既存譜面の1フレームGIFに
  delayがある場合にロードが延びるのを避けるため（0404fbは1フレームGIFもdelayを待つ）。
- WebPの補完は、beatorajaの画像拡張子で見つからない場合だけ最後に探す（BMZ拡張）。

## 実装

- `bmz-render/src/assets/animated.rs`: `AnimatedImage` / `DecodedImage`と3形式のdecode。
  上限（64MiB、8192px、4096フレーム、RGBA 256MiB、10秒）はアニメ判定後だけ適用する。
- `bmz-render/src/assets/sprite_sheet.rs`: `getSpriteSheetGrid`の移植とシート生成。
- `bmz-player/src/animated_bga.rs`: 動画BGAと共通の`active_bga_channels`で起点を決め、
  表示フレームが変わった時だけtextureを書き換える。
- `bmz-player/src/app/meta_image_animation.rs`: texture 20..22の最後の書き手のkeyを追跡し、
  別の画像を古いアニメーションで上書きしない。
- `bmz-player/src/play_presentation.rs`、`app/presentation.rs`: PlayのSTAGEFILE待機。
  READY開始とchart開始を分け、延長中のAUTOPLAY / Replayは等速にする。
- 仕様は[譜面の画像](../../docs/chart-images.md)と[スキンのImage Sources](../../docs/skin.md#image-sources)。

## 検証

2026-10-19、Linux（Ubuntu 24.04のcontainer、GPUなし）で実施した。

- fixture: Pillow 12.3で2×2の`webp_anim.webp`（3フレーム、loop 2）、`apng_anim.png`
  （loop 3）、`webp_still.webp`、`apng_single.png`（既定画像＋1フレーム）を生成し、
  `data/songs/bga-compat`へ追加した。`bga-compat.bms`に`#BMP06 webp_anim.bmp`と
  `#BMP07 apng_anim.png`を追加した。
- `image-webp`は不透明フレームのalpha合成に/256の近似を使い、255が254になる。
  テストは±1を許容する。
- 同梱スキン（default、Rmz-skin、mz-select、Luxez-Flat）にアニメーション画像は無く、
  シート化による表示の変化は無い。
- 結果は本ファイル末尾の「検証結果」に記載する。

## 検証結果

- `cargo fmt --check`、`cargo check --workspace --locked`、
  `cargo clippy --workspace --all-targets --locked -- -D warnings`は成功。
- `cargo test --workspace --locked --no-fail-fast`は4340成功、6失敗、39無視
  （submodule初期化済み）。失敗は次の6件で、画像処理とは無関係。
  - 選曲・設定の表示文言4件：containerのロケールが英語で、日本語の文言を期待している。
  - `scan_song_roots_skips_unreadable_subdirectory_and_keeps_accessible_files`：
    rootで実行したため、読めないフォルダを作れない。
  - `tar_path_traversal_and_cross_platform_escapes_are_rejected`：楽曲ダウンロードのtarパス検証。
  - 変更前のcommit `59e456a0`でも同じ環境で同じ6件が失敗することを確認した。
- 主な追加テスト：格子計算と配置、3形式のフレーム・delay・総再生回数、上限とpolicy、
  skin sourceのシート寸法（JSON / LR2）、BGAのフレーム切替とPOOR優先、メタ画像の起点・
  scene reset・keyの照合、Selectのcache上限、PlayのSTAGEFILE待機と再生、
  動画出力の延長READY中の譜面固定、Playでのsource `100`の差し替え。

## レビュー指摘の修正（2026-10-20）

PR #36のレビューで指摘された4件を修正した。

- BGAのメモリ上限: 上限はcanvasサイズで判定していたが、BGAは256×256へpaddingしてから
  保持するため、16×16の画像では判定の256倍を確保できた。`FrameLayout`で保持サイズを
  判定し、各フレームをdecode直後にpaddingする。予算を超える時点でdecodeを止める。
- Selectのcache: 解放したアニメーションが`Ready`のまま残り、再選択しても戻らなかった。
  保持・解放済み・再decode中・静止画を区別し、解放済みは表示時にworkerで再decodeする。
  容量はLRU順の台帳と合計値で管理する。
- GPU上限: 16384px固定の格子では、4096 / 8192pxのGPUに収まる画像も読めなかった。
  decodeとシート生成を分け、upload先デバイスの`max_texture_dimension_2d`で格子を決める。
  GPU生成前のinstallはRendererがフレームを保留し、接続時に組み立ててsourceサイズを直す。
- 同一素材の複数チャネル: textureを素材ごとに1つしか持たず、開始時刻の異なるチャネルが
  フレームを共有していた。開始時刻が異なるチャネルだけチャネル専用textureを使う。

## 未確認事項

- 実機（Windows / macOS / Linux）での表示、GPU上の動画出力。
- GIF / APNGを含む第三者製スキン（LR2を含む）での見え方。`divx` / `divy`なしで使う
  スキンはシート全体が表示される。
- 大きなアニメーション画像のdecode時間・メモリ量の計測。
- 実GPU（上限4096 / 8192 / 16384px）でのスプライトシート表示と、GPU生成前にinstallした
  スキンのsourceサイズ更新。
