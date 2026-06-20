# BMZ Rule Mode Notes

BMZ の `RuleMode` は、判定窓、判定ランク、ゲージ、コンボ、score / replay
の保存キーを切り替えるための設定である。profile の `[play].rule_mode` に保存し、
score DB / replay slot でも別条件として扱う。

現在の値:

- `Beatoraja`
- `Lr2Oraja`
- `Dx`

参照ソース:

- beatoraja: `.local/beatoraja/`
- LR2oraja Endless Dream: `.local/lr2oraja-endlessdream/`
- LR2oraja Endless Dream RIAN / DX MODE: `.local/lr2oraja-endlessdream-rian/`

## Mode Summary

| item | Beatoraja | Lr2Oraja | Dx |
| --- | --- | --- | --- |
| 主な参照 | beatoraja | LR2oraja Endless Dream | LR2oraja Endless Dream RIAN DX MODE |
| 判定 property | key mode 別 `JudgeProperty` | `JudgeProperty.LR2` | `JudgeProperty.IIDX` 固定 |
| ゲージ property | key mode 別 `GaugeProperty` | `GaugeProperty.LR2` | IIDX 用 gauge 定義へ差し替え |
| `#RANK` 未指定 | 100% | 75% (`#RANK 2` 相当) | 固定窓のため無視 |
| `#RANK 4` | 125% (`PMS` は 133%) | 75% (`#RANK 2` 相当) | 固定窓のため無視 |
| `#EXRANKxx` / chA0 | import はするが runtime では無視 | import はするが runtime では無視 | 固定窓のため無視 |
| MultiBad | 無効 | 有効 | 有効 |
| LN start late BAD 抑制 | 無効 | 有効 | 有効 |
| BAD 消費 | 消費 (`PMS` だけ非消費) | 消費 | 消費 |
| EmptyPoor combo break | 5K / 10K / 9K だけ break | 継続 | 継続 |
| score / replay 保存 | `rule_mode=Beatoraja` | `rule_mode=Lr2Oraja` | `rule_mode=Dx` |

DX MODE の IR 抑制は BMZ では実装しない方針。

## Judge Windows

### Beatoraja

key mode から beatoraja の player rule に対応する `JudgeProperty` を選ぶ。

- 5K / 10K: `FIVEKEYS`
- 7K / 14K: `SEVENKEYS`
- 9K: `PMS`
- 4K / 6K / 8K: beatoraja に対応 mode が無いため `SEVENKEYS` 相当

`#RANK` の倍率は、key mode から選ばれる beatoraja `JudgeWindowRule` に従う。
5K / 7K / 10K / 14K と BMZ 拡張の 4K / 6K / 8K は `NORMAL`、9K は `PMS`。

`NORMAL`:

| `#RANK` | percent |
| ---: | ---: |
| 0 | 25 |
| 1 | 50 |
| 2 | 75 |
| 3 | 100 |
| 4 | 125 |

`PMS`:

| `#RANK` | percent |
| ---: | ---: |
| 0 | 33 |
| 1 | 50 |
| 2 | 70 |
| 3 | 100 |
| 4 | 133 |

未指定は 100%、範囲外は各 rule の `#RANK 2` (`NORMAL=75%`, `PMS=70%`) に寄せる。

`#DEFEXRANK` は beatoraja `BMSPlayerRule.validate` と同じく、その rule の `#RANK 2`
を基準にする。たとえば `#DEFEXRANK 100` は `NORMAL=75%`, `PMS=70%`。
BMSON `judge_rank` は raw percent として扱い、0 以下は 100% にフォールバックする。

判定窓の倍率適用も beatoraja `JudgeWindowRule.create` に合わせる。`PMS` では
PGREAT / BAD / MISS が固定、GREAT / GOOD だけが rank で変化する。`NORMAL` でも
PGREAT / GREAT / GOOD が BAD を超えないようにし、狭い上位判定が広い下位判定を
逆転した場合は単調化する。

`PMS` は beatoraja `judgeVanish[Bad] = false` / `MissCondition.ONE` に従う。
BAD はスコア/ゲージへ反映するがノーツを消費しないため、同じノーツを後続の
GOOD 以上で再判定できる。BAD 後に見逃した場合は内部的にノーツを消費するが、
追加の POOR はスコア/ゲージへ入れない。

`#EXRANKxx` / chA0 は import 結果として `judge_rank_events` に残すが、
Beatoraja mode の runtime 判定窓には反映しない。互換上、ヘッダ側の判定ランクだけを使う。

### LR2oraja

LR2oraja mode は key mode に関係なく `JudgeProperty.LR2` 相当を使う。

通常ノート / scratch:

- PGREAT: `±21000us`
- GREAT: `±60000us`
- GOOD: `±120000us`
- BAD: `±200000us`
- EMPTY POOR FAST: `1000000us`
- EMPTY POOR SLOW: `0us`

LN end / long scratch end:

- PGREAT / GREAT / GOOD: `±120000us`
- BAD: `±200000us`
- EMPTY POOR: なし

`#RANK` は元祖 LR2 互換の fallback を使う。

| source | behavior |
| --- | --- |
| 未指定 | 75% (`#RANK 2`) |
| `#RANK 0` | 25% |
| `#RANK 1` | 50% |
| `#RANK 2` | 75% |
| `#RANK 3` | 100% |
| `#RANK 4` | 75% (`#RANK 2`) |
| 不正 / 範囲外 | 75% |

`#DEFEXRANK` は LR2oraja の `BMS_DEFEXRANK` と同じく NORMAL 75% を基準にする。
たとえば `#DEFEXRANK 100` は 75%、`#DEFEXRANK 125` は整数除算で 93%。

BMSON `judge_rank` は LR2oraja の `BMSON_JUDGERANK` と同じく raw percent として扱う。
0 以下は 100% にフォールバックする。

`#EXRANKxx` / chA0 は import しても runtime では無視する。
LR2oraja の jbms-parser 経路では chA0 が runtime の判定ランク変更として使われないため。

### DX

DX MODE は LR2oraja Endless Dream RIAN の `dxMode` を参照する。
判定は `JudgeProperty.IIDX` に固定し、custom judge rate は無効化する。

通常ノート / scratch:

- PGREAT: `±16666us`
- GREAT: `±33333us`
- GOOD: `±116666us`
- BAD: `±200000us`
- EMPTY POOR FAST: `1000000us`
- EMPTY POOR SLOW: `200000us`

LN end / long scratch end:

- PGREAT / GREAT / GOOD: `±116666us`
- BAD: `±200000us`
- EMPTY POOR: なし

`#RANK`, `#DEFEXRANK`, `#EXRANKxx` / chA0 は固定 IIDX 窓へ反映しない。
BMZ の実装でも DX mode の `judge_percent_at_time` は常に 100% を返し、
`judge_windows_for_rule_mode` は倍率を無視する。

## Judge Algorithm

判定対象ノーツの選択アルゴリズムは rule mode とは別設定で、profile の
`[judge].judge_algorithm` から選ぶ。

現在の enum は LR2oraja / beatoraja の並びに合わせる。

- `Combo`
- `Duration`
- `Lowest`
- `Score`

`Duration` は時間差最小、`Lowest` は先に見つかったノーツ優先、`Combo` は combo 継続寄り、
`Score` は score 寄りの比較を行う。BMZ ではすべての rule mode で同じ設定値を使う。

## MultiBad

LR2oraja / DX mode では、押下時に選ばれたノーツ周辺の BAD 範囲ノーツへ追加 BAD を出す。
これは LR2oraja Endless Dream の `MultiBadCollector` 相当である。

Beatoraja mode では MultiBad を出さない。

## Gauge

### Beatoraja

`GaugeProperty` は key mode から選ぶ。

- 5K / 10K: `FiveKeys`
- 7K / 14K: `SevenKeys`
- 9K: `Pms`
- 4K / 6K / 8K: `SevenKeys`
- course constraint の gauge 指定がある場合はそれを優先

`#TOTAL` が正値ならそれを使う。未指定または 0 以下なら beatoraja 互換の既定 TOTAL:

```text
max(260.0, 7.605 * total_notes / (0.01 * total_notes + 6.5))
```

### LR2oraja

`GaugeProperty.LR2` 相当を使う。

`#TOTAL` が正値ならそれを使う。未指定または 0 以下なら LR2oraja Endless Dream の既定 TOTAL:

```text
160.0 + (total_notes + clamp(total_notes - 400, 0, 200)) * 0.16
```

HARD / CLASS 系の guts や death border は `crates/bmz-gameplay/src/gauge.rs` の
`lr2oraja_gauge_definitions()` を正とする。

### DX

DX MODE は key mode や course gauge property に関係なく IIDX 用 gauge 定義を使う。

主な値:

| gauge | init | border | PG/GR/GD | BAD/POOR/EMPTY POOR |
| --- | ---: | ---: | --- | --- |
| AssistEasy | 22 | 60 | `IIDX total * 1.0/1.0/0.5` | `-1.6/-4.8/-1.6` |
| Easy | 22 | 80 | `IIDX total * 1.0/1.0/0.5` | `-1.6/-4.8/-1.6` |
| Normal | 22 | 80 | `IIDX total * 1.0/1.0/0.5` | `-2.0/-6.0/-2.0` |
| Hard | 100 | 0 | `0.16/0.16/0.0` | `-4.5/-9.0/-4.5` |
| ExHard | 100 | 0 | `0.16/0.16/0.0` | `-9.0/-18.0/-9.0` |
| Hazard | 100 | 0 | `0.16/0.16/0.0` | `-100.0/-100.0/-9.0` |
| Class | 100 | 0 | `0.16/0.16/0.04` | `-1.5/-2.5/-1.5` |
| ExClass | 100 | 0 | `0.16/0.16/0.04` | `-3.0/-5.0/-3.0` |
| ExHardClass | 100 | 0 | `0.16/0.16/0.04` | `-6.0/-10.0/-6.0` |

AssistEasy / Easy / Normal の回復量は chart `#TOTAL` ではなく IIDX 用 TOTAL で計算する。

```text
iidx_total = max(260.0, 7.605 * total_notes / (0.01 * total_notes + 6.5))
recovery = base * iidx_total / total_notes
```

Hard / Class は 30% 未満でダメージを 0.5 倍にする。

## Score And Replay Separation

`RuleMode` は score DB と replay slot のキーに含める。
同じ譜面でも Beatoraja / LR2oraja / DX は別スコアとして保存する。

保存箇所:

- `score_history.rule_mode`
- `score_best.rule_mode`
- `replay_slots.rule_mode`
- replay slot filename suffix

`RuleMode::Beatoraja` は後方互換のため replay slot filename suffix を省略する。
`RuleMode::Lr2Oraja` / `RuleMode::Dx` は suffix を付ける。

## Known Notes

- DX MODE の IR 抑制は BMZ では実装しない。
- Beatoraja mode の `#DEFEXRANK` 実機挙動は、必要になった時点で改めて
  beatoraja 実行結果と突き合わせる。LR2oraja mode は元種別を保持して実装済み。
- LR2 csvskin は未対応。現在の rule mode は gameplay / score / gauge の切替だけを対象にする。

## Implementation Pointers

- rule mode enum: `crates/bmz-gameplay/src/rule.rs`
- judge windows / rank conversion: `crates/bmz-gameplay/src/judge/window.rs`
- judge engine / MultiBad: `crates/bmz-gameplay/src/judge/engine.rs`
- judge algorithm enum: `crates/bmz-gameplay/src/judge/model.rs`
- score / combo policy: `crates/bmz-gameplay/src/score.rs`
- gauge definitions / TOTAL: `crates/bmz-gameplay/src/gauge.rs`
- play session wiring: `crates/bmz-player/src/screens/play_session.rs`
- profile UI / settings: `crates/bmz-player/src/ui.rs`, `crates/bmz-player/src/config/settings_registry.rs`
- score DB / replay persistence: `crates/bmz-player/src/storage/score_db.rs`,
  `crates/bmz-player/src/storage/replay.rs`
