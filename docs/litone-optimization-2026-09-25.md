# LITONE9の表示同等性修正と描画最適化

## 実装順序

1. `cd17636c`: `secret_store.rs` のテストモジュールを末尾へ移し、既存の
   `clippy::items_after_test_module` 警告を解消。本体の処理は変更しない。
2. `52d563db`: LuaのIRランキング欠損値と、数値画像の非表示処理を修正。
3. `e771a1d`: Select / Play / Resultの通常destinationの数値画像を、前回の描画入力が同じ場合に再利用。
4. `ef7b5b5e`: Lua runtime callbackの関数ハンドルをロード時から保持。

外部スキンは変更していない。通常利用のVsyncOff設定も維持する。

## IR欠損値とAuto/Compat

IR未取得時、native refはNoneとなって非表示になる一方、Lua APIは0を返していた。
LITONE9のCompatではIR EXSCORE1～3が0として描かれ、Autoと3個の画像命令差があった。

beatoraja 0.8.8の配布jarに含まれる `IntegerPropertyFactory$ValueType` と
`SkinNumber.prepare` の挙動に合わせ、EXSCORE/順位 ref380～399の欠損はLuaへ
Integer.MIN_VALUEを渡す。Lua条件や文字列からはその値を読めるままにして、
数値画像の生成時にのみintのMIN/MAXを非表示にする。実在するスコア0は表示する。

自己完結のLuaスキンで、同一contextを再使用して
欠損 → 0/42/9999到着 → MIN/MAX → 再欠損を試し、Auto/Compatの描画一致を検証した。
既存のResult BP用sentinelと、対象外refの0 fallbackは維持する。

## 数値キャッシュ

各destinationに前回の数値画像と入力を1件だけ保持する。値、座標、幅・高さ、RGBA、
texture ID、source寸法、画像animationの位相、符号の描画方法が一致すると、
数値の文字列化、桁分解、UV・矩形生成を省く。

数値画像側のvalue定義のindexも保持し、キャッシュhit時の線形探索を省く。
重複IDでは既存の「propertyは最後、画像定義は最初」という挙動を維持する。
エントリー数はdocumentのdestination数に制限され、過去のスコアを蓄積しない。
document/静的optionの更新時は既存のcontextキャッシュ破棄経路で更新する。

timer、draw、op、座標補間、マウス判定、Lua valueの評価を済ませてからキャッシュを参照する。
同じ値を返すstateful Lua callbackも毎回呼び、呼び出し順を保つ。
画像がなくなった場合は描かず、画像差し替えや寸法変更も照合する。
この変更は描画plan全体を保存したり、prepare頻度を間引いたりするものではない。

## Lua関数ハンドル

従来はcallbackごとに `Lua::registry_value` から `Function` を取り出していた。
mlua 0.10.5のこの経路は、VMロック、registry key照合、Lua registry参照、
一時Functionハンドルの作成・破棄を伴う。
登録時に取得済みのFunctionをruntimeが保持し、毎回そのハンドルを直接呼ぶ形に変更した。
Function自体がLuaの参照を保持するため、closureとupvalueは引き続き生存する。

Autoの推論判定やCompatの実行頻度は変えない。runtime専用VM、借用stateのscope、
callback単位/フレーム単位の命令数制限、失敗時のfallbackも維持する。
既存テストでmutable closure、nested scope、panic後の解放、無限ループ制限、
同じplayback時刻でのフレームbudget復帰を検証した。

## 計測と検証

比較元はIR修正まで含む `52d563db`。最適化前後のexeとCPU probeを別々に保存し、
本体・probeの各比較で同じ入力と設定を使う。
測定用設定、ログ、バイナリのSHA-256はローカルの
`.local/performance/2026-09-25-litone-optimization/` に保存する。

### CPU描画plan

LITONE9の未改変Select/Play7を使用。300フレームのwarmup後に3,000フレームを計測し、
各モード・各ケースで前後を交互に3回ずつ実行した。以下は各試行の平均時間の中央値。
動画decode、GPU、DB、アプリのsnapshot生成は含まない。
Playは256 tap notesの合成snapshot、Select scrollはprobeによるスクロール状態。

| Luaモード | ケース | 前 µs | 後 µs | 短縮率 |
|---|---|---:|---:|---:|
| Auto | Select静止 | 242.65 | 233.01 | 4.0% |
| Auto | Selectスクロール | 242.58 | 231.56 | 4.5% |
| Auto | Play | 171.41 | 151.85 | 11.4% |
| Compat | Select静止 | 255.14 | 243.34 | 4.6% |
| Compat | Selectスクロール | 249.79 | 235.70 | 5.6% |
| Compat | Play | 141.45 | 131.42 | 7.1% |

AutoとCompatは別の時間帯に計測したため、この表からモード間の優劣は判定しない。
試行間の揺れがあり、改善率がそのままアプリFPSへ反映されるわけではない。
数値キャッシュだけを実装した段階の別比較では、短縮率は約1～4%だった。

描画命令数は前後・両モードとも、Select平均326.136、Play 464で一致。
LITONE9 Selectの4代表フレーム（0/299/300/347）を比較し、
画面右下の実時刻表示に使うatlas UVだけを除いて描画planが一致した。
座標・色・texture・順序・命令数・それ以外のUVは比較対象に含めた。
Auto/Compat間も一致し、今回の「0」3個の差が実スキンでも解消している。

### 4K surface

DX12、RTX 5090、3840×2160、BorderlessFullscreen、VSyncOff（実効Immediate）、
最大待ちフレーム数1、FPS上限なし、Lua Autoで測定。
Selectは空ライブラリ・標準の動画2本、Playは音声/BGAなしの固定譜面をseed 42でautoplay。
前→後→後→前の順で各30,000フレームを実行し、
開始3～11秒に完全に含まれる120フレーム集計区間を比較する。
起点はSelectが最初のprofile集計時刻、Playがgameplay thread開始時刻。
FPSは描画フレーム数とログ時刻から算出し、GPU timestampによる計測ではない。

| ケース | 前 FPS（2試行） | 後 FPS（2試行） | 前 平均FPS | 後 平均FPS | 変化 |
|---|---|---|---:|---:|---:|
| Select静止 | 1,235 / 1,256 | 1,311 / 1,370 | 1,245 | 1,341 | +7.6% |
| Play | 1,568 / 1,579 | 1,492 / 1,584 | 1,573 | 1,538 | -2.2% |

同じ試行のCPU側計測区間の平均（ms）:

| ケース | plan 前→後 | total redraw 前→後 | surface取得 前→後 | present 前→後 |
|---|---|---|---|---|
| Select静止 | 0.2020 → 0.1807 | 0.7499 → 0.7057 | 0.0462 → 0.0394 | 0.1438 → 0.1394 |
| Play | 0.1766 → 0.1644 | 0.5972 → 0.6022 | 0.0304 → 0.0390 | 0.1359 → 0.1400 |

SelectはCPU planが約10.5%短縮し、両試行でFPSも上がった。
PlayはCPU planが約6.9%短縮したものの、surface取得/presentの増加に打ち消され、
この測定ではFPS改善を確認できなかった。2回の比較だけで差の原因や安定した効果は断定しない。
4,000～5,000 FPSへの到達を示す結果ではない。

全試行でアプリのERRORログは0件。
空ライブラリのSelectでは既存のLua callback fallback警告が各10件あり、前後で同数。
これらの警告が解消したという意味での警告ゼロではなく、Clippyの警告ゼロを確認した。

### 次の候補

今回の変更でGPUへの転送・encode・submit・presentの処理自体は変えていない。
Playの最適化後でもupload 0.0490ms、encode 0.0636ms、queue 0.0705ms、
surface取得 0.0390ms、present 0.1400msを要した。
次はこの経路を優先し、繰り返し描くgeometryの転送削減、queue/Presentと待ちフレーム数、
DX12/Vulkanの差を、同じ描画内容・遅延条件で比較する価値がある。
今回のCPU側時間だけではGPUのfill rateやshader実行時間は判定できない。

### 再現と検証

ローカルの `bench.py` は `BMZ_BENCH_LUA=auto` / `compat` を指定してCPU計測を実行する。
`verify.py` が描画planの前後・モード間比較、`surface.ps1` が本体計測、
`analyze.py` が3～11秒の集計を担当する。
前後のexe/probeは `bin/before` / `bin/after`、数値キャッシュ単独版は `bin/number-only`。
`binary-sha256.json`、`build-head.txt`、`lua-handles.patch` に測定バイナリの出自を保存した。

最終検証はworkspace test（3,435 passed / 9 ignored）、workspace check、
全targetの厳格Clippy (`-D warnings`)、fmt/diff check。
キャッシュには未キャッシュ経路との比較テストを設け、数値・符号・MIN/MAX、
座標・寸法・色・alpha、source差し替え/欠損、animation位相を検証した。
既存のstateful callback順序テストは同じ時刻を繰り返し、キャッシュhit時も評価されることを確認する。
