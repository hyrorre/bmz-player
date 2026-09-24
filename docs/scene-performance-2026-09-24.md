# 選曲・リザルト・コースリザルトの性能調査（2026-09-24）

## 結論と着手順

3画面ともCPU側に改善余地がある。小さく検証できる順に、選曲バーの不要な生成・検索、
Starseekerの判定円グラフ、Luaがある場合の構造・グラフキャッシュ、Lua呼び出し準備を候補にする。
今回は調査と再計測用exampleの追加までで、実行時の最適化や改善率のA/B検証は行っていない。

現在使用中のスキンは選曲がECFN、単曲リザルトがStarseeker、コースリザルトがECFN。
実画面はVSyncOffでも約120 FPSで、surface取得に6.35～7.45ms/フレームかかっている。
CPU負荷の削減余地は確認できたが、この環境での上限FPS向上はまだ確認できていない。

## 条件と計測範囲

- 基準コミットは `8788d87d`。Apple M4 / macOS 26.6.2、release build。
- CPU probeは実スキンをdecodeし、Lua runtimeを含む `Renderer::prepare_scene` を実行する。
  各条件300フレームのウォームアップ＋2000フレームを3回、2回目はスキンの実行順を反転。
  他の計測・ビルド・テストと同時実行しない。
- 表の平均・p95は、それぞれ各実行の値の中央値。時間の計測にはナノ秒単位の `Instant` を使う。
  スナップショットの入力用cloneは計測外、前フレームのplan/snapshot破棄は計測内。
- 選曲fixtureは25行、選択位置12、3000ノーツ・EX 5400の架空譜面、日本語タイトル。
  `chart_count=1000` は表示値であり、1000件のDB検索を実行する条件ではない。
- 単曲fixtureは120秒・3000 timing points・1440 gauge points（6種、500ms間隔）・
  120個ずつの密度/判定/FAST-SLOW bucket。コースはこれを4曲または10曲分生成する。
  コースの曲名・曲別成績・WMII用仮想 `courseData.json` も設定する。
- GPU、文字ラスタライズ、動画更新、音声、DB、アプリ側snapshot生成、初回ロードはCPU probeの対象外。
  probe時間の逆数を実アプリの到達FPSとして扱わない。
- 既定オプションと現在のprofileの比較は分ける。CPU probeのprofile指定は該当画面の
  options/filesのみを反映し、offsetsやその他のアプリ設定は反映しない。

## CPU描画計画作成

単位はµs。既定オプション・静止状態。コースは4曲。
callback数は登録数であり、各フレームの実行回数ではない。
commandsは描画計画の平均コマンド数を整数に丸めた値で、GPU draw call数ではない。

| 画面 | スキン | 平均 | p95 | commands | Lua callbacks |
|---|---|---:|---:|---:|---:|
| 選曲 | default | 80.4 | 87.7 | 103 | 0 |
| 選曲 | ECFN | 318.4 | 339.0 | 264 | 3 |
| 選曲 | mz-select | 418.0 | 492.0 | 263 | 92 |
| 選曲 | Luxez-Flat | 629.8 | 799.1 | 244 | 412 |
| 選曲 | Starseeker | 317.8 | 340.8 | 268 | 3 |
| 選曲 | ADFX_02 | 322.3 | 350.6 | 271 | 3 |
| 単曲リザルト | default | 11.7 | 12.0 | 39 | 0 |
| 単曲リザルト | Starseeker | 284.6 | 306.1 | 966 | 0 |
| 単曲リザルト | ECFN | 38.9 | 44.0 | 196 | 1 |
| 単曲リザルト | WMII FHD | 96.5 | 105.5 | 228 | 19 |
| 単曲リザルト | mz-select | 24.3 | 26.5 | 101 | 0 |
| 単曲リザルト | Luxez-Flat | 199.0 | 248.0 | 211 | 217 |
| 単曲リザルト | MILLIONDOLLAR | 386.3 | 434.9 | 960 | 69 |
| コースリザルト | default（単曲用のfallback） | 11.7 | 11.9 | 39 | 0 |
| コースリザルト | ECFN | 44.3 | 48.9 | 219 | 1 |
| コースリザルト | WMII FHD | 89.8 | 102.1 | 234 | 61 |
| コースリザルト | mz-select | 19.2 | 20.8 | 90 | 0 |
| コースリザルト | Luxez-Flat | 190.5 | 237.5 | 214 | 208 |

現在のprofileのoptions/filesを適用した結果:

| 画面・スキン | 平均（µs） | p95（µs） |
|---|---:|---:|
| 選曲 ECFN | 319.2 | 345.1 |
| 単曲リザルト Starseeker | 323.3 | 345.8 |
| コースリザルト ECFN・4曲 | 107.2 | 115.8 |
| コースリザルト ECFN・10曲 | 182.8 | 200.5 |

ECFNコースは設定による差が大きい。現在設定では4→10曲で約76µs増えるが、
commandsは329→330とほぼ同数。出力数だけでは説明できないグラフデータ処理が候補になる。
既定設定での4→10曲はECFN 44.3→54.3µs、WMII 89.8→112.7µs、
mz-select 19.2→24.7µs、Luxez-Flat 190.5→213.1µsだった。
どの処理が増分の何割を占めるかは未分離で、全量をキャッシュで削れるとは判断しない。

選曲の疑似スクロールではdefault 80.7µs、ECFN 320.7µs、mz-select 417.3µs、
Luxez-Flat 610.9µs。これは同じ25行の位置・選択timerを変える条件で、
実際の選曲移動、DB検索、banner/stagefile切替の測定ではない。
オプションパネル表示時のLuxez-Flat選曲は685.8µs（p95 865.5µs）で、
静止状態より評価量が増える。単曲リザルトのパネル表示はWMII 110.7µs、
Starseeker 286.8µs、Luxez-Flat 199.7µsだった。

## 実ウィンドウの測定

Metal / VSyncOff（effective present mode: Immediate）/ 2944×1656 / Native内部解像度 /
FPS制限0・バックグラウンド制限0。専用profile・DBと32個の合成BMSを作り、音量0で実行した。
ECFN選曲・Starseeker単曲・ECFNコースのみ現在のスキン設定を使用し、他は既定設定。

- 選曲はルート画面の静止状態。32譜面のフォルダ内での連続スクロールは未計測。
- 単曲は既存の `--boot-result-sample`。上記CPU probeとは別のfixture。
- コースは112ノーツの無音譜面1曲をオートプレイした実際のCourseResult。
  最終曲のMusicResultが先に表示されるため、Courseスキンの読込後3秒以降に入る区間のみ採用した。
  単曲リザルトを測ってしまった初回実行は除外し、コース専用スキンで取り直している。
- 各条件1回、終了直前の集計を除いた安定区間の120フレーム×6区間、計720フレーム。
  FPSはログ時刻から求めた描画開始頻度。ディスプレイのscanout頻度ではない。

平均時間はms。スキン間の微差を確定的な性能差や修正効果として扱わない。
「取得・present除外」はCPU側の実時間であり、内部のqueue待ち等も含む。GPU実行時間ではない。

| 画面・スキン | 描画計画 | 取得・present除外 | surface取得 | FPS |
|---|---:|---:|---:|---:|
| 選曲 default | 0.318 | 1.207 | 6.981 | 119.5 |
| 選曲 ECFN（現在設定） | 0.164 | 0.813 | 7.453 | 120.0 |
| 選曲 mz-select | 0.739 | 1.846 | 6.353 | 120.0 |
| 選曲 Luxez-Flat | 0.734 | 1.185 | 7.063 | 119.8 |
| 単曲 default | 0.099 | 0.848 | 7.360 | 120.0 |
| 単曲 Starseeker（現在設定） | 0.597 | 1.478 | 6.741 | 120.0 |
| 単曲 WMII FHD | 0.784 | 1.684 | 6.526 | 120.0 |
| 単曲 Luxez-Flat | 0.884 | 1.776 | 6.425 | 120.0 |
| コース ECFN（現在設定） | 0.320 | 1.061 | 7.148 | 120.0 |
| コース mz-select | 0.141 | 0.869 | 7.335 | 120.0 |
| コース WMII FHD | 0.357 | 1.115 | 7.093 | 120.0 |

snapshot生成は0.014～0.048ms、文字準備は0.034～0.145ms。
ECFN選曲の動画更新は0.312ms/描画フレームで、描画計画より大きい。
今回の条件ではsurface取得待ちが支配的なので、まずCPU probeで効果を比較し、
実画面では取得・present除外時間とフレーム時間分布、表示の一致を併せて確認する。

## スタック採取と修正候補

CPU probeの長時間実行中にmacOS `sample` を3秒・1ms間隔で採取した。
各ケースのメインスレッドは約2500 samples。
比率は子関数を含むinclusive値で、項目間に重複がある。削減可能時間や改善率そのものではない。

### 1. 選曲バーの不要な生成と繰り返し検索

ECFN選曲では `select_songlist_items` が41.2%、その内側の `select_bar_item` が10.5%、
`SkinDrawState::default` が8.2%を占めた。

- `crates/bmz-render/src/skin/document_render/select/bar.rs` の `select_bar_item` は、
  バーごとにtimerの経過時間を得るためだけに `SkinDrawState::default()` を生成する。
  default stateに対する既存のtimer評価結果を保持したまま、生成を外へ出す／省く候補。
  現在フレームのstateへ単純に置換するとアニメーションの意味が変わるので分けて扱う。
- 同関数はimageset/imageを行ごとに線形検索する。選択画像のindex・参照先を事前解決できる。
- `crates/bmz-render/src/skin/animation.rs` の `destination_entry_at` は、
  1件を取得するために条件展開後の全件Vecを生成して `nth` する。
  songlist内の子要素・ランプ・レベルの処理でも繰り返すため、割り当てなしの探索、
  またはdocument/optionsごとの展開結果再利用が候補。

まずdefault state生成と `destination_entry_at` を別々に変更し、
ECFN / Starseeker / ADFX_02 / mz-select / Luxez-Flatで計測する。
条件付きdestinationの順序・index、重複IDの解決順、バー種別のfallbackを維持する。

### 2. Starseekerリザルトの判定円グラフ

既定状態で983 destinations、966 commands。
`result_judge_pie_destination_item` が32.1%、destination解決が28.9%、
render item追加が9.0%を占めた。Lua runtime callbackは無い。

`crates/bmz-render/src/skin/document_render/core/clip.rs` の
`result_judge_pie_destination_item` は、各 `judge_graph` destinationについて
frame・image・source・色・geometryを計算する。
`core/static_render.rs` の `static_image_destination_cacheable` は
`judge_graph` を明示的に除外している。

判定比率・画像参照などの共通化を先に行い、その後に固定状態のsegment geometryを再利用する候補。
入場アニメーション、timer、回転・色・alpha、offset、hover、画像フレーム、描画順を保持し、
新しいリザルトや判定数の変更時に無効化する。IR情報などが更新されるため画面全体は固定しない。

### 3. Luaが存在しても構造・グラフキャッシュを利用する

`crates/bmz-render/src/skin/runtime/context/scene.rs` の選曲・リザルト経路は、
`lua_draw_runtime.is_none()` の場合にだけ描画キャッシュを使用する。
callbackが1個でも登録されると、そのフレームで実行されなくてもキャッシュ経路を外れる。
ECFNの選曲では3個登録されているが、今回のsampleにcallback実行のスタックは無かった。
ECFNについては、Lua実行時間よりキャッシュを外れる影響を優先して切り分ける。

`ResultRenderCache::prepare_gauge_graph` は `Arc` の同一性を使ってグラフ準備を再利用するが、
LuaがあるECFNコースはこの経路も使用しない。4→10曲で増える処理をA/Bで分離し、
画像/valueの参照indexやグラフの固定情報を再利用する候補。

現在の回避理由はLua呼び出し中にキャッシュlockを保持しないためとコメントされている。
lockの保持範囲・再入を確認し、動的値やcallback結果を固定しない設計が必要。
Playには既にLuaとキャッシュを併用する経路があるが、同じlock設計を無条件に流用しない。
callbackの順序・回数、closure state、失敗時fallback、フレーム単位の命令数制限を維持する。

### 4. Lua callbackごとのdispatcher生成を減らす

`evaluate_callback_inner` の比率はLuxez-Flat選曲67.4%、同単曲リザルト56.5%、
WMIIコース36.4%。そのうちLua関数生成のスタックはそれぞれ19.6%、19.4%、12.6%。
関数呼び出し本体の `create_callback::do_call` は「関数生成」の集計から除いた。

`crates/bmz-skin/src/lua/runtime.rs` は `1e9837e8` でmain_state accessorを再利用済みだが、
`evaluate_callback_inner` 内の `scope.create_function` による借用state用dispatcherは
callbackごとに生成・登録・解除される。
安全な評価scope内での共有や連続評価のまとめ方を検討する。
行ごとのstate、借用寿命、エラーやunwind時の解除、callbackの実行順と命令数制限が制約になる。
変更範囲が広いため、先に1～3の効果を確認する。

## 既に対処済みの箇所と未計測の範囲

- コース成績・グラフの集約は `app/scene_state.rs::install_finished_course` から一度だけ行われ、
  snapshotは集約済みgraphの `Arc` をcloneする。毎フレーム全曲を再集計する実装ではない。
- 動画は `97d595eb` で描画計画が使用するものだけを更新する実装になっている。
  ECFN選曲の可視動画の転送には負荷が残るが、「非表示動画を止める」は今回の新規候補にならない。
- 今回の定常状態ではsnapshotやgeometryを先に全面改修する根拠は弱い。
- 大規模な実ライブラリの検索・フォルダ移動・連続スクロール、画像切替時の引っ掛かり、
  入退場、長い実コースのGPU描画、Windows/Linuxは未計測。
- 全18条件でdecodeは成功し、CPU通常計測のログには画像・フォント欠損やcallback実行失敗は無い。
  ただしmz-select選曲はcustom timer 10000/10001/10003/10013未対応とgenerated timer 9000の推論失敗、
  Luxez-Flat選曲はcustom timer 10007未対応、WMII単曲は17件のdestination op修復の警告が出る。
  未対応機能を含むため、beatorajaの全動作を再現した状態の性能比較ではない。

## 再計測と検証

リポジトリのルートから実行する。FFmpegのパスはこのMacの例。

```bash
LIBRARY_PATH=/opt/homebrew/opt/ffmpeg/lib cargo build --release --offline -p bmz-player --bin bmz-player --example scene_plan_profile
target/release/examples/scene_plan_profile select data/skins/ADFX02/ECFN/select/select.luaskin idle
target/release/examples/scene_plan_profile result data/skins/ADFX02/Starseeker/result/result.luaskin panel
BMZ_SCENE_PROFILE_STAGES=10 target/release/examples/scene_plan_profile course data/skins/ADFX02/ECFN/RESULT/course_result.luaskin idle data/profiles/hyrorre/profile.toml
```

`BMZ_SCENE_PROFILE_FRAMES` でフレーム数を指定できる。出力はmetadataと時間集計の2行のJSON。
初回decode・素材読込は計測外。外部スキンはローカルに必要で、計測のための改変は不要。

ローカルの生ログ・集計・実行コマンド・一時profile/DBは
`.local/performance/2026-09-24/scenes/`（gitignore対象）に保存した。
`scan.py` が3回のCPU計測、`sample_cpu.py` がスタック採取、`surface.py` が独立した実画面環境の準備・起動、
`summarize.py` が720フレーム集計を行う。一時データには元profileから複製した設定を含むためコミットしない。
コース実画面は `surface.py course-ecfn course-mz course-wmii --frames 4800` を使用した。
`--smoke-exit-after-result-frames` だけで早期終了すると最終曲のMusicResultしか測れない点に注意する。

検証結果:

- releaseアプリ・exampleのbuild、`cargo fmt --check`、`cargo check --offline`、
  `cargo clippy --offline`、exampleを含むbmz-playerのclippyは成功。
- `cargo test --offline -- --test-threads=1` は1975成功・3 ignored。
- bmz-renderは619成功・1 ignored、bmz-fontは9成功、bmz-skin-documentは10成功、
  bmz-skin-convertは2成功。
- bmz-skinは214成功・3失敗。失敗は既存の
  `wmii_fhd_play_lua_features_when_available`、`wmii_fhd_play_stage_draws_follow_scene_modes_when_available`、
  `wmii_beatoraja_branch_next_rank_updates_when_available`。
  今回はexampleと本文書のみの追加で、これらのテストや実装は変更していない。
- exampleの1フレーム実行、および0フレーム・11曲・不正scene・resultへのscroll指定の拒否を確認。
- 採用した実画面11条件は全て正常終了し、effective present modeがImmediateであることを確認。

## 後続の実装とA/B計測

以下は調査後の変更。バイナリを変更段階ごとに保存し、同じ条件で交互に起動した。
CPU計測は300フレームのウォームアップ＋3000フレーム、原則5回ずつの中央値。
生ログは `.local/performance/2026-09-24/scene-optimization/` に保存する。
描画一致の確認は性能計測と別実行とし、描画順・色・UV・座標・文字を含む命令をhash化する。
RectBatchのキャッシュIDとbatch境界だけを除外し、内部の矩形は順序通りに比較する。

### 1. 選曲バーと条件付きリスト

バー画像の既存のtimer評価はdefault stateから0を得るだけだったため、大きなstate生成を除いた。
`destination_entry_at` は全要素をVecに展開せず、条件を評価しながら対象indexまで進む。
画像の線形検索はこの変更には含めていない。

| スキン・静止状態 | 変更前（µs） | 変更後（µs） | 短縮 |
|---|---:|---:|---:|
| default | 80.5 | 73.6 | 8.6% |
| ECFN | 320.9 | 307.2 | 4.3% |
| mz-select | 420.3 | 405.8 | 3.5% |
| Starseeker | 317.6 | 304.9 | 4.0% |
| ADFX_02 | 322.5 | 308.5 | 4.4% |

default / ECFN / mz-selectでは静止時の全348フレームの描画hashが一致。
条件付きリストの順序・空グループ・範囲外と、default timerの値を含むsonglist関連10テストが成功した。
Luxez-Flatは起動ごとにcallback登録数412/440、出力244/250 commandsが変わるため、
この段階の性能差は採否の根拠にしない。
Starseeker選曲も同じ変更前バイナリの再実行同士でhashが異なったため、完全一致を確認できたとは扱わない。
これらの揺れは今回の変更前から存在する。
