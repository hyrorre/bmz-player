# スキン関連レビューの修正と最適化

2026-10-21。レビュー対象は `88b8196c` 自体から `aa1c15b4` まで。
作業開始時のHEADは `a7615105`。第三者製スキン、設定、DBは変更しない。

## レビュー指摘の修正

1. `bit32`をglobalと同じテーブルで`package.loaded`へ登録し、`require("bit32")`を使えるようにした。
   ヘッダ・文書読込とpersistent runtime callbackを確認する回帰テストを追加した。
2. ビットマップフォントの見た目推定に、ページ矩形・画素データ長・総走査画素数（1,048,576）・
   行座標の加算と近傍行・行座標幅（16,384px）の検証を追加した。
   不正な定義ではフォント本体を変えず、推定を見送る。public loader経由の再現もテストする。
3. `bit32.extract` / `replace`は加算の前にfieldとwidthを検証する。
   `math.maxinteger`と有効な32bit全幅・最上位bitを回帰テストへ追加した。

3件とも修正前に回帰テストの失敗を確認し、修正後に対象テストが通ることを確認した。

検証: fmt、workspace check / all-targets Clippy（`-D warnings`）成功。
Windowsの通常権限でworkspace testは4,548成功、0失敗、54 ignored（成功数は子プロセスの1件を含む）。
制限環境では一時ファイルとlocalhostへのアクセスで無関係なテストが失敗するため、
同じ失敗テストが通常権限で成功することを確認して全体を再実行した。
生ログは`fix-workspace-tests.log`。実画面・他OSの確認は未実施。

## 最適化と比較計測

アニメーションの静的情報、Lua APIの呼出経路、欠字の距離変換の順で実装・比較する。
条件・timer・Lua callbackの評価順と回数、closure state、sandboxの上限を維持する。
計測はreleaseで変更前後を交互に実行し、CPU処理時間と実ウィンドウの待ち時間を区別する。
生ログ・比較用実行ファイルは`.local/performance/skin-review-20261021/`へ保存する。

### 1. アニメーション情報のキャッシュ（採用見送り）

`DestinationAnimationMetadata`を文書のplanning cacheへ保持し、通常描画で再集計しない実装を試した。
単一フレームには従来の専用経路を使い、時刻・条件・Lua callbackの評価回数と順序は変えない。
LR2 / beatorajaのループ境界・条件付きフレーム・文書差し替えの比較テストとrenderer全テスト、
check / all-targets Clippyは成功した。

Windows 11（10.0.26300）、Core i9-12900K、RTX 5090（driver 32.0.16.1742）、rustc 1.98.1。
変更前は`6cde03f6`。既存の`scene_plan_profile`で300フレームwarmup後に6,000フレーム計測し、
変更前後を隣接して3回、2回目は実行順を反転した。GPU・文字ラスタライズ・snapshot作成は含まない。

最初の実装ではLR2 Selectが1〜3%遅くなったため、metadataを値で渡さず参照にし、
単一フレームではmetadataを読み出さない実装も試した。再計測は次のとおり。
各実行の平均CPU時間の3回中央値で、単位はµs。

| skin / 条件 | 変更前 | 再調整後 |
|---|---:|---:|
| WMIX Select idle | 104.13 | 106.15 |
| WMIX Select scroll | 103.73 | 109.10 |
| WMIX Select panel | 113.26 | 112.29 |
| ECBE Select idle | 112.37 | 115.54 |
| ECBE Select scroll | 115.87 | 118.29 |
| ECBE Select panel | 117.76 | 120.81 |
| mz-select idle | 199.96 | 197.92 |
| mz-select scroll | 200.63 | 202.97 |
| WMIX Result | 50.83 | 50.99 |

全体の改善を確認できず、LR2 Selectの複数条件で2〜5%遅い結果になったため、この最適化は戻した。
static metadataだけのキャッシュでは、追加の引数・cacheアクセス・構造体サイズ等のコストを
取り戻せない可能性がある。原因の寄与は未分離で、別環境への一般化はしない。

WMIXの4条件は全描画commandのhashが一致した。ECBEの3条件は時計の秒を揃えた別実行で一致した。
mz-selectのhashは時計のUV差を含むため一致根拠には使わない。同じ変更前実行ファイルを2回実行しても
同じ位置の数字UVだけが変わることをplan dumpで確認した。比較中のCPU時間は採用しない。
実験差分は`animation-experiment.patch`、最初の計測は`prototype/`、再調整後は`animation-cpu/`に保存した。

### 2. Lua APIの呼出経路（採用）

従来は安定したRust accessorが毎回scoped Rust dispatcherを呼び、1回のプロパティ参照で
Lua/Rust境界を2回通っていた。安定したaccessorを小さなLua closureへ置き、private slotから
scoped dispatcherを直接呼ぶ。load時のprovider、ネスト復元、panic時のguard、
stateful callbackの評価順・回数、命令数・メモリ上限は維持する。
`set_timer`の引数検証とadapterの再入保護用mutexは従来のままにした。

再実行用の`lua_runtime_profile` exampleを追加した。300フレームwarmup後、10,000フレームを
変更前後で交互に3回計測した。scope準備を含むAPI/callbackのCPU時間であり、GPUや描画FPSではない。
各実行の平均時間の3回中央値（µs）は次のとおり。

| scope / workload | 変更前 | 変更後 | 短縮率 |
|---|---:|---:|---:|
| callbackごと / draw 500件 | 620.20 | 551.85 | 11.0% |
| callbackごと / API 53回 | 16.23 | 6.73 | 58.5% |
| 共有 / draw 500件 | 223.18 | 128.51 | 42.4% |
| 共有 / API 53回 | 14.12 | 6.68 | 52.7% |

全callbackの戻り値と失敗件数0をprobeで検証した。`bmz-skin`は321成功、1 ignored、
check / all-targets Clippyも成功。既存テストで捕捉したaccessor、ネスト・panic復元、
callback評価順、frame/callback命令数上限、20,000フレーム後のメモリ上限を確認した。
生ログは`lua-baseline-*.jsonl` / `lua-lua-*.jsonl`。

### 3. 欠字の距離変換バッファ再利用（採用）

距離変換の包絡線を求める`vertices` / `bounds`を、1文字につき2本確保して全行・全列で再利用する。
従来は各行・各列で2本ずつ確保していた。生存期間は1文字の変換中だけで、文字数に応じた蓄積はない。
先頭頂点と境界を各変換で初期化し、前の行の値を持ち越さない。

縦横比6種×mask8種×縦横倍率3種の全画素を、最も近い不透明画素までの総当たり距離と比較する
テストを追加した。rendererは914成功、9 ignored、check / all-targets Clippyも成功。

releaseの専用probeを追加し、test用記号フォントの✡を塗り・縁取り付きで20回warmup後に200回描いた。
フォント読込・atlas格納・GPU upload・hash計算は測定外。変更前後を交互に3回実行した平均時間の
中央値（µs）は次のとおり。

| PxScale | 変更前 | 変更後 | 判断 |
|---|---:|---:|---|
| 24 | 54.71 | 50.87 | 7.0%短縮 |
| 72 | 485.92 | 452.23 | 6.9%短縮 |
| 192 | 2915.96 | 2909.20 | 同程度 |

全3種でRGBAのhashと画像寸法が一致した。効果はatlasに無い文字の初回描画に限られ、
定常FPSの改善とは扱わない。生ログは`glyph-baseline-*.jsonl` / `glyph-final-*.jsonl`。

## 採用した変更をまとめた実スキン計測

変更前は指摘修正後の`6cde03f6`、変更後はLua・欠字処理の最適化を含む`ad8bca93`。
前述と同じWindows環境で、release実行ファイルを保存して比較した。
アニメーション情報のキャッシュ実験は変更後に含めない。
再実行用probeの手順は[development.md](../../docs/development.md)を参照する。

### CPU描画準備

実際のWMIX / ECBE / mz-selectのアセットで、300フレームwarmup後に6,000フレーム計測した。
変更前後を交互に3回実行し、2回目は順序を反転した。各実行の平均CPU時間の中央値（µs）。
GPU描画・文字ラスタライズ・snapshot作成は含まない。

| skin / 条件 | 変更前 | 変更後 |
|---|---:|---:|
| WMIX Select idle | 107.17 | 107.70 |
| WMIX Select scroll | 108.72 | 108.67 |
| WMIX Select panel | 115.17 | 113.40 |
| ECBE Select idle | 117.27 | 115.57 |
| ECBE Select scroll | 117.80 | 118.92 |
| ECBE Select panel | 120.42 | 122.79 |
| WMIX Result idle | 50.99 | 50.53 |
| mz-select idle | 194.46 | 185.64 |
| mz-select scroll | 203.04 | 189.25 |

mz-selectでは4.5〜6.8%短縮した。WMIX / ECBE / Resultの差は約±2%以内で、明確な改善とは扱わない。
生ログは`cpu-baseline-*.jsonl` / `cpu-final-*.jsonl`、集計は`summary.json`。

描画内容は次の範囲で変更前後の一致を確認した。

- WMIX Selectのidle / scroll / panelとResultは全commandのhashが一致した。
- ECBE Selectの3条件はシステム時計の秒を揃えた実行で全commandのhashが一致した。
- mz-selectはidle / scrollの各0 / 299 / 300 / 1299フレーム、合計8個のplan dumpを比較した。
  時計の数字6桁だけを描画矩形とatlas内の行で特定して除外すると、残りの内容・順序はすべて一致した。
  時計の定義は`data/skins/mz-select/customize/advanced/default_time/parts.lua`のref 21〜26。
  時計を含む全hashの一致は主張しない。比較スクリプトは`compare-mz-plans.py`。

### 実ウィンドウ

1280×720、DX12、VSyncOff（実効Immediate）、maximum frame latency 1、
フォーカス時・非フォーカス時ともFPS制限なし。同じsample1曲と機能フォルダを表示した。
設定・profile・DB・cacheは計測専用ディレクトリへ分離し、IR・難易度表自動取得・更新確認・
OBS・Discord・gamepadを無効にした。初回DBへのsample登録は描画計測開始前に完了した。

WMIX / mz-selectを変更前後で各3回、計12プロセス実行した。各12,000フレームのうち
先頭3,600フレームを除外し、残り8,400フレームの平均を各実行で求めて3回中央値を比較した。
全実行で動画のactive / visible sourceは0。表示FPSではなく、アプリの描画ループ速度である。

| skin | 項目 | 変更前 | 変更後 |
|---|---|---:|---:|
| mz-select | plan（ms） | 0.2112 | 0.1996 |
| mz-select | total redraw（ms） | 0.6198 | 0.6158 |
| mz-select | surface取得（ms） | 0.0048 | 0.0051 |
| mz-select | present（ms） | 0.1150 | 0.1158 |
| mz-select | 描画ループFPS | 1509.1 | 1518.8 |
| WMIX | plan（ms） | 0.0973 | 0.0960 |
| WMIX | total redraw（ms） | 0.4500 | 0.4447 |
| WMIX | surface取得（ms） | 0.0112 | 0.0079 |
| WMIX | present（ms） | 0.1135 | 0.1143 |
| WMIX | 描画ループFPS | 1991.7 | 2014.5 |

mz-selectのplan時間は5.5%短縮し、CPU単独probeと同じ方向の結果になった。
FPS差は0.6〜1.1%と小さく、surface取得などにも揺れがあるため、明確なFPS改善とは扱わない。
生ログ・スクリーンショットは`window-*`、実行手順は`compare-window.ps1`。

全12実行が正常終了し、ERROR / panic / runtime callback失敗はなかった。
WMIXの未対応README・MOUSECURSOR命令、`off.mpg`の先頭frame decode失敗、
mz-selectのgenerated timer 9000推論不可のwarningは変更前後で同じ内容・件数だった。
動画を正常再生した場合の性能は今回の計測範囲に含まない。
WMIX / mz-selectの変更前後の静止画4枚を目視比較し、時計・アニメーション以外に目立つ差は見られなかった。

## 最終検証

- `cargo fmt --check`成功。
- `cargo check --workspace --locked --offline`成功。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`成功。
- 通常のWindows権限で`cargo test --workspace --locked --offline --no-fail-fast`成功。
  各test binaryの結果は合計4,548成功、0失敗、55 ignored。
  子プロセスで再実行する1件も成功し、ログ内の成功数をすべて足すと4,549になる。
- `bmz-player`本体と`scene_plan_profile` / `play_plan_profile` exampleのrelease build成功。

生ログは`final-fmt.log` / `final-check.log` / `final-clippy.log` / `final-tests.log` / `final-release.log`。
実プレイの入力・音声遅延、動画正常再生時、他OS / GPU backend、LR2 / beatoraja本体との実行比較は未実施。
