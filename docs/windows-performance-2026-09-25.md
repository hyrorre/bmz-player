# WindowsでのFPS最適化レビューと追加調査（2026-09-25）

v0.4.1 (`24992795`) から `0608b186` までのFPS最適化をWindowsで確認した。
Rmz / ECFNのPlay、ECFNのSelect、StarseekerのResultで平均FPSが上がった。
一方、`97d595eb` の動画の可視性によるdecoder破棄には再生位置をリセットする回帰があり、
`add4c5db` で修正した。

## 計測条件

- Windows、Core i9-12900K、RTX 5090、driver 32.0.16.1088、Rust 1.96.1、release build。
- DX12、1920×1080 Windowed、Native、VSyncOff、前景・背景FPS上限0、frame latency Auto=1。
  ログで `requested=Immediate effective=Immediate` を確認した。
- 各実行に設定・DB・cacheを分離。ネットワーク更新、OBS、Discordを無効化。
  WASAPI Shared、master volume 0。同じ現在のローカルスキンと既定オプションを両版で使用。
- Playは180 BPM、7鍵×16 notes/小節、84 notes/sの合成譜面。音源・譜面BGAなし、
  autoplay、seed 42。ECFNのスキン動画は有効。
- 各条件2回、旧→新、新→旧の順に単独で実行。CPU probe、build、testとの同時実行なし。
- FPSは120フレーム単位のログ時刻から求めたアプリの描画頻度であり、物理的な表示回数ではない。
  Playはgameplay thread開始、それ以外は最初のprofileログを基準に、3～11秒に完全に収まる
  集計区間を採用。実行終了が早い場合は11秒より前に計測が終わる。
- 生ログ、fixture、集計スクリプトはローカルの `.local/performance/2026-09-25-windows/` に保存。
  外部スキンやユーザーのDBはコミットしない。通常利用の設定もユーザー指定でVSyncOffにした。

## v0.4.1からの実画面比較

2回の平均。描画計画時間はCPU時間。

| 条件 | v0.4.1 FPS | 0608b186 FPS | 変化 | plan ms: 旧→新 |
|---|---:|---:|---:|---:|
| Play / Rmz | 1,336 | 1,568 | +17.4% | 0.271 → 0.142 |
| Play / ECFN | 1,500 | 1,689 | +12.6% | 0.180 → 0.094 |
| Select / ECFN | 1,177 | 1,387 | +17.9% | 0.287 → 0.132 |
| Result / Starseeker | 1,112 | 1,458 | +31.1% | 0.422 → 0.202 |
| Result / Luxez-Flat（参考） | 1,226 | 1,190 | -3.0% | 0.315 → 0.339 |

Luxez-Flatは動的callback修正により描画内容も変わっており、同一負荷の性能回帰とは扱わない。
直近のSelect/Result最適化だけを分離した `8788d87d` → `0608b186` のCPU probeでは、
ECFN Select -23.4%、Starseeker Result -19.8%、Luxez-Flat Result -10.5%、
ECFN CourseResult（4曲）-15.0%だった。300 warmup＋3000 frames、3回の平均値の中央値。
別の348フレームでStarseeker / Luxez-Flat / CourseResultの描画primitive hashの一致も確認した。

平均FPSが上がっても裾の遅延が改善するとは限らない。ECFN Playの120フレーム区間p99の最大値は
旧1.672/1.647ms、新2.127/1.759ms。これは全フレームを混ぜたp99ではない。

## 一時非表示による動画の巻き戻し修正

`skin_flow/video.rs` は評価済み描画計画からalpha=0、面積0、命令なしを判定し、
非表示になったdecoderを破棄していた。再表示時に `loop_start_us` を現在時刻へ変更するため、
動画が先頭に戻り、描画スレッドでdecoderの停止待ちと再起動も繰り返していた。
beatorajaの `SkinSourceMovie#getImage` は初回だけ再生を開始する。

修正後は未表示の動画だけ遅延起動し、一度表示した動画のdecoderと開始時刻を保持する。
非表示中もplayback clockを更新し、GPU転送を止める。再表示時はその時点のフレームを転送する。
静的オプションの無効化、スキンの置換など既存の解放経路は維持する。
一度表示した動画の非表示中のCPU decodeとメモリは残る。長時間の非表示で解放するには、
ループ位相を保ったseek/resumeを別途設計する必要がある。

600ms周期で約300msずつ表示・透明にするJSONスキンで16,000フレームを実行した。
decoder起動回数はv0.4.1が1回、0608b186が14回、修正後が1回。
外部動画ファイル自体は変更していない。自己完結のY4M動画を使う回帰テストでは、
未表示時の起動抑制、非表示中の転送0回、再表示時の開始時刻維持・PTS進行も検証した。

通常のECFN Playは22,000フレームで再比較した。
修正前のFPSは1,502/1,671、修正後は1,602/1,704。
plan時間は0.097/0.092ms → 0.094/0.092ms。
区間p99の最大値は2.799/2.186ms → 4.065/1.859ms。
平均での悪化は見られないが、実行間の変動があり、修正によるFPS・p99改善は主張しない。
採用ログは `surface/validfix{0,1}-{head,video-fix}-ecfn-Dx12/`。
初期の `fix0/fix1` ECFN実行は一部重複したため全て性能比較から除外した。

## 追加最適化調査

`0608b186` の独立コピーにのみ区間計測を追加し、既存の `play_plan_profile` /
`scene_plan_profile` で調査した。300 warmup＋3000 frames、各条件3回、通常版と交互に実行。
表は各実行の平均値の中央値。Playは256 visible taps、mz-selectはscroll、それ以外はidle。
GPU、動画、DB、snapshot作成は含まない。今回の修正は動画更新なので、このCPU probe経路を変えない。

| 条件 | 通常版plan µs | Lua実行回数/frame | Lua関数生成 µs/frame | dst展開 µs/frame |
|---|---:|---:|---:|---:|
| Play / ECFN | 64.8 | 0 | 0 | 1.5 |
| Select / ECFN | 123.5 | 8 | 4.2 | 1.3 |
| Select / mz-select scroll | 266.8 | 61 | 28.3 | 3.7 |
| Result / Starseeker | 177.5 | 9 | 3.7 | 6.7 |
| Result / Luxez-Flat | 362.2 | 215 | 97.1 | 13.3 |

「Lua関数生成」は `scope.create_function`、「dst展開」は `flatten_dst_entries` の区間。
これらは計測コードを含む参考値であり、実装すればその時間が全て減るという予測ではない。
計測版のplan全体は通常版より約19～68µs長い。空の区間計測は約22.7～23.1ns/回だった。
表の通常版planと区間値を直接加算したり、FPSへの改善率に換算したりしない。

**1. Lua dispatch関数の生成・破棄を減らす案が最も有力。**

`bmz-skin/src/lua/runtime.rs::evaluate_callback_inner` はcallbackごとに
`Lua::scope` とborrowした状態を参照する関数を作る。
Luxez-Flatでは登録217件のうち215回/frameを実行し、区間内全体249.6µs、
関数生成97.1µs、callback本体40.5µsだった。残りにはregistry取得・slot設定・scope破棄と
計測負荷などが含まれる。mz-selectでも生成28.3µsに対して本体8.8µsだった。
Starseekerは生成3.7µs、本体37.6µsであり、同じ施策の効果はスキンで異なる。

次の設計候補は、同じ状態providerを使う範囲で借用scopeを共有する評価API。
renderer側の `SkinLuaDrawRuntime` とplayer側のadapterまで変更が必要になる。
Selectの行ごとに異なるstate、callbackの呼出順・回数、永続upvalue、エラー時のslot解放、
frame全体とcall単位の命令数上限を維持する。結果のキャッシュや推論への強制変換は行わない。
ライフタイムを延長するunsafeポインタを使う案は採らない。今回は設計・計測までで未実装。

**2. 複数キーフレームの構造と固定メタデータをキャッシュする。**

`bmz-render/src/skin/animation.rs::resolve_destination_frame` は単一frameにfast pathがあるが、
複数frameは毎回Vecへ展開し、cycle・acc・fixed_colorを走査する。
Luxez-Flatでは153回/frame、展開を含む複数frame解決全体が27.8µs。
Starseekerでは124回/frame、全体16.8µsだった。
まず条件展開済みのframe列とcycle/acc/fixed_colorを保存し、補間・`h_expr`・offsetは実行時に残す。
option変更時のcache切替とcloneしたcontextの分離が必要。変更範囲を抑える代案は
小さなframe列のallocationをなくす方法。こちらも実装後のA/B計測が必要。

**3. ID検索の省略は現状では優先度を下げる。**

`value_number_render_items` のvalue再検索、Selectバーのimage/imageset検索、
`core/resolve.rs` の型別検索を計測した。空でない配列だけでも388～795回/frameだが、
合計9.8～24.8µsの大部分は上記の区間計測の負荷に近く、大きな改善余地は確認できない。
採用するならresolved definitionを渡すか型別indexを作る。
現在のimage/value mapは最後の同名定義を使い、既存の線形検索は最初の同名定義を使うため、
単に同じmapへ置き換えると重複IDの意味が変わる。この互換性を先に整理する必要がある。

区間計測版と通常版の348フレームのprimitive hashはStarseeker / Luxez-Flatで一致した。
Selectは実時計の数字が描画に含まれ、同じ通常版の再実行でもhashが変わる。
mz-selectの代表planを比較し、秒の数字のUV差を確認した。
Select全体の画素一致は主張しない。mz-selectの未対応custom timer警告は両版に残る。

再現用ローカル資料:

- `prepare_hotspots.py` / `refine_hotspots.py`: 区間計測を入れた独立コピーの作成。
- `cpu-hotspots-refined/` / `cpu-hotspots-refined-results.json`: 通常版と区間計測版の実行結果。
- `hotspot-summary.json`: callback回数、区間時間、通常版plan時間。
- `verify-hotspots.json` / `mz-plan-dumps/`: hash確認と時計差の調査。

独立コピーのbuildには共有target内の古い成果物が混ざらないようコピー側のmtimeを更新し、
各crateのcompile元パスをログで確認した。調査終了後は計測コードを含む3 crateのrelease成果物を
cleanし、通常のworkspaceから本体・exampleを再buildした。
exampleの再実行で区間計測出力が残っていないことも確認した。

## 検証

- `cargo test --offline -p bmz-player -p bmz-video -- --test-threads=1`:
  player 1991 passed / 3 ignored、video 32 passed。失敗なし。
- `cargo check --offline --workspace`、`cargo fmt --all -- --check`、`git diff --check`: 成功。
- `cargo clippy --offline --workspace --all-targets -- -D warnings`:
  既存の `bmz-player/src/ir/secret_store.rs:49` の `items_after_test_module` で失敗。
  当該lintだけを `-A clippy::items_after_test_module` で除外すると成功。
  ファイルは変更していない。strict Clippy全体の成功とは扱わない。
- release buildとWindows DX12 / VSyncOffの実アプリ起動が成功。採用した再計測ログにERRORなし。
- 前段のレビューではrender 625 passed / 1 ignored、skin 216 passed、
  texture uploadのGPUテスト1件も成功している。今回これらの本体コードは変更していない。
