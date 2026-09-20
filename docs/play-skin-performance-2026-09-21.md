# 他のプレイスキンの性能調査（2026-09-21）

## 条件

Rmz / ECFN向けの共通最適化3件を含む `8e1ae3ae` を基準とする。
以下は追加最適化前の調査結果。後続の実装・A/B計測は末尾に追記する。
外部スキンのファイルは変更しない。

- MacBook Pro / Apple M4 / RAM 16GB / macOS 26.6.2、release build。
- 7Kの既定スキンオプション。Rmz / ECFNも今回の表では既定値で再測定。
  前回のECFN実験で使用したユーザー選択オプションとは異なる。
- GPUなしの `play_plan_profile`：各条件300フレームのウォームアップ＋3000フレーム、
  3回の平均時間の中央値。実行順を反転・循環させ、直列実行した。
- 可視tap 8 / 64 / 256個、LN/HCN 2本、小節線2本の共通スナップショット。
  実スキンをdecodeし、Lua runtimeも有効な通常の描画計画作成経路を通す。
- このprobeはGPU描画・文字ラスタライズ・動画更新・音声を含まない。
  表の逆数から実アプリの到達FPSを計算してはいけない。

## 描画計画作成の比較

単位はµs。p95も各実行のp95の中央値。default / Rmz / ECFNは比較用。

| スキン（7K） | destination数 | tap 8 | tap 64 | tap 256 | tap 256 p95 |
|---|---:|---:|---:|---:|---:|
| default | 59 | 20.5 | 21.0 | 28.0 | 32 |
| Rmz | 176 | 88.9 | 89.9 | 99.8 | 112 |
| ECFN | 365 | 43.6 | 47.7 | 61.9 | 70 |
| ECFN wide | 362 | 44.0 | 47.7 | 62.2 | 70 |
| WMII FHD AC | 370 | 71.7 | 77.5 | 98.2 | 108 |
| WMII FHD wide | 370 | 71.7 | 77.8 | 97.9 | 107 |
| ADFX-Test | 332 | 50.0 | 54.9 | 70.3 | 78 |
| ADFX-Test wide | 314 | 41.8 | 45.3 | 59.8 | 67 |
| ADFX_02 | 352 | 44.9 | 47.9 | 61.9 | 69 |
| ADFX_02 wide | 352 | 43.7 | 48.4 | 61.5 | 69 |
| mz-select antique | 212 | 49.6 | 53.8 | 63.6 | 71 |
| REMI-S hachimi FHD（欠損あり） | 195 | 39.8 | 44.2 | 57.2 | 64 |
| GenericTheme | 203 | 40.4 | 42.5 | 49.0 | 55 |
| mz-select brand-new | 107 | 36.0 | 38.3 | 45.7 | 52 |
| Starseeker | 210 | 34.6 | 35.9 | 43.3 | 49 |
| REMI-S-arr（欠損あり） | 158 | 33.0 | 35.1 | 41.7 | 48 |

WMIIは既定設定のECFNより256 tap条件で約1.6倍重い。
同じ370 destinationsのAC / wideで近い結果が出ている。
8→256 tapの増加分もWMIIは約26µs、REMI-S hachimiは約17µsある。
これはノーツの追加生成・出力も含む増加であり、全量を検索処理の無駄とは見なさない。

今回の条件でLua runtime callbackを持つのはRmz（5個）とantique（3個）。
WMII / ADFX / GenericTheme / Starseekerにはruntime callbackが無く、
WMIIを軽くするためにLua VMを最適化しても、この条件には直接効かない。

## 実画面の確認

Metal / VSyncOff（requested・effectiveともにImmediate）/ Windowed /
2944×1656 / Native内部解像度 / FPS制限0で実行した。
専用の一時profile・DBを使い、ユーザーの設定ファイル・スコアDBは変更していない。
高密度の合成7K譜面（180 BPM、84 tap/秒、1344 tap、BGA・音声なし）をオートプレイ。
各スキン1回、プレイ開始後2～9秒に完全に収まる120フレーム集計6区間、720フレーム。
起動・READY・終了・フレーム制御切替をまたぐ区間は除外した。

平均時間の単位はms。実画面の順位は1回ずつの参考値であり、
微差を確定的なスキン間差や改善効果として扱わない。

| スキン | 描画計画 | 文字準備 | geometry | 動画更新 | 取得・present待ち除外 | surface取得 |
|---|---:|---:|---:|---:|---:|---:|
| default | 0.139 | 0.121 | 0.004 | 0 | 1.030 | 7.166 |
| WMII FHD AC | 0.421 | 0.058 | 0.009 | 0 | 1.405 | 6.799 |
| ADFX-Test | 0.291 | 0.053 | 0.007 | 0.120 | 1.268 | 6.939 |
| ADFX_02 | 0.272 | 0.053 | 0.006 | 0.111 | 1.237 | 6.971 |
| GenericTheme | 0.243 | 0.056 | 0.007 | 0 | 1.133 | 7.077 |
| Starseeker | 0.253 | 0.057 | 0.007 | 0.115 | 1.272 | 6.931 |
| mz-select antique | 0.307 | 0.053 | 0.010 | 0 | 1.236 | 6.967 |
| REMI-S hachimi FHD（欠損あり） | 0.227 | 0.057 | 0.007 | 0 | 0.971 | 7.253 |

全ケースで描画開始頻度は119.9～120.0 FPS。前回同様、surface取得待ちが大きいため、
この環境ではCPU側最適化による上限FPSの差を確認できていない。
「取得・present待ち除外」もCPU側の実時間で、内部のGPU queue待ち等を含む。
実GPU実行時間やCPU使用サイクルそのものではない。

Starseekerの既定背景 `TYPE-M.mp4` は30fpsで更新され、6秒に180回転送された。
動画転送は平均0.403ms/更新、描画1フレーム当たり0.101ms。
ECFNで行った共通転送最適化の対象経路だが、今回は旧実装とのA/B比較はしていない。
ADFX-Testも30fps・180回転送で、平均0.419ms/更新、0.105ms/描画フレームだった。
ADFX_02も180回転送で、平均0.390ms/更新、0.098ms/描画フレームだった。

## 改善候補と着手順

256 tapを固定した長時間probeから、macOS `sample` で3秒・1ms間隔のスタックを採取した。
以下はメインスレッドのサンプルに占めるinclusive比率の目安で、
関数同士は重複し得る。短い通常probeとは実行時間・状態の範囲が異なり、
削減可能時間や改善率そのものではない。

| 処理 | WMII AC（2279 samples） | antique（2292） | REMI hachimi（2294） |
|---|---:|---:|---:|
| static play items作成全体 | 53.3% | 48.5% | 44.3% |
| `note_part_render_item` | 23.9% | 16.3% | 23.8% |
| `is_lift_lane_cover_id` | 6.9% | 4.4% | 3.7% |
| `value_number_render_items` | 5.0% | 10.5% | 6.4% |

### 1. ID判定の文字列確保をなくす（小さく切り出せる共通改善）

`crates/bmz-render/src/skin/interaction.rs` の `is_lift_lane_cover_id` は
通常の画像IDにも `to_ascii_lowercase()` を実行する。
WMIIのスタックでは、この判定の配下にmalloc/freeが繰り返し現れた。
描画先の評価中に同じIDを何度も判定するため、LIFTがOFFでも発生する。

現行のASCII大小文字を無視する判定を割り当てなしで行うか、
document側の計画キャッシュで分類を保存する。
まず前者を独立した変更にすると、A/Bと描画一致確認がしやすい。
完全一致の `lift-cover` / `lift_cover` と部分一致の `liftcover` の意味を保持する。

### 2. ノーツ画像の参照先を先に解決する（高密度譜面向け）

`crates/bmz-render/src/skin/document_render/play/note.rs` の
`note_part_render_item` は、ノーツごとに `self.image.iter().find(...)` で
画像IDを線形検索し、source検索とUVの算出を行う。
WMIIは253画像、REMI hachimiは234画像を持つ。
前回キャッシュしたのはノーツの座標・レーン形状であり、この検索は残っている。

レーン・ノート種別ごとに画像indexやsource参照を準備し、
フレーム内で再利用する候補。画像検索以外の生成コストも含まれるため、
サンプル比率24%をそのまま短縮できるとは考えない。
LN/HCNの状態別画像、processed / Mineのfallback、LR2のHOLD timer、
sourceサイズ変更・スキンreload・重複IDの現行解決順を保つ必要がある。

### 3. destination評価の型・参照先と固定情報をキャッシュする

`skin/document_render/core/static_render.rs` は毎フレームimage/valueのHashMapを構築し、
`core/resolve.rs` は種類別の配列を繰り返し検索する。
`play/judge.rs` の数値描画も、解決済みvalueをIDから再検索し、桁配列を生成する。
WMIIのstatic items経路が約53%、antiqueでは数値描画が約11%あることから、
両スキンを中心に測る候補。

初期化時にdestinationの種別・index、固定offset列、条件展開済みkeyframeなどを保存する。
描画順、timer/op/draw、動的数値はその都度評価する。
広い変更になるため、1・2の効果を確認してから進める。

antiqueには3個のLua callback定義があるが、今回の長時間probeではLua実行のスタックを
確認できなかった。callbackの存在だけを理由にLua実行系を最優先にしない。
対象の数値を表示するスキンオプション・状態で別途計測する必要がある。

### 今回は優先度を下げるもの

- GPU geometry生成は実画面で0.004～0.010ms程度。
  インスタンシング方式の全面変更を先に行う根拠は薄い。
- GenericTheme / brand-new / Starseekerの計画作成はWMIIより軽い。
  個別の専用最適化より、上記共通処理の改善結果を確認する。
- ADFX-Test / ADFX_02 / Starseekerの既定動画は1280×720・30fps、
  ECFNは同解像度・60fps。動画転送の改善余地はあるが、
  解像度・フレームレートを落とす変更は比較条件から除外する。

## 対象範囲と読み込み上の制約

- Hubは元の配置では `require("const")` に失敗した。
  entryは `data/skins/Hub/Hub_play7.luaskin`、実ファイルは
  `data/skins/Hub/Hub/const.lua`、指定されたaliasは `skin/Hub/const.lua`。
  現在のskin library rootからは一段ずれている。失敗値を高速な結果として扱っていない。
  `/tmp` にファイル内容を変えず階層を合わせたコピーでも再確認したところ、
  次に `main.lua:207` で `luaskin.name` がnilのまま連結されるエラーになった。
  配置だけでは計測可能にならず、header decode等の互換性調査が先。
- REMI-S hachimiはフォント3本と `dummy.png`、
  REMI-S-arrはフォント3本と `bomb/Default.png` の読み込みに失敗した。
  CPUの計画作成は実行できたが、完全なアセット構成の描画負荷を表す値ではない。
- GenericThemeでは `property[].item[].isSelected` の未対応等のdecode警告がある。
  正しい描画の完全一致を今回の性能調査だけで保証するものではない。
- WMII wideには `destination[12].dst[1]` のmixed table変換警告が1件ある。
- Luxez-Flat / MILLIONDOLLARには、今回配置されているファイルにプレイ用entryが無い。
- 5K / 9K / 14K等、個別のオプション組み合わせ、実曲のBGA負荷は未調査。
  7Kの既定値で軽いスキンでも、全設定・全譜面で軽いとは断定しない。

## 再現と記録

全entryの一覧、実行コマンド、戻り値、3回分の生JSONLはgitignore対象の
`.local/performance/2026-09-21/other-skins/` に保持。
集計は `plan-summary.json`、直列実行スクリプトは `scan_plans.py`。
実画面8本のログ・条件・集計は `surface/`、スタックは `*-sample.txt`、
decode構成と警告は `*-metadata.jsonl` / `*-metadata.stderr` に保持。

```sh
LIBRARY_PATH=/opt/homebrew/opt/ffmpeg/lib cargo build --release -p bmz-player --example play_plan_profile --offline
target/release/examples/play_plan_profile data/skins/WMII_FHD/play/play7ac.luaskin
```

外部サンプリング用に、probeへ `BMZ_PLAN_PROFILE_FRAMES` と
`BMZ_PLAN_PROFILE_NOTES_PER_LANE` を追加した。未指定時の条件は従来と同じ。
長時間のsampling実行は上の比較統計に含めない。
構成情報の出力も増やし、decode警告をstderrへ出すようにした。

検証：release build、`cargo clippy --release -p bmz-player --example play_plan_profile --offline`、
`cargo fmt --check`、`git diff --check`が成功。
probeの未指定条件（3000フレーム×3負荷）、1フレーム・単一負荷の指定、
フレーム数0・ノーツ数0・非数値入力のエラー終了を確認した。

## 実装1：LIFTカバーID判定の文字列確保を削減

`to_ascii_lowercase()` による一時Stringを、ASCIIの大小文字を無視するbyte windowの
部分一致判定に置き換えた。既存の別名・部分一致・非ASCIIの扱いは保持した。

変更前後を交互に各3回実行した中央値（µs）。既定オプション、同じprobe・同じ負荷。

| スキン | 8 tap：前→後 | 256 tap：前→後 | 256 tap短縮 |
|---|---:|---:|---:|
| WMII AC | 73.814 → 66.963 | 102.006 → 95.131 | 6.7% |
| antique | 50.880 → 48.628 | 66.347 → 63.426 | 4.4% |
| REMI hachimi | 41.152 → 38.790 | 59.306 → 56.872 | 4.1% |
| Rmz | 90.205 → 88.911 | 100.618 → 99.184 | 1.4% |
| ECFN | 45.706 → 42.719 | 64.711 → 62.381 | 3.6% |
| GenericTheme | 41.575 → 36.703 | 49.621 → 45.329 | 8.6% |
| default | 19.553 → 19.207 | 28.382 → 27.782 | 2.1% |

全7スキン×3負荷×3時点＝63組のDrawPlanが変更前と完全一致。
`cargo test -p bmz-render --offline`：616 passed / 1 ignored（既存の明示実行GPUテスト）。
ID別名・ASCII大小文字・Unicodeを含むID・ハイフン付きIDの部分一致禁止を検証した。
生データは `.local/performance/2026-09-21/other-optimizations/ids/`。

VSyncOff実画面（各720フレーム）は順序を変えて2組測定した。
WMIIのplanは前→後が0.4060→0.4685ms、逆順実行では前0.3577 / 後0.3777ms。
取得・present待ち除外も1.3741→1.4895ms、逆順で1.2200→1.3553msとなり、
実画面のCPU時間短縮は確認できなかった。文字準備等の未変更処理も変動しているが、
変動の原因は確定していない。いずれも120.0 FPS。
採用根拠は連続実行のCPU計画作成での改善と一時割り当て削減であり、
実画面のFPS・フレーム時間改善とは主張しない。
`cargo clippy -p bmz-render --all-targets --offline`も成功。

## 実装2：通常／処理済みノーツのspriteをフレーム内で再利用

レーンごとの通常tapとprocessed画像を必要になった時点で一度だけ解決する。
texture / UV / sourceサイズをフレーム内で再利用し、座標とalpha等はノーツごとに適用する。
LN/HCN/Mineの画像選択・timer処理は既存の経路を保つ。
フレームをまたぐキャッシュではなく、sourceやskin変更時の古い参照を保持しない。

実装1との交互A/B各3回の中央値（µs）。

| スキン | 8 tap：前→後 | 256 tap：前→後 | 256 tap短縮 |
|---|---:|---:|---:|
| WMII AC | 67.159 → 67.182 | 94.292 → 73.130 | 22.4% |
| antique | 48.051 → 48.511 | 63.328 → 54.821 | 13.4% |
| REMI hachimi | 38.807 → 39.375 | 56.626 → 45.480 | 19.7% |
| Rmz | 89.158 → 88.925 | 99.861 → 94.869 | 5.0% |
| ECFN | 42.222 → 42.490 | 61.701 → 48.130 | 22.0% |
| GenericTheme | 36.808 → 37.152 | 45.060 → 42.933 | 4.7% |
| default | 19.345 → 19.683 | 28.070 → 25.687 | 8.5% |

少数ノーツではキャッシュ管理の分だけ最大0.568µs増加した。
密度が高い条件の改善を目的として採用する。各スキン9組、合計63組のDrawPlanが一致。
追加テストは全8キー数でtap/processed・異なる矩形・欠損source/画像・重複IDの先勝ち・
sourceサイズの変更を確認した。

renderの全体実行は616 passed / 1 failed / 1 ignored。
失敗した既存CIM切り詰めテストでは、別のCIM正常系テストと同じ画素を読み込んでいた。
テスト一時パスが時刻だけで生成されており並列競合が疑われる。
当該テストの単独再実行と、assets全11テストの逐次実行はいずれも成功。
本体の画像decode処理は変更していない。clippyも成功。
生データは `.local/performance/2026-09-21/other-optimizations/taps/`。

VSyncOff実画面のWMIIはplan 0.4528→0.4043ms、取得・present待ち除外は
1.4797→1.3369ms。各720フレームの1組であり、実画面の確定的改善率とは扱わない。
FPSは両方120.0。区間p99の最大値は9.130→9.020ms。

## 実装3：画像・数値のID検索表を構造キャッシュへ保持

毎フレーム作り直していたimage/valueのHashMapを、documentのID→index表として
既存のplanning cacheへ保存する。描画評価は現在のdocumentとsourceを参照し、
値・timer・op・draw・Lua callbackは引き続き毎フレーム評価する。
キャッシュを使わない経路は従来同様に参照表を作る。

画像・数値の重複IDは従来の検索表と同じ後勝ちにし、数値spriteの先勝ちも維持する。
重複IDを含むcached/uncached描画一致、値の変更、sourceサイズ・texture変更を追加検証。
レンダラー全618テストは逐次実行で成功（既存GPUテスト1件はignored）。
実装2で失敗したCIMテストもこの全体実行で成功した。

今回は検索表の生成と再利用を対象とし、destinationの種類別dispatchやkeyframeの
全面的な事前評価、数値spriteの検索経路の変更は含めていない。

実装2との交互A/B各3回の中央値（µs）。

| スキン | 8 tap：前→後 | 256 tap：前→後 | 256 tap短縮 |
|---|---:|---:|---:|
| WMII AC | 65.689 → 61.025 | 71.641 → 67.612 | 5.6% |
| antique | 47.834 → 45.222 | 54.327 → 51.609 | 5.0% |
| REMI hachimi | 39.176 → 35.361 | 45.104 → 41.349 | 8.3% |
| Rmz | 89.019 → 85.906 | 94.937 → 91.127 | 4.0% |
| ECFN | 42.407 → 39.144 | 48.311 → 44.632 | 7.6% |
| GenericTheme | 37.264 → 34.635 | 43.051 → 40.479 | 6.0% |
| default | 19.503 → 19.110 | 25.174 → 24.669 | 2.0% |

各スキン9組、合計63組のDrawPlanが実装2と完全一致。
生データは `.local/performance/2026-09-21/other-optimizations/indices/`。

VSyncOff実画面のWMIIは、各720フレームでplan 0.3502→0.3518ms、
取得・present待ち除外1.2690→1.2607ms、区間p99最大9.093→9.126ms。
両方120.0 FPSで、実画面では明確な改善を確認できなかった。
この変更も、連続実行の計画作成時間短縮と毎フレームの検索表生成削減を根拠に採用する。

## 3変更を合わせた計測

作業開始時の `8e1ae3ae` と実装3まで適用したバイナリを改めて交互に各3回測定した。
別々に測った短縮率を加算せず、同じ条件で直接比較した中央値（µs）。

| スキン | 8 tap：前→後 | 256 tap：前→後 | 256 tap短縮 |
|---|---:|---:|---:|
| WMII AC | 71.795 → 62.579 | 98.752 → 68.288 | 30.8% |
| antique | 50.130 → 45.519 | 65.024 → 52.063 | 19.9% |
| Rmz | 91.580 → 86.593 | 100.792 → 92.172 | 8.6% |
| ECFN | 45.550 → 39.639 | 64.351 → 44.752 | 30.5% |

この4スキンについても36組のDrawPlanが作業開始時と完全一致した。
生データは `.local/performance/2026-09-21/other-optimizations/total/`。
数値はCPUでの描画計画作成の短縮で、実アプリのFPS改善率ではない。

実画面は同じMetal / VSyncOff / 2944×1656 / Native条件で、作業開始時と直接比較した。
Rmzは既定オプション、ECFNはユーザーの既存選択オプションを維持しているため、
上の既定オプションでのprobeとは構成が異なる。各720フレーム、単位ms。

| スキン | 描画計画：前→後 | 取得・present待ち除外：前→後 | 区間p99最大：前→後 | FPS：前→後 |
|---|---:|---:|---:|---:|
| Rmz | 0.4447 → 0.4415 | 1.2320 → 1.2920 | 9.064 → 8.921 | 120.0 → 120.0 |
| ECFN | 0.1877 → 0.1988 | 0.7027 → 0.8177 | 8.618 → 9.000 | 120.0 → 120.0 |

この1組では実画面のCPU時間短縮を確認できず、取得・present待ち除外の時間は増加した。
実行順を変更後→変更前にした追試は以下。表の数値は比較しやすいよう前→後で表記。

| スキン | 描画計画：前→後 | 取得・present待ち除外：前→後 | 区間p99最大：前→後 | FPS：前→後 |
|---|---:|---:|---:|---:|
| Rmz | 0.4948 → 0.4830 | 1.4023 → 1.3834 | 9.001 → 8.949 | 120.0 → 120.0 |
| ECFN | 0.2850 → 0.1803 | 1.1185 → 0.7357 | 9.282 → 8.851 | 120.0 → 120.0 |

CPU実時間の増加は逆順の組では再現しなかった。未変更のencode/queue等も変動しており、
同じECFN変更前バイナリでも待ち除外時間は0.7027～1.1185msと幅がある。
変動原因は特定していないため、実画面での改善・悪化の確定的な割合は出さない。
全8実行でrequested/effectiveともにImmediateを確認し、FPS増加は確認できなかった。
採用理由は連続CPU計測での一貫した描画計画作成の短縮であり、
このMac上の実画面FPS・CPUフレーム実時間の改善とは区別する。

実画面の生ログ・実行条件・集計は
`.local/performance/2026-09-21/other-optimizations/surface/` に保存。

## 全体検証の結果

- `cargo fmt --check`、`cargo check --offline`、`cargo clippy --offline` は成功。
- `bmz-render`：618 passed / 1 ignored。追加した重複ID・動的値のテストも再実行で成功。
- `bmz-player`：1959 passed / 1 failed / 3 ignored。
  最初のsandbox内実行ではローカルsocketの待受制限による14件の失敗もあったが、
  権限付き再実行で全14件が成功した。
- `bmz-skin`：211 passed / 3 failed。
- `bmz-skin-document`：10 passed、`bmz-skin-convert`：2 passed、`bmz-font`：9 passed。

残る4件は、現在配置されているスキンと読み込みテストの期待値が一致しないもの。

1. playerの `select_lua_skins_decode_with_explicit_library_root_when_available`：
   同梱選曲スキンに `bmz_select_mode` が無い。mz-select / Luxez-Flatのsubmoduleはcleanで、
   作業開始時と同じcommitを参照している。描画前のdecode結果に対するassertionで失敗。
2. skinの `wmii_fhd_play_lua_features_when_available` と
   `wmii_fhd_play_stage_draws_follow_scene_modes_when_available`：
   WMIIのextrastage / practiceのdraw条件が期待値と異なる。
3. skinの `wmii_beatoraja_branch_next_rank_updates_when_available`：
   WMIIの期待する次ランク関連valueが見つからない。

skin crateはrenderへ依存しておらず、今回の変更はこれらのdecode処理・テスト・
スキンファイルを変更していない。変更前checkoutでの同一テスト再実行はしていないため、
変更前の実測結果とは扱わない。全テスト成功とはせず、この不一致を残した状態で記録する。
