# LR2スキン描画準備の最適化（2026-10-16）

v0.4.3以降に追加したLR2スキンの描画経路を調べ、Selectの定義探索・BAR部品の振り分け・数値画像の再利用を改善した。
変更前の基準はmainの `2af86f49`。仕様は [スキン仕様](../../docs/skin.md) を参照。

## 実装

- `DocumentPlanningCache` にdestinationごとのobject indexと検索欄textの組を保持する。
  表示条件・timer・値・Lua callbackは毎フレーム評価する。image / valueの既存の重複ID規則、
  その他のobjectの最初の定義を使う規則、描画できなかった種類から別種類へのfallbackを維持する。
- LR2 BARのmarker一覧とorder集合を構造cacheへ置く。orderを持つ部品は1回の走査で振り分け、
  各DST位置で取り出す。行順・未指定部品・重複marker・clipで見えない最初のmarkerを扱い、
  通常画像との前後関係を保つ。旧形式のindex指定markerは従来の照合を使う。
  Lua runtimeが無い場合はバー本体のorderを既に選んだdestinationから取得し、条件の再選択を減らす。
  Lua runtimeがある場合は元の条件評価回数・順序を維持する。
- LR2数値のcacheキーを経過msから実際の画像frame番号へ変更する。
  1フレーム素材のcycle指定でも再利用できる。回転とLR2 blend/filter/centerも照合し、
  描画属性が変わったときに古い数字を再利用しない。clipはcache外で適用する。
- Playのレーン座標とnote frameの重複解決を、Lua runtimeが無い場合に共有する。
  Lua条件の評価回数・順序と、基準レーン矩形が無い場合の早期returnは維持する。

schema・外部スキン・設定・保存データは変更していない。汎用keyframe metadata cacheや
LN/Mine専用sprite cacheは追加していない。

## 検証

- `cargo fmt --check`
- `cargo check -p bmz-render --locked --offline`
- `cargo check -p bmz-player --locked --offline`
- `cargo clippy -p bmz-render --all-targets --locked --offline -- -D warnings`
- `cargo test -p bmz-render --locked --offline`: 758 passed、6 ignored。
- `cargo test -p bmz-player --locked --offline --lib skin_loader`: 217 passed、27 ignored。
- `cargo build -p bmz-player --release --locked --offline --examples --bin bmz-player`

追加した回帰テストでは、検索欄の重複ID・表示/編集状態、object種類間のfallback、
BARの未指定部品・非表示marker・旧形式markerとLua条件の再評価、
10/11/24セル数値のanimation境界、数字の回転/style変更、PlayのLua条件の評価回数を確認した。
既存のLua value/draw順序・source差替え・skin更新・Selectスクロールのテストも通過した。

Cargoのincremental cacheでhard linkを作れずコピーへfallbackする警告が出たが、
check / Clippy / testは成功した。warning抑制や環境に合わせたコード変更はしていない。

## 計測条件

Windows x64、Core i9-12900K、RTX 5090、NVIDIA driver 617.42。
変更前後のrelease実行ファイルを保存し、同じLR2 / WMIX HD / ECBEのコピーを読む。
外部CSV・画像は編集せず、設定・DB・cache・ログは計測専用ディレクトリへ分離する。

CPU probeは300フレームwarmup後に6000フレーム、各条件3回。
実行順を反転し、同じ条件の変更前後を隣接して実行する。Selectは25行の合成snapshot、
Playは可視tap 8 / 64 / 256個とLN2本、Result / Course Resultは合成グラフ。
CPU probeはGPU・文字ラスタライズ・動画・DB・app側snapshot構築を含まない。

描画内容の比較には別実行でSelect / Result / Course Resultの全commandのhashと、
Playの代表frameのplan dumpを使用する。時間計測とは分け、結果の一致を確認する。
ECBEは現在日時refの秒を表示するため、逐次起動では時計の数字だけhashが変わる。
同じ秒に2プロセスを起動して再確認し、idle / scroll / panelの全hashが一致した。
この同時実行のCPU時間は性能の集計に使わない。

実ウィンドウは1280×720、Windowed、DX12、VSyncOff、実効Immediate、frame latency 1。
focus内外ともFPS無制限。各sceneを12000フレーム描き、最初の3600フレームを除外する。
ゲームパッド・OBS・Discord・更新確認・難易度表取得は無効。
Selectは初期ライブラリの機能フォルダー、Playは同梱sample autoplay、Resultはdebug合成結果。
WMIX Playは存在するCUSTOMFILE `space.png` / `perfect_dark.png` を計測profileで選ぶ。

実表示cadenceは計測していない。FPSはアプリの描画ループ速度として記録し、
CPU側のplan時間、redraw全体、surface取得・presentを区別する。
WMIX Selectの `movie/off.mpg` はdecodeできず、可視動画0の条件。
実操作の選曲スクロール/パネル、4K、Vulkan、他OS、密集LN、LR2本体との同条件比較は未実施。

## 計測結果

CPU probeの各実行の平均時間について、3回の中央値を比較した。単位はµs。

| Selectスキン | 条件 | 変更前 | 変更後 | 短縮率 |
|---|---|---:|---:|---:|
| LR2標準 | idle | 142.54 | 94.29 | 33.9% |
| LR2標準 | scroll | 145.74 | 92.99 | 36.2% |
| LR2標準 | panel | 158.72 | 103.71 | 34.7% |
| WMIX HD | idle | 154.41 | 106.88 | 30.8% |
| WMIX HD | scroll | 156.59 | 109.71 | 29.9% |
| WMIX HD | panel | 168.09 | 115.83 | 31.1% |
| ECBE | idle | 173.33 | 117.91 | 32.0% |
| ECBE | scroll | 171.53 | 114.46 | 33.3% |
| ECBE | panel | 172.55 | 118.29 | 31.4% |

PlayのCPU時間の変化は -0.6〜5.5%の短縮、WMIX Resultは51.12→50.59µs、
Course Resultは18.51→18.63µs。準備段階の計測ではPlayの改善を再現できていないため、
今回の明確な性能改善はSelectと判断する。

全13ケースの描画比較が一致した。Select 3スキン×3条件とWMIX Result / Course Resultは
各1300フレームの描画プリミティブ全体を比較し、Play 2スキンは各3負荷の代表3フレームを比較した。
RectBatchのcache IDとbatch境界を除き、展開したプリミティブの内容・順序を照合した。
WMIX / ECBE Selectの変更前後のスクリーンショットも確認した。

実ウィンドウの結果も各実行の平均値の3回中央値。時間はms、FPSは描画ループ速度。

| scene | plan 変更前→後 | redraw全体 変更前→後 | FPS 変更前→後 | FPS差 |
|---|---:|---:|---:|---:|
| WMIX Select | 0.145→0.100 | 0.547→0.508 | 1693→1809 | +6.8% |
| ECBE Select | 0.160→0.105 | 0.571→0.513 | 1598→1792 | +12.1% |
| WMIX Play | 0.067→0.066 | 0.454→0.446 | 1988→1989 | +0.0% |
| WMIX Result | 0.067→0.064 | 0.420→0.411 | 2113→2178 | +3.1% |

Selectのsurface取得時間はWMIX 0.044→0.053ms、ECBE 0.042→0.055ms。
presentはWMIX 0.112→0.112ms、ECBE 0.117→0.110ms。
Selectは変更前後ともcommand数が同じ（WMIX 97、ECBE 115）で、CPUのplan時間短縮が再現した。
各対のFPSも3回とも増えたが、surface/present待ちは変動するため、この割合を別環境へ一般化しない。
Playはほぼ同程度。Resultは各対で増減があり、FPSの小さな差を明確な改善とは判断しない。
全24プロセスが正常終了し、各100 profile block、input panicなし、難易度表の取得開始なしを確認した。

生ログ・コピーした素材・比較用実行ファイル・script・詳細JSONは
`.local/performance/lr2-optimization-20261016/` に保存する。Git管理・再配布の対象には含めない。
