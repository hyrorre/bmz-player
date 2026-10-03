# スキン互換の対応状況

この文書は、互換修正や次の作業を選ぶための索引です。最終確認日は2026-10-03です。
「対応済み」は記載した範囲の実装・テストがあることを意味し、全スキンでの見た目の完全一致を保証しません。
BMZ固有のIDや詳細な動作契約は [skin.md](skin.md)、互換修正の方針は [AGENTS.md](../AGENTS.md) を参照します。

対応範囲を変更したら、根拠となるコード・テストとともに該当行を更新します。
不具合を追加するときは、スキン名・画面・条件・期待結果と実際の差を記載し、未検証の推測と区別します。
第三者製スキンや個人の設定・DBはコミットに含めません。

## 対応済みの主な範囲

| 項目 | 現在の範囲・確認入口 |
|---|---|
| JSON / Lua / LR2読込 | JSON、`.luaskin` / `.lua`、LR2 CSV skinを読み込む。[decode API](../crates/bmz-skin/src/lib.rs)、[app側decode](../crates/bmz-player/src/skin_loader/decode/document.rs)。LR2全命令の完全互換を意味しない |
| LR2の基準解像度 | `#RESOLUTION` 省略時は640×480。プリセットと幅・高さの明示指定も対応。[仕様](skin.md#lr2の基準解像度)、[回帰テスト](../crates/bmz-skin/src/lr2/tests/cases_01.rs) |
| LR2のDST時刻 | 単一時刻の表示保持、終端を過ぎてからのloop、開始前と負loopの非表示をOpenLR2に合わせる。[仕様](skin.md#lr2のdst時刻)、[回帰テスト](../crates/bmz-render/src/skin/tests/core/cases_05.rs) |
| 旧DXA内のfont / image | 形式v1〜4の直接読込、既定キー・LZ圧縮に対応。KCOOLの3種類の専用フォントを実アーカイブでdecode検証。[仕様と制約](skin.md#lr2のdxaアセット)、[読込](../crates/bmz-skin-assets/src/lib.rs) |
| JSON document | numeric/string IDの正規化、include、property条件等。[schema / loader](../crates/bmz-skin-document/src/lib.rs) |
| 基本描画 | source、image/imageset、value/text、note/gauge/judge、slider、hiddenCover、destinationのtimer/op/draw、keyframe、UV animation等。[描画評価](../crates/bmz-render/src/skin/document_render/) |
| LR2減算合成 | `blend=3` はsource alphaを掛けたRGBを描画先から減算し、描画先alphaを保持する。通常描画とAmbient内の描画に対応 |
| LR2 DST / SRC animation | 区間ごとの三次accとblend/filter/center、SRC/DSTが同じ時計の場合の画像cycle原点を保持。数字はSRC時計で更新 |
| LR2ノーツ | `DST_NOTE`の全行・時計・条件・座標・大きさを評価し、`SRC_AUTO_*`を部分AUTO入力レーンへ適用。全体AUTOPLAYは通常画像 |
| LR2横スクロール | `#HORIZONTAL`で通常ノーツ・LN・小節線を右から左へ移動し、LIFT等のoffsetをXへ適用。参照実装で縦計算が残るLN/小節線はBMZ側で補完。カバー消失ラインの横向きcropは未対応 |
| LR2画像透過色 | RGB画像の既定緑と`#TRANSCOLOR`（互換綴り2種を含む）を適用。元からalphaを持つ画像は維持。DXA内の画像も対象 |
| LR2 system font | `#FONT`の番号・宣言サイズに基づく幅制限とDST高さ、typeの縁取りを適用。DxLib固有の太さ・AA方式は未対応 |
| LR2 2P数値 | Play ref=120..136を2Pの実スコア・コンボ・判定数・達成率・スコア差へ接続。ref=120は確定値で、カウントアップ補間は未対応 |
| BGAの色・透明度 | destinationのRGBAをBase / Layer / Layer2 / POORのtintと乗算する。Ambientにも合成前に適用。[core/resolve](../crates/bmz-render/src/skin/document_render/core/resolve.rs)、[回帰テスト](../crates/bmz-render/src/skin/tests/play/ambient.rs) |
| destination変換 | `center`、`offset` / `offsets`、`filter` の処理がある。[geometry](../crates/bmz-render/src/skin/geometry.rs)、[animation](../crates/bmz-render/src/skin/animation.rs)、[core評価](../crates/bmz-render/src/skin/document_render/core/)。オブジェクトごとの適用経路は個別に確認する |
| stretch | static imageに加えslider・hiddenCover・judge・BGA等の適用経路がある。[graph/image](../crates/bmz-render/src/skin/document_render/graph/image.rs)、[judge](../crates/bmz-render/src/skin/document_render/play/judge.rs)、[core/resolve](../crates/bmz-render/src/skin/document_render/core/resolve.rs) |
| graph | type `101/102/110..115/140..149` の値解決と、Select / Result等のgraph描画がある。[graph値](../crates/bmz-render/src/skin/state_values/graph.rs)、[graph描画](../crates/bmz-render/src/skin/document_render/graph/) |
| Mine sprite | JSON / Luaの `note.mine` を使い、指定がない場合はデフォルトtextureへfallback。[note描画](../crates/bmz-render/src/skin/document_render/play/note.rs)、[plan](../crates/bmz-render/src/plan/play/document.rs) |
| RANDOM gauge animation | `type=0` のanimationをシーンごとのruntime stateで更新する。[GaugeAnimationRuntime](../crates/bmz-render/src/skin/runtime/gauge_animation.rs)。parts・flashの一致確認は下記の未検証項目を参照 |
| text / font | align、overflow、wrapping、shadow/outline、TTF/OTF/TTC、bitmap font等。文字atlas cacheを利用する。[text renderer](../crates/bmz-render/src/renderer/text/)、[GPU側cache利用](../crates/bmz-render/src/renderer/gpu/text.rs) |
| フォント解決 | 同梱フォントを優先し、OSフォントへfallback。path / memory bytes / TTC indexを扱う。[bmz-font](../crates/bmz-font/src/system.rs) |
| SCROLL / SPEED | SCROLLの区間積分と、SPEEDイベント間の線形補間を実装。見かけ距離にはnote位置のSPEED倍率を適用する。[scroll.rs](../crates/bmz-player/src/screens/play_snapshot/scroll.rs)、[snapshotテスト](../crates/bmz-player/src/screens/play_snapshot/tests/cases_03.rs) |

### Luaの推論とruntime

`auto` はfunctionを宣言的なref / expr / draw条件等へ変換できる場合に推論を使い、
推論不能な対応fieldを永続VMへ残します。`--lua-skin-runtime compat` は対応fieldを実行時に評価します。
closure/module stateを維持し、frameごとのmain_stateを参照します。

| field / API | 対応範囲 |
|---|---|
| destination `draw` | ロード時推論とruntime callback |
| Resultのパネル状態 | `Expand_op` / callbackが直接保持する `result_mode` を現在のパネルへ同期。Luxe FlatのGRAPHとIRの混在を防止。通常／compat、左右配置、WMIIの回帰確認は [作業記録](../notes/2026/2026-10-03-result-panel-lua-state.md) |
| value / text / graph / sliderの `value` | ロード時推論とruntime callback |
| `customTimers[].timer` | ID `10000..19999`。推論とruntime callback。宣言順で1フレーム1回更新 |
| `main_state.set_timer` | 受動custom timerのON/OFF。組み込みtimerや能動timerへの書き込みには制約がある |
| `timer_util` | timer_function / timer_observe_boolean / new_passive_timer等をcustom timer内で利用可能 |
| file / module解決 | `SkinPathContext` の許可rootに限定。require / dofile / loadfile、source等のパス解決を扱う |
| `os` / `io` | clock/date/time等の限定API、仮想ファイル読込・永続化しない書込stub。任意のOS操作を許可しない |

timerの単位・OFF値・書き込み制約・失敗時の値は [skin.md](skin.md) のLua Runtime Compatibility Modeを参照します。
実装入口は [function変換](../crates/bmz-skin/src/lua/conversion/function_field.rs)、
[runtime](../crates/bmz-skin/src/lua/runtime.rs)、[sandbox](../crates/bmz-skin/src/lua/sandbox/) です。
命令数・メモリ・table量の上限は互換修正でも維持します。

## 一部対応・未対応

| 項目 | 残っている範囲 |
|---|---|
| DXA | v5以降、独自キー、アーカイブ内wildcard列挙、動画・音声・Lua/CSV includeは未対応 |
| 任意のdestination timer function | custom timerのruntime対応とは別。destinationへ直接渡す任意functionの汎用runtime評価は未対応 |
| `act` / `customEvents` | 一部functionのロード時推論はあるが、任意のaction / conditionを永続VMで実行する汎用イベントruntimeは未対応 |
| その他のLua function field | 汎用float writer等は上記callbackの対象外。対応追加時はfieldと呼び出し契約を個別に定義する |
| function診断 | field path付きwarningやcustom timerのID付き診断はある。object ID・Lua source file / lineの一貫した提示は改善余地がある |
| PMchara | `--` 座標補間と右下pixel色によるchroma keyは未対応。通常のalphaは対応。[詳細仕様](skin.md) |

「未対応Lua timer」のような一括表現は避け、custom timerとdestination timer、
ロード時推論とruntime評価を分けて記載します。

## 未検証の互換差分

次の項目は、再現条件と比較結果が揃っていない確認候補です。
確定した不具合や未実装機能として扱わず、同一スキン・同一条件でbeatorajaと照合して判断します。

- `SkinGauge.prepare` 相当のタイミングでの `parts` 再計算と、モード切り替え時のボーダー位置。
- Lua側の `P1_grooveflash` 等、固定座標flashとBMZ側のgauge表示の一致。
- destination変換やstretchの、上記で確認した描画経路以外への適用範囲。

調査の入口は [gauge描画](../crates/bmz-render/src/skin/document_render/play/gauge.rs) と
`.local/beatoraja/` の対応クラスです。参照ソースや外部アセットがない場合は、未確認と明記します。

## 品質改善案

- text outline / shadowの品質改善。現在のoutlineには周囲8方向へ描く近似がある。
  変更時は字形・サイズ・描画負荷を比較する。
- SDF / 距離場フォントは代替方式の検討候補。現行のatlas cacheとは別の設計課題として扱う。

## 調査・検証の入口

- schema / decode: [bmz-skin-document](../crates/bmz-skin-document/src/)、[bmz-skin](../crates/bmz-skin/src/)
- 描画評価 / regression: [skin評価](../crates/bmz-render/src/skin/)、[skinテスト](../crates/bmz-render/src/skin/tests/)
- appの読込 / install: [skin_loader](../crates/bmz-player/src/skin_loader/)、[loaderテスト](../crates/bmz-player/src/skin_loader/tests/)

外部スキンの存在を条件にするテストは、素材が無いと早期returnする場合があります。
テスト成功件数だけで、そのスキンを検証したと判断しないでください。
検証コマンドの選択は [AGENTS.md](../AGENTS.md) に集約します。
