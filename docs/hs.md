# BMZ Hispeed Notes

BMZ のハイスピードは、基準方式の Normal / Classic / LR2 と、表示時間を基準にする
Floating を組み合わせて扱う。HS-FIX はこれとは別に BPM 基準を選択する。

## 設定

設定画面の `HS CONFIG` には次の6種類がある。

| 設定 | 基準方式 | Floating の扱い |
| --- | --- | --- |
| `NORMAL` | Normal | 無効 |
| `CLASSIC` | Classic | 無効 |
| `FLOATING` | Classic | 常時有効 |
| `NORMAL+FLOATING` | Normal | プレイ中に切替可能 |
| `CLASSIC+FLOATING` | Classic | プレイ中に切替可能 |
| `LR2` | LR2 | 無効 |

既定値は `CLASSIC+FLOATING`。HS設定を保存していない profile もこの設定として読み込み、
直接倍率と Floating を使う。

profile では、基準方式を `base_hispeed`、Floating の扱いを `floating_policy`、
Normal の段階を `normal_hispeed_level` に保存する。直接倍率は `classic_hispeed`、
Floating の目標緑数字は `floating_target_green` に保存する。旧名の `hispeed`、
`target_green_number`、`hispeed_step_nhs`、`hispeed_step_fhs` は読込時の別名として
受け付ける。

## Classic

Classic は `classic_hispeed` の倍率をそのまま使い、HS-FIXでは変換しない。
倍率の範囲は `0.01..=20.0`。操作刻みは `classic_hispeed_step` で、既定値は0.25、
設定範囲は `0.05..=1.00`。

## LR2

LR2 は Classic と同じ直接倍率で、`classic_hispeed` と `classic_hispeed_step` を
CLASSICと共有する。Floatingは使えない。profile の `base_hispeed` は `Lr2`、
`floating_policy` は `Disabled` になる。

HS-FIX が `OFF` のときは Classic と同じ。HS-FIX が `OFF` 以外では LR2 の HS-FIX と
同じく、`classic_hispeed` を BPM150 での倍率として扱う。HS-FIXの基準BPMで
ノーツが流れる速さは、BPM150・同じHS値と一致する。

```text
effective_hispeed = classic_hispeed * 150 / (hsfix_base_bpm * playback_rate)
```

- HS操作、設定UIでの変更、profileへの保存、skinのHS表示
  （`NUMBER_HISPEED` 310 / 311、LR2 HS）は `classic_hispeed` の値で行う。
  たとえばMAX BPM 300の譜面でHS 3.00は実効倍率1.50で、表示と保存は3.00のまま。
- `NUMBER_DURATION` 等の表示時間と緑数字は実効倍率から求める。
- 基準BPMは次節「HS-FIX」の表と同じ定義を使う。LR2のMAX/MIN BPMはノーツのある
  BPMだけで決まるが、BMZは初期BPMとBPM変化イベント全体で決める。
- プレイ中に CLASSIC と LR2 を切り替えた場合は、表示中のHS値を保ったまま実効倍率を
  求め直す。CLASSIC と LR2 でHS値を共有するため、同じ値でもHS-FIX使用時の速さは変わる。
- skin の基準方式 (1900 / 1916) は Classic と同じ `0` / `CHS` で、`1918` だけが `5=LR2`。

参照: OpenLR2 `LR2_bmsload.cpp` の `speedmultiplier = 150 / fixBPM`。

## Normal

Normal は1～20の段階を持つ。各段階はレーンカバーとLIFTを除外した全レーンの
目標緑数字に対応する。

| 段階 | 緑数字 | 段階 | 緑数字 | 段階 | 緑数字 | 段階 | 緑数字 |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 1200 | 6 | 600 | 11 | 440 | 16 | 340 |
| 2 | 1000 | 7 | 550 | 12 | 420 | 17 | 320 |
| 3 | 800 | 8 | 500 | 13 | 400 | 18 | 300 |
| 4 | 700 | 9 | 480 | 14 | 380 | 19 | 280 |
| 5 | 650 | 10 | 460 | 15 | 360 | 20 | 260 |

段階を変更すると、その時点の基準BPMとSCROLL/SPEED倍率から実際のHS倍率を
再計算する。SUDDEN+、LIFT、HIDDEN+の量はNormalの計算に含めない。

Floating から Normal へ戻すときは、現在のHS倍率から全レーン緑数字を求め、最も
近い段階を選ぶ。同距離は速い側、つまり小さい緑数字を選ぶ。ただし700～500の
50刻み区間では、速い側の緑数字に30を加えた位置を境界にする。たとえば580は550、
581は600へ変換する。

## Floating

Floating は `floating_target_green` を保つように、BPM、SCROLL/SPEED倍率、
SUDDEN+、LIFTからHS倍率を逆算する。緑数字の範囲は `1..=6000`、既定値は300。
HIDDEN+は可視レーン長を変えないため計算に含めない。

HS倍率の直接操作は `floating_hispeed_step` 刻みで行う。既定値は0.50、設定範囲は
`0.05..=1.00`。緑数字、SUDDEN+、LIFT、またはカバー表示を操作して再計算が起きる
までは、直接操作した倍率を維持する。

切替可能な設定で基準方式からFloatingへ入るときは、現在の見た目から緑数字を取得
して `floating_target_green` にする。FloatingからClassicへ戻る場合は現在の倍率を
維持する。FloatingからNormalへ戻る場合は前節の規則でNormal段階へ変換する。

LIFTとHIDDEN+が両方有効な場合、`E1 hold + E2` は両者の操作対象切替を優先する。
それ以外では、Floatingが「切替可能」の設定でのみ基準方式との切替を行う。

## 計算式

BMZ の表示時間計算は次の式を使う。

```text
visible = 1.0 - lane_cover - lift
duration_ms = 240000 / bpm / hispeed / scroll_multiplier * visible
green_number = round(duration_ms * 0.6)
```

Floating の逆算は次のとおり。

```text
hispeed = 240000 * visible * 0.6
          / (floating_target_green * bpm * scroll_multiplier)
```

Normal も同じ逆算式を使うが、`visible = 1.0`、目標緑数字は段階表の値とする。
計算後の倍率は `0.01..=20.0` に収める。現在位置のSCROLL/SPEED倍率が正でない
場合は譜面先頭の正の倍率、それも利用できなければ1.0を使う。

skin の `NUMBER_DURATION` (312) には導出した表示時間、
`NUMBER_DURATION_GREEN` (313) には緑数字を渡す。CONSTANT は導出した表示時間を
表示境界に使い、`constant_fade_ms` は `-1000..=1000ms` の範囲で境界前後を
フェードさせる。Practice の区間プレビューと区間プレイではCONSTANTを無効にする。

選曲画面の `NUMBER_DURATION` と `NUMBER_DURATION_GREEN` は、選択中のキーモードで
有効なSUDDEN+とHIDDEN+を上下のカバーとして扱い、両者の間でノーツが見える割合を
反映する。無効なカバーの保存値は無視し、カバーが重なる場合は表示時間を0に収める。
FloatingではSUDDEN+とLIFTを反映済みの目標緑数字を基準にし、HIDDEN+が隠す範囲だけを
残りの可視レーンに対する相対比で反映する。

## HS-FIX

HS-FIX の選択肢とBPM基準は次のとおり。

| option | BPM basis |
| --- | --- |
| `OFF` | 初期BPM |
| `START BPM` | 初期BPM |
| `MAX BPM` | 初期BPMとBPM変化イベントの最大BPM |
| `MAIN BPM` | ノート数が最も多いBPM |
| `MIN BPM` | 初期BPMとBPM変化イベントの最小BPM |

`FLOATING` はHS-FIXにかかわらずFloatingで開始する。Floating無効の `NORMAL`、`CLASSIC`、
`LR2` は常に基準方式で開始する。切替可能な2設定は、HS-FIXが `OFF` なら
基準方式、その他ならFloatingで開始する。

Floating の再計算は、READY前と曲タイマー開始前には選択したHS-FIXの基準BPM、
曲開始後には現在BPMを使う。Normalの初期倍率は開始時の基準BPM、段階操作時は
その時点のBPMを使う。LR2は基準BPMから求めた実効倍率をプレイ中に固定し、
BPM変化では再計算しない（前述のLR2を参照）。

設定UIと選曲中の巡回順は次のとおり。

```text
OFF -> START BPM -> MAX BPM -> MAIN BPM -> MIN BPM -> OFF
```

play skin の `event_index(55)` は `0=OFF`, `1=START`, `2=MAX`, `3=MAIN`,
`4=MIN`。

## 操作

主な操作は次のとおり。詳細は `docs/controls.md` を参照。

| 操作 | 動作 |
| --- | --- |
| `Left` / `Right` | Normalは段階を変更、Classic/Floatingは設定刻みで倍率を変更 |
| `Up` / `Down` | SUDDEN+表示中はSUDDEN+、非表示中は有効なLIFT/HIDDEN+を変更 |
| `E1 hold + E2` | LIFT/HIDDEN+両方有効時は操作対象を切替。それ以外は許可されたFloating切替 |
| `E1 hold + 鍵盤` | Left/Rightと同じ方式で速度を変更 |
| `E2 hold + 鍵盤` | Floatingが利用可能なら緑数字を変更 |
| `E2 hold + Scratch Up/Down` | Floatingが利用可能なら緑数字を変更 |
| `E1 double press` | SUDDEN+が有効な場合だけ表示を切替 |

HIDDEN+を操作対象にした場合は、カーソルキーと `E1 hold + Scratch Up/Down` の
どちらでもUpで量を増やし、Downで減らす。

SUDDEN+が無効または非表示でLIFT/HIDDEN+も無効な場合、カバー操作は仮想的な
SUDDEN+=0操作として扱う。HS Auto AdjustがOFFならデジタル入力は現在方式の速度
操作、アナログ入力は1 tickあたり倍率0.01の操作になる。HS Auto AdjustがONの
Floatingでは、SUDDEN+=0、LIFT=0として再計算だけを行う。無効なカバー値は変更・
保存しない。

Floating無効の設定では緑数字操作を受け付けない。Floating固定の設定では
切替操作を受け付けない。Courseの `NoSpeed` 制約中は、速度、緑数字、レーンカバーの
操作をすべて無効にする。

## Skin Refs

| ref | kind | meaning |
| ---: | --- | --- |
| 1900 | number / event_index / text | numberは `0=基準方式`, `1=Floating`。textは `CHS` / `NHS` / `FHS` |
| 1901 | number / option | Floating active flag。`0=OFF`, `1=ON` |
| 1902 | number | Floating中は固定目標緑数字、それ以外は現在緑数字 |
| 1916 | number / event_index / text | 基準方式。`0=Classic`, `1=Normal`。textは `CHS` / `NHS` |
| 1917 | number / event_index | Normal段階 `1..=20` |
| 1918 | number / event_index / text | 6設定のindexと設定名 |

ref 1918は `0=NORMAL`, `1=CLASSIC`, `2=FLOATING`,
`3=NORMAL+FLOATING`, `4=CLASSIC+FLOATING`, `5=LR2`。

E1/E2操作に連動する共通HSパネルには、押下中option `1928`、共通press / hold timer
`19008`、共通release timer `19009`を使える。詳細は`docs/skin.md`を参照。

## 実装入口

- Configと段階表: `crates/bmz-player/src/config/profile_config/`, `config/play.rs`
- Session初期化: `crates/bmz-player/src/screens/play_session/`
- プレイ中操作と再計算: `crates/bmz-player/src/app/play_support/`
- 表示時間とSCROLL/SPEED: `crates/bmz-player/src/screens/play_snapshot/`
- Skin refs: `crates/bmz-render/src/skin/`
