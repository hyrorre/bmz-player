# Play画面の性能調査と修正方針（2026-09-21）

Rmz / ECFNを対象に、最初に **ノーツ描画用情報の事前解決とスキン構造のキャッシュ** を進める。
次にECFNの動画転送を検討する。後半の「段階的な実装と再計測」に、調査後の実装・比較結果を記録する。

## 計測条件

- 本体: `0b69b5387f4fbb2b2312ae04b23558766c5e1794`、通常のrelease build。
- MacBook Pro / Apple M4（CPU 10コア、GPU 10コア）/ 16GB / macOS 26.6.2。
- Metal、Windowed、Native、2944×1656。比較として1920×1080も実行。
- **VSyncOff、target_fps=0、frame_limit_in_background=0**。
  初期化ログで`requested=Immediate effective=Immediate`、frame latency 2を確認。
- Rmz: `data/skins/Rmz-skin/play7main.luaskin`、submodule `03445268974a46d0641b944f880653662b0801ba`。
  7Kのスキンオプション・ファイル選択は既定値。
- ECFN: `data/skins/ADFX02/ECFN/play/play7.luaskin`、現在のprofileの7Kオプション・ファイル選択。
- オートプレイ、seed 42、同梱sampleと自作の高密度7Kを比較。
  高密度譜面は180 BPM、12小節、鍵盤7レーン各小節16ノート、計1,344 tap / 84 notes/s。
  高密度譜面には音源・BGAを付けていない。ECFN自身のスキン動画は動作する。
- `BMZ_DATA_DIR`等を`/tmp/bmz-fps-20260921/<case>`へ分離。DBは新規、IR・OBS等は無効。
  master volumeのみ0（mixerは合成後のgain乗算なので処理を省略しない）。実ユーザーの設定・DBは変更しない。
- `bmz_player::play_profile=debug`の120フレーム集計を使用。
  gameplay worker開始2〜9秒の範囲内に全体が収まる集計ブロックを採用し、focusによるprofiler resetを跨ぐブロックは除外。
  主対象の各ケースは720フレーム / 約6秒。起動・ロード・Resultは除外。
- 各条件1回の探索的な実機測定。GPU実行時間・物理ディスプレイ表示間隔は未計測。
  ビルドは実機測定と同時に実行していない。

## 実機結果

単位はms/フレーム、平均。`描画待ち除外`は
`total_redraw_ms - surface_ms - present_ms`で、GPU送信API等を含むCPU側wall-time。
CPUの実行時間そのものやGPUの処理時間ではない。

| スキン・譜面 | 描画開始頻度 | 描画待ち除外 | plan | スキン動画処理 | text | snapshot消費・投影 | surface取得 |
|---|---:|---:|---:|---:|---:|---:|---:|
| Rmz / sample | 120.0 FPS | 1.236 | 0.477 | 0 | 0.056 | 0.047 | 6.977 |
| Rmz / 高密度 | 120.0 FPS | 1.478 | 0.599 | 0 | 0.057 | 0.080 | 6.734 |
| ECFN / sample | 120.0 FPS | 0.993 | 0.230 | 0.439 | 0.030 | 0.018 | 7.249 |
| ECFN / 高密度 | 120.0 FPS | 0.988 | 0.304 | 0.385 | 0.026 | 0.022 | 7.264 |

1920×1080でも両方120FPS。sampleの描画待ち除外はRmz 1.151ms / ECFN 0.891msだった。
比較用default skinは2944×1656のsampleでplan 0.109ms / 描画待ち除外0.758ms。

このMacではImmediateでもsurface取得が約7msを占める。`sample`のスタックでも
`Surface::get_current_texture → CAMetalLayer::nextDrawable`の待機が支配的だった。
設定がFifoへfallbackした結果ではないが、表示側の待ちの正確な原因は未確定。
**このFPS値からCPU最適化の効果や最大FPSを判定しない。** 描画待ち除外値の逆数も最大FPSとは扱わない。

ECFNの背景動画は`generic.mp4`（H.264、1280×720、60fps）。
スタックでは動画の`Queue::write_texture`からの`memmove`を確認した。
RGBAは1枚3.69MB、60枚/秒で約221MB/秒の転送データとなる。
`video_ms`はこの転送が大部分を占め、別スレッドの動画decode時間は含まない。
同じPTSの再転送防止と同じサイズのGPU texture再利用は既に実装されている。

## 表示待ちを除いた描画プランの反復計測

追加した`crates/bmz-player/examples/play_plan_profile.rs`は、実際のskin decode / runtime adapter /
`Renderer::render_scene_status`を使い、GPUを接続せず`plan_us`だけを測る。
合成snapshotに8 / 64 / 256 tap、固定のLN/HCN 2本と小節線2本を置く。
時刻・combo・score・判定演出を進め、各条件300フレームwarmup後に3,000フレーム計測する。
ウィンドウ、音声、DB、動画転送、文字ラスタライズ、ノーツ投影は測定対象外。

3プロセス反復の各統計の中央値。単位はµs。p95/p99は各run内の生サンプルから算出し、
全runを混ぜた分位ではない。

| スキン | 可視tap数 | plan平均 | p95 | p99 |
|---|---:|---:|---:|---:|
| Rmz | 8 | 107.1 | 120 | 152 |
| Rmz | 64 | 136.4 | 150 | 184 |
| Rmz | 256 | 236.0 | 254 | 296 |
| ECFN | 8 | 88.8 | 95 | 111 |
| ECFN | 64 | 132.4 | 141 | 161 |
| ECFN | 256 | 280.1 | 296 | 325 |

Rmzは176 destinations / 159 imagesでLua draw callbackが5個あり、ECFNは365 destinations / 265 imagesでruntimeなし。
8→256 tapでRmzは約129µs、ECFNは約191µs増える。
実機とはsnapshot・呼出頻度・CPU動作状態が異なるため、上表の絶対値を実機結果と直接比較しない。
今後同じfixtureで変更前後を比較するための基準とする。

## 修正の優先順位

### 1. ノーツ描画用のフレーム情報を一度だけ準備する

対象:

- `crates/bmz-render/src/skin/runtime/context/play.rs`: `note_rect_for_progress` / `note_body_rect`。
- `crates/bmz-render/src/skin/document_render/play/lane.rs`: `note_lane_area` / `notes_destination_offset`。
- `crates/bmz-render/src/skin/document_render/play/note.rs`: `note_part_render_item`。
- `crates/bmz-render/src/plan/play/document.rs`。

現在はノーツごとに`enabled_options()`のVecを作り、レーン定義を展開する。
さらにoffset適用時に`all_destinations()`を作り直して`notes`を探し、画像も線形探索する。
Rmzの実機スタックでも`notes_destination_offset → all_destinations`を確認した。

ロード時にnotes destination・ノーツ画像をindexへ解決し、フレーム開始時に
レーン矩形、note height、notes offset、alphaをまとめて求める。
各ノーツでは座標・UV・描画itemだけを生成する。可視ノーツ数×destination数の走査を除く。
LIFT / SUDDEN / HIDDEN / user offset / PMS拡縮等の変化は毎フレーム反映する。

### 2. スキン構造のキャッシュをLua runtimeと両立させる

対象:

- `crates/bmz-render/src/skin/runtime/context/scene.rs`: `static_document_play_items_split_for_state_and_text`。
- `crates/bmz-render/src/skin/runtime/context/types.rs`: `DocumentPlanningCache`。
- `crates/bmz-render/src/skin/document_render/core/static_render.rs` / `resolve.rs`。
- `crates/bmz-render/src/skin/animation.rs`。

現在は`lua_draw_runtime.is_none()`の場合だけplanning/static cacheを使う。
Rmzではruntimeが存在するため、callbackに依存しない要素までキャッシュを使えない。
またimage/valueのHashMap構築と、destinationの種類を探す各配列の線形探索が毎フレーム残る。
両スキンのスタックでstatic destination評価がplanの主要部分だった。

`SkinContext`に画像・value等のindexとdestinationの種類・参照先を保持する。
複数keyframeの条件展開・継承情報もロード時またはoptions変更時に準備する。
動的なtimer / op / offset / expression / Lua callbackはそのフレームで評価し、
callbackの呼出順・回数・closure stateを変えない。
スキンreload、options、source変更時のcache無効化を明示する。
スキーマ専用の`bmz-skin-document`にはrenderer用の依存を追加しない。

1と2は別の小さな変更として進め、まず1の高密度ケースの傾きを下げ、その後2の基礎コストを下げる。
改善率は実装前には保証しない。初回の評価目標は両スキンのplan平均・p95の低下と、
256 tapで少なくとも20%の短縮。届かなければ次の変更へ広げずプロファイルを取り直す。

### 3. ECFNの動画転送を独立して改善する

対象は`app/skin_flow/video.rs`、`bmz-render/src/renderer/gpu/texture.rs`、`bmz-video`。
まず転送バッファの確保・コピーを細分化して測り、再利用するstaging bufferの有界ringと現行
`Queue::write_texture`をA/B比較する。改善が乏しければYUV plane転送＋GPU色変換を検討する。
色空間・stride・動画loop・PTS・alphaの扱いまで変更範囲が広いため、1/2と分離する。
第三者スキンの動画・Lua・JSONは変更せず、解像度や動画fpsを落として数値だけ改善しない。

### 後回しにするもの

- snapshot clone / allocation: 存在するが今回の実機では消費・投影0.018〜0.080ms。
  長尺・DP・SCROLL/SPEED・大量LNでは再計測する。判定済みmapの全コピーも別スレッドの調査候補。
- egui: 非表示時のidle経路が既にある。今回0.018〜0.084ms。
- 文字描画: atlas / layout cacheが既にあり、今回0.026〜0.057ms。
- GPU描画方式の全面変更: geometry生成は0.004〜0.008ms程度、20〜32 draw steps程度。
  bind group cache、instance buffer / geometry scratch再利用、隣接batch化は実装済み。
  GPU timestamp等の裏付けなしにshader・atlas・描画順を大きく変えない。
- gameplay threadの周期・判定・音声時計: 今回の主な描画負荷ではないため変更対象外。

## 実装後の判定方法

1. CPU probeをreleaseで3回以上実行し、平均・p95・p99・command数を変更前と比較する。
2. JSON / Lua auto / Lua compatの既存テストと、同一stateのDrawPlan比較を使う。
   特にnotes offset、LN/CN/HCN、Mine、9K、DP、reload、dynamic timer、callback stateを守る。
3. `cargo fmt --check`、`cargo check`、`cargo clippy`、変更対象の
   `cargo test -p bmz-render` / `cargo test -p bmz-skin` / `cargo test -p bmz-player`を実施する。
4. Rmz / ECFNを同じVSyncOff設定でsample・高密度・実曲BGA付きで再測定する。
   起動とsteady stateを分け、フレームごとのログまたは集約器で正確な分位を求める。
5. FPS向上の最終確認はsurface待ちの上限に当たらない環境で行う。
   このMacではCPU probeを継続利用し、必要に応じGPU timestamp付きoffscreen測定を追加する。
   WindowsではDX12/VulkanのVSyncOffでも確認し、GPU完了・present・描画開始頻度を分けて記録する。

## 再実行と成果物

```bash
cargo build --release -p bmz-player
cargo build --release -p bmz-player --example play_plan_profile
target/release/examples/play_plan_profile data/skins/Rmz-skin/play7main.luaskin
target/release/examples/play_plan_profile data/skins/ADFX02/ECFN/play/play7.luaskin /path/to/profile.toml
```

このMacでは既存FFmpeg build metadataに削除済みHomebrew Cellarパスが残っており、
exampleの通常linkが失敗した。依存や本体の設定は変更せず、実在するlibパスを指定して検証した:

```bash
cargo rustc --release -p bmz-player --example play_plan_profile --offline -- -L native=/opt/homebrew/opt/ffmpeg/lib
```

実機ログ・sampleスタック・JSON集計・分離設定・再実行スクリプトは
`.local/performance/2026-09-21/`へ保存（gitignore対象）。外部スキン自体は含めない。
実機の再実行scriptは`/tmp/bmz-fps-20260921/<case>`を生成するため、新しいcase名を指定する。
benchmarkは読み取りのみで、指定profile・DBを書き換えない。

調査段階の検証: 本体release build、probeのrelease buildと両スキンでの実行、
`cargo clippy --release -p bmz-player --example play_plan_profile --offline`、
`cargo fmt --check`、`git diff --check`は成功。この時点では本体の挙動変更はなくunit testは追加していない。

# 段階的な実装と再計測

## 1. ノーツ座標のフレーム内共有

`PreparedNoteLayout` でレーン矩形・ノーツ高・notes offsetをフレームごとに準備する。
Tap／Mine／LN・CN・HCNの各ノーツから利用し、ノーツごとのoption解決・destination全走査を除去した。
`note.dst` のレーン選択も一時Vecを作らない形にした。画像やLua callbackの評価方法はこの段階では変更していない。

変更前後のreleaseバイナリを交互に3回実行したCPU probeの平均値の中央値（µs）:

| スキン | 可視Tap数 | 変更前 | 変更後 | 短縮 |
|---|---:|---:|---:|---:|
| Rmz | 8 | 108.647 | 101.132 | 6.9% |
| Rmz | 64 | 139.525 | 103.052 | 26.1% |
| Rmz | 256 | 244.021 | 111.429 | 54.3% |
| ECFN | 8 | 89.449 | 79.957 | 10.6% |
| ECFN | 64 | 133.211 | 84.472 | 36.6% |
| ECFN | 256 | 283.680 | 98.402 | 65.3% |

`BMZ_PLAN_DUMP=1` で各負荷の300／1500／3299フレーム目のDrawPlanを取得し、各スキン9件とも変更前と完全一致。
全8キーモード、条件付きレーン、LIFT、offsetの重複・幅・高さ・alpha、見逃し落下、欠損定義について従来計算と比較するテストを追加。
`cargo test -p bmz-render --offline` は613件成功。`cargo check --offline`、対象crateの`cargo clippy --all-targets --offline`、`cargo fmt --check`も成功。
このMacでは古いFFmpegのlink検索先がbuild cacheに残っていたため、link時に`LIBRARY_PATH=/opt/homebrew/opt/ffmpeg/lib`を指定した。

比較スクリプト・生データ・変更前後バイナリはgitignore管理の`.local/performance/2026-09-21/`に保存。

実画面でもVSyncOff／Metal Immediate／2944×1656／高密度譜面で再計測した。
Rmzのplanは初回調査0.5985ms→0.5242ms、ECFNは0.3042ms→0.2403ms。
この比較は各1試行で、CPU probeほど厳密に時系列を交互化していない。
両方とも720フレームの集計で120.0 FPS。surface待ちはRmz 6.7623ms／ECFN 7.2603msあり、実FPSの向上はこのMacでは確認できていない。

## 2. Lua併用スキンの静的キャッシュと補間メタデータ評価

PlayではLua runtimeがある場合も既存の構造・静的画像キャッシュを利用する。
`draw`、timer、offset、アニメーション等に依存するdestinationは従来の条件でキャッシュ対象外とし、callbackの値を保持しない。
補間の`acc`・固定色判定は必要なフィールドだけを継承して調べる。従来は補間方法を調べるだけでも各キーフレームで`SkinDrawState::default()`を構築していた。

第1段階との交互3回比較（平均の中央値、µs）:

| スキン | 可視Tap数 | 第1段階 | 第2段階 | 追加短縮 |
|---|---:|---:|---:|---:|
| Rmz | 8 | 101.264 | 91.064 | 10.1% |
| Rmz | 64 | 103.196 | 92.931 | 9.9% |
| Rmz | 256 | 110.543 | 101.462 | 8.2% |
| ECFN | 8 | 80.418 | 62.460 | 22.3% |
| ECFN | 64 | 84.854 | 66.832 | 21.2% |
| ECFN | 256 | 99.044 | 81.831 | 17.4% |

両スキン各9件のDrawPlanは最初の変更前と完全一致。statefulなLua draw/value callbackの呼び出し順・回数と時間／offset変更時の描画結果を、キャッシュなしの評価と比較した。
色・補間方法の継承は128パターンを従来のフレーム評価と比較。`bmz-render` 615テスト、対象crateのclippy、fmt、diff checkが成功。

VSyncOff／2944×1656の実画面（高密度譜面、各720フレーム）ではplan時間がRmz 0.4917ms、ECFN 0.2180ms。
FPSは両方120.0。ECFNは動画アップロード1枚あたり0.8250ms、video全体は描画フレームあたり0.4175msだった。
