# BMZ Hispeed Notes

BMZ のハイスピード周りは、現状では次の 2 軸に分かれる。

- HS MODE: `NHS` / `FHS`
- HS-FIX: `OFF` / `START BPM` / `MAX BPM` / `MAIN BPM` / `MIN BPM`

`HS Auto Adjust` は独立した profile 設定としては持たない。BMZ では FHS 中の
自動再計算を、beatoraja の `HI-SPEED FIX Auto Adjust` ON 相当として扱う。

## Terms

### NHS

Normal Hispeed。設定画面では `NORMAL`、BMZ skin ref では `NHS` と表示する。

NHS は `profile.lane.hispeed` の倍率をそのまま使う。倍率は `0.5..=10.0`、
操作は `profile.lane.hispeed_step_nhs` 刻み。既定値は 0.25 で、設定可能範囲は
0.05..=1.00。

### FHS

Floating Hispeed。設定画面では `FLOATING`、BMZ skin ref では `FHS` と表示する。

FHS は `profile.lane.target_green_number` を固定したまま、BPM、SCROLL/SPEED 倍率、
レーンカバー量に応じて `session.hispeed` を逆算する。緑数字の範囲は `1..=999`、
既定値は `300`。

FHS で HS 倍率を直接変更した場合は、現在の見た目から緑数字を再計算して
`target_green_number` を更新する。緑数字やレーンカバーを変更した場合は、
`target_green_number` を維持するように HS 倍率を再計算する。

FHS の HS 変更は `profile.lane.hispeed_step_fhs` 刻みで行う。既定値は 0.50 で、
設定可能範囲は 0.05..=1.00。設定画面の「NHS HS変更刻み」 / 「FHS HS変更刻み」
または `profile.toml` の `[lane]` で変更できる。

## Formula

BMZ の表示時間計算は beatoraja の係数 `240000` を使う。

```text
visible = 1.0 - lane_cover - lift
duration_ms = 240000 / bpm / hispeed / scroll_multiplier * visible
green_number = round(duration_ms * 0.6)
```

FHS ではこの式を逆向きに使う。

```text
hispeed = 240000 * visible * 0.6 / (target_green_number * bpm * scroll_multiplier)
```

計算後の HS 倍率は `0.5..=10.0` に clamp する。`visible` はレーンカバーと
LIFT の合計を考慮した可視レーン率で、レーンカバー非表示中は `lane_cover = 0`
として扱う。

`scroll_multiplier` は現在 tick の SCROLL factor と SPEED factor の積。SCROLL /
SPEED がある譜面では、同じ BPM と緑数字でも HS 逆算結果が変わる。

## HS-FIX

HS-FIX の選択肢は次の通り。

| option | BPM basis |
| --- | --- |
| `OFF` | 初期 BPM。FHS の曲開始前計算では `START BPM` と同じ扱い |
| `START BPM` | 初期 BPM |
| `MAX BPM` | 初期 BPM と BPM 変化イベントの最大 BPM |
| `MAIN BPM` | ノート数が最も多い BPM |
| `MIN BPM` | 初期 BPM と BPM 変化イベントの最小 BPM |

プレイ画面の開始時は HS-FIX が HS MODE を決める。`OFF` なら NHS、`START BPM` /
`MAX BPM` / `MAIN BPM` / `MIN BPM` なら FHS で開始する。プロファイルに保存された
HS MODE は開始時の選択よりも、プレイ中の操作後に保存する状態として扱う。

`MAIN BPM` は、地雷を除いた通常ノートとロングノート始点を BPM ごとに数え、
最も count が大きい BPM を選ぶ。ロングノートの始点が通常ノート一覧と重複する場合は
二重に数えない。

BMZ の設定 UI / 選曲中の巡回順は次の順番。

```text
OFF -> START BPM -> MAX BPM -> MAIN BPM -> MIN BPM -> OFF
```

この順番は beatoraja の `fixhispeed` / `event_index(55)` と同じ内部順に合わせる。
play skin へ渡す beatoraja 互換の `event_index(55)` は次の値を使う。

| index | meaning |
| ---: | --- |
| 0 | OFF |
| 1 | START BPM |
| 2 | MAX BPM |
| 3 | MAIN BPM |
| 4 | MIN BPM |

プレイ中 skin 用の表示 index はセッション開始時に profile の HS-FIX 設定から決まり、
プレイ中の手動操作では変更しない。

## FHS Recalculation

プレイセッション作成時、BMZ は HS-FIX から `session.hsfix_base_bpm` を決める。

FHS の BPM 基準は、曲開始前と曲開始後で異なる。

| state | BPM used by FHS recalculation |
| --- | --- |
| READY 前 / 曲タイマー開始前 | `session.hsfix_base_bpm` |
| 曲開始後 | 現在 BPM |

実装上は `audio_clock.running && now >= 0` を満たすと曲開始後として扱う。

これは beatoraja の `HI-SPEED FIX Auto Adjust` ON に寄せつつ、BMZ では READY 前後の
調整中だけ START / MIN / MAX / MAIN BPM の固定基準を保つためのルール。

FHS の自動再計算は主に次の操作で起きる。

- 緑数字を変更する
- レーンカバー / LIFT を変更する
- レーンカバー表示を OFF から ON に戻す
- NHS から FHS に切り替える

Course の `NoSpeed` 制約中は、HS 変更、緑数字変更、FHS 再計算を行わない。

## Play Controls

主なプレイ中操作は次の通り。詳細は `docs/controls.md` を参照。

| operation | behavior |
| --- | --- |
| `Left` / `Right` | HS 倍率を HS MODE ごとの設定刻みで下げる / 上げる (NHS 既定 0.25、FHS 既定 0.50) |
| `Up` / `Down` | レーンカバー表示中はカバー位置、非表示中は LIFT を調整 |
| `E1 hold + E2` | HS MODE を切り替える |
| `E1 hold + KEY...` | HS 倍率を変更する |
| `E2 hold + KEY...` | 緑数字を変更する |
| `E2 hold + Scratch Up/Down` | 緑数字を変更する |
| `E1 double press` | レーンカバー表示を切り替える |

## Skin Refs

BMZ 独自の HS MODE ref は `1900` 台を使う。詳細は `docs/skin.md` も参照。

| ref | kind | meaning |
| ---: | --- | --- |
| 1900 | number / event_index / text | `0=NHS`, `1=FHS`。text は `NHS` / `FHS` |
| 1901 | number / option | FHS active flag。`0=NHS`, `1=FHS` |
| 1902 | number | FHS 時は固定 target green、NHS 時は現在 green number |

beatoraja 互換の HS-FIX event は `event_index(55)`。値は `0=OFF`, `1=START`,
`2=MAX`, `3=MAIN`, `4=MIN`。

Rm-skin 系で使う adjusted hidden cover / BPM 比率表示は、現在は `MAX` と `MAIN` の
HS-FIX index に反応する。`OFF` / `START` / `MIN` では adjusted 値を出さない。

## Implementation Entry Points

- Config: `crates/bmz-player/src/config/profile_config.rs`
- HS-FIX select option: `crates/bmz-player/src/select_options.rs`
- Session initialization: `crates/bmz-player/src/screens/play_session.rs`
- Runtime controls and FHS recalculation: `crates/bmz-player/src/app.rs`
- Render snapshot duration / green number calculation: `crates/bmz-player/src/screens/play_snapshot.rs`
- Skin refs: `crates/bmz-render/src/skin.rs`
- Adjusted graph helpers: `crates/bmz-render/src/chart_graph.rs`
