# 起動時間の調査（2026-09-26）

## 結論

macOS上で公式配布版v0.2.1、v0.4.0、v0.4.2を同一設定・DB・スキンで各3回測定した。
選曲初期描画まで約5.1秒で、今回の条件では大幅な起動時間の悪化は再現しなかった。
一方、起動時間自体の大部分を占める処理は特定できた。
**描画用とegui用が、CJKフォント探索をメインスレッドでそれぞれ実行し、合計約4.2〜4.4秒を費やしている。**

このフォント探索経路は指定の旧版にも存在する。今回の結果だけから、体感差を最近の変更やOS更新に帰属させることはできない。
Windows/Linux、再起動直後、当時のOS・設定・同梱スキンを含む環境全体の比較は未実施。

## 比較条件と結果

- Apple M4 / macOS 26.6.2 (25G83)、arm64、Metal。
- インストール版の設定・DBを毎回別ディレクトリへコピー。元のDBにWALがないことを確認してからコピーした。
- Windowed / 1280×720 / Native / VSync / target_fps=240。
- 全版にv0.4.2の同一Resourcesを指定。選曲は `mz-select/music_select.luaskin`。
  エンジンと同梱ライブラリの差を比較する条件であり、各版の当時の既定アセットの差は含まない。
- IR・難易度表取得・更新確認・OBS等の外部連携を無効化、master volume=0。
- v0.2.1互換のため、全版の比較用profileでassistを旧形式の `"None"` とし、
  `classic_hispeed` / `floating_target_green` を互換のある旧キー名に変換。
  旧版非対応のUI入力bindingsは除去。プレイは開始せず、選曲で自動終了する。
- アプリは1個ずつ起動し、コンパイル・テスト・sample採取とは同時実行しない。
  実行順は 0.2.1→0.4.0→0.4.2、逆順、0.4.0→0.2.1→0.4.2。
- OSキャッシュを消去しない反復起動。ダウンロード直後の初回起動、設定の互換性確認、
  ビルド直後の起動、sample付き試行は本表から除外。

プロセス生成直前から各ログイベントまでの壁時計時間。単位は秒。

| 公式配布版 | 選曲初回描画直前・中央値 | 3フレーム処理完了・中央値 | 3フレーム処理完了・最小〜最大 |
|---|---:|---:|---:|
| v0.2.1 | 5.099 | 5.145 | 5.137〜5.203 |
| v0.4.0 | 5.057 | 5.111 | 5.099〜5.139 |
| v0.4.2 | 5.110 | 5.167 | 5.099〜5.346 |

v0.4.2の中央値差は、v0.2.1比+22ms（+0.4%）、v0.4.0比+56ms（+1.1%）。
3回の小標本で微小な回帰の有無は断定しないが、秒単位の悪化は確認できない。

測定点は `app scene active scene=Select` と `smoke exit frame count reached`。
v0.2.1にはfirst-presentログがないため、全版共通の指標を使用した。
3フレーム処理完了は厳密な初回present時刻ではない。
v0.4.0/0.4.2ではsuccessful presentログを確認し、別の120フレーム試行では
v0.2.1/0.4.2のスクリーンショットを確認して、実際に同じ選曲スキンが描画されていることを検証した。
全9試行は正常終了し、ERROR・スキンロード失敗・難易度表の取得はなかった。

旧版はリポジトリの公式GitHub Releaseから実行ファイル・ライブラリを取得した。
ZIP全体をGitHub公開SHA256と照合し、展開した各バイナリとの完全一致とcodesign整合性を確認した。

| バージョン | ソースタグ | arm64配布ZIPのSHA256 |
|---|---|---|
| v0.2.1 | `5e0a5259`（8月12日） | `97b36ee4348113e8043266a51f3349a2f568ec745517412c020ee2ddea18085d` |
| v0.4.0 | `59de3ef5`（9月13日） | `1ade529428cecc1be028af2fc11d26338e7eeeb374d0db25dcfe77f916255392` |

## 現行コードの内訳

HEAD `38e71040` に計測ログだけを追加したreleaseビルドを、同じインストール版データで計測。
`measured-head-1` のアプリ内起動タイマーから初回presentまで4,845ms。
ビルド直後のこの試行にはログ初期化前にも約890msの時間があり、上の公式版比較には混ぜていない。

| 区間 | 時間 |
|---|---:|
| 設定・DB・サンプル曲scanを含むbootstrap | 6ms |
| 全スキンのカタログ走査 | 55ms |
| 初期スキン準備 | 319ms |
| うち選曲document decode | 58ms |
| うち選曲font decode | 245ms |
| うち選曲source decode | 11ms |
| 描画用デフォルトフォント解決・読込 | **2,102ms** |
| egui用フォント解決・読込 | **2,078ms** |

font/source/documentの行はスキン準備の内訳なので重複して加算しない。
フォントfallback準備2回だけで4,180ms、初回presentまでの約86%を占める。
`window and renderer surface ready` までの長さはGPUだけの負荷ではなく、
その内部で行うデフォルトフォント探索の時間を含んでいる。

## 静的解析とスタック採取

1. `crates/bmz-render/src/renderer/gpu/init.rs` のrenderer初期化が
   `load_default_font_fallbacks()` を同期実行する。
2. 続いて `crates/bmz-player/src/app/input_lifecycle.rs` が `EguiLayer::new()` を呼ぶ。
   `crates/bmz-player/src/ui/runtime.rs` の `install_cjk_fonts()` は、
   `load_cjk_font_fallback_data()` を通じて再び同じフォント解決を行う。
3. 両者は `crates/bmz-font/src/system.rs::resolve_font_fallbacks()` に入り、
   日本語・韓国語・簡体字・繁体字・香港字形の5種類を毎回探索する。結果の共有キャッシュはない。
4. `font-kit 0.14.3` のmacOS実装では、family選択時にTTCデータを読み込み、
   各descriptorに対応するfaceをcollection内から探す。
   `create_handles_from_core_text_collection()` → `Font::from_bytes()` → CoreText/CFDataの経路で
   メモリコピーやフォント解析が繰り返される。
5. 同梱フォントがあっても、現在はfamily候補が外側のループである。
   `MultiSource(FsSource, SystemSource)` は「同じfamily名での同梱優先」なので、
   Notoより先に列挙されるOS固有familyの探索コストを省略できない。

`sample` のmain thread 5,054サンプル中、描画用fallbackの解決下に1,843、
egui用fallbackの解決下に1,829サンプルがあった（合計約73%）。
これは別試行のサンプリング比率であり、上の壁時計割合とは別の指標。

フォント単独probeの2回目:

| coverage | 解決時間 | 選択結果 |
|---|---:|---|
| Japanese | 38ms | メモリ上の約7.9MBのface |
| Korean | 306ms | メモリ上の約55.4MBのcollectionのface |
| SimplifiedChinese | 656ms | 同梱Noto Sans CJK |
| TraditionalChinese | 645ms | 同梱Noto Sans CJK |
| HongKong | 650ms | 同梱Noto Sans CJK |

全fallbackを一括解決するprobeは1回目2,218ms、2回目2,212ms。
単独coverageの測定はそれぞれsourceの初期化を含むため、合計値と完全には一致しない。

全CJK fallbackは7月16日の `2a6462fc`、同梱font sourceは7月29日の `45447e48` に由来する。
この主要経路はv0.2.1/v0.4.0にも存在し、今回の旧版比較と整合している。

## 二次的な待ち時間

開発ディレクトリのprofile（ECFN選曲）でも計測した。
選曲スキンの同期準備は約0.8秒、全スキンのカタログ走査は約0.15〜0.18秒だった。
カタログ候補は113件。`scan_skin_catalog_dir()` は未使用スキンも含め再帰走査して、
Lua/JSON/LR2ヘッダーを同期ロードする。

ECFNを標準select.jsonに変えると初期スキン準備は8msまで減ったが、
初回presentまでは4,788msかかった。フォントfallbackの待ち時間は残る。
したがって今回の優先調査対象はDB migrationや曲scan、GPU shader compilationではなくフォント解決である。

## 改善する場合の優先順位

最初の調査では計測ログと診断exampleのみを追加した。以下の改善結果を後段に追記する。

1. **解決済みのフォントをrendererとeguiで共有する。**
   `font_roots` とcoverageをキーにし、再探索を避ける。
   `SystemSource` 自体のstatic共有は以前Linuxで問題になっているため、
   パス・face index・`Arc`で保持したバイト列など、解決結果を共有する設計が適切。
2. **同梱フォントをOS family探索より先に解決できるか検討する。**
   選ばれる字形・メトリクスが変わるので、CJK表示とfallback順の確認が必要。
   中国語3系統の約2秒/探索のコストに対する候補だが、短縮量は未検証。
3. 選曲bitmap fontのdecode共有・キャッシュと、スキンカタログの遅延走査を検討する。
   今回の環境では効果の上限が上2項目より小さい。

## 再計測

永続的な計測点:

- `startup constructor timings`: カタログ・初期スキン・前後処理。
- `startup synchronous skin decode complete`: document/font/source別の時間。
- `default render font fallbacks loaded` / `egui font fallbacks loaded`: フォントの時間と個数。
- `RUST_LOG=info,bmz_player::startup_profile=debug`: カタログ候補別の時間。

フォント単独probe:

```bash
cargo run --release -p bmz-font --example startup_profile -- data/fonts/noto-cjk
```

ローカル測定資料（gitignore対象）: `.local/performance/2026-09-26/startup/`

- `probe.py`: 設定・DBコピーと起動・ログ解析。
- `compare.py`: 共通条件で公式3版を各3回起動。
- `comparison-summary.json`: 比較結果。
- `font-probe.txt`: フォント単独測定。
- `runs/current-sample/sample.txt`: 起動時スタック。
- `runs/compare-*/`: 採用した9試行のログ・個別集計。
- `runs/measured-head-1/`: 詳細計測ログ。
- `runs/verify-v021-screen/` / `runs/verify-v042-screen/`: 描画確認。

初回の開発ビルドは削除済みHomebrew FFmpeg 9.0.1のキャッシュ参照でリンクに失敗した。
`LIBRARY_PATH=/opt/homebrew/opt/ffmpeg/lib` を指定して現在の9.0.2でリンクした。
公式版同士の比較では、各配布物に同梱されたFFmpegをそのまま使用している。

## 検証

releaseビルド、実画面起動、計測ログ、旧版・現行版の選曲スクリーンショットを確認。
通常の設定・DB・第三者製スキンは変更していない。

- `cargo fmt --check`: 成功。
- `cargo check --locked`: 成功。
- `cargo clippy --locked`: 成功。
- `cargo test -p bmz-player --lib startup --locked`: 12件成功。
- `cargo test -p bmz-font --locked`: 9件成功。
- `git diff --check`: 成功。

リンクを伴うコマンドには上記の `LIBRARY_PATH` を指定した。

## 改善1: フォント解決結果の共有

`c15b23cd` + 計測ログ（`faff8cd6`）を新しい比較基準とし、同じreleaseビルド条件で比較。
この節以降はアプリ内起動タイマーからsuccessful first presentまでを測るため、
公式版比較の「プロセス生成〜3フレーム完了」とは指標が異なる。
変更前後を交互に各4回起動し、各バイナリの最初の1回を除いた3回の中央値を採用した。
設定・DBは試行ごとのコピーを使用し、コンパイルやテストは計測と同時実行しない。

`bmz-font` が最後に使用した正規化済みfont rootsの解決結果をプロセス内に保持する。
rendererとeguiは同じパス・face index・`Arc`のbytesを再利用する。
coverageの優先順と重複除去を維持し、locale変更は再探索せず並び替える。
異なるrootsではキャッシュを置換し、OSフォントの変更は次回起動で反映する。
OSのsourceオブジェクトは共有しない。

| 選曲スキン | 変更前中央値 | 変更後中央値 | 短縮 | 各3回の値（前 → 後、ms） |
|---|---:|---:|---:|---|
| mz-select | 4,789ms | 2,713ms | 43.3% | 4813/4789/4789 → 2727/2713/2702 |
| ECFN | 5,189ms | 3,172ms | 38.9% | 4889/5189/5439 → 3141/3195/3172 |

mz-selectのegui font loadは2,051〜2,076msから6〜7msになった。
renderer側の最初の探索は約2.1秒のままで、次の改善対象になる。

検証: `cargo test -p bmz-font --locked` 11件、`cargo test -p bmz-render fallback --locked`
20件、`cargo clippy -p bmz-font --all-targets --locked -- -D warnings` 成功。
生ログと集計は `optimization/{installed,ecfn}-baseline-step1.json` と
`runs/opt-{installed,ecfn}-baseline-step1-*/` に保存した。

## 改善2: 同梱フォントをOS探索より優先

同梱source内で候補familyをすべて探してから、見つからないcoverageだけOSを探索する。
これにより同梱Notoが利用できる場合、CJK探索用のOS source自体も初期化しない。
汎用SansSerifの追加fallbackは従来どおりOSから取得する。
日本語・韓国語の既定fallbackはmacOS固有フォントからNotoへ変わるので字形・メトリクスは変わる。
スキンが明示したフォントは従来どおり使用する。

| 選曲スキン | 改善1中央値（再測定） | 改善2中央値 | この段階の短縮 | 各3回の値（前 → 後、ms） |
|---|---:|---:|---:|---|
| mz-select | 2,739ms | 695ms | 74.6% | 2782/2739/2731 → 690/701/695 |
| ECFN | 3,158ms | 1,071ms | 66.1% | 3103/3193/3158 → 1320/1021/1071 |

全試行は正常終了。mz-selectの1試行ではrendererのfont loadは56ms、eguiは8ms。
第1段階の前からの短縮率はmz-selectで約85%。
ECFNには数百msの試行差があり、小さい差の評価には追加測定が必要。

検証: `bmz-font` 12件（同梱の地域別5face・代表グリフ・OS未探索・同梱なしのfallbackを含む）、
renderer fallback 20件、bmz-fontのclippy成功。
描画確認用試行は計測表から除外した。
集計は `optimization/{installed,ecfn}-step1-step2.json`。
