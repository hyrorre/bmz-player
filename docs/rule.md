# ルールモード

BMZの`RuleMode`は、判定窓、判定ランク、ゲージ、コンボ、score / replay
の保存キーを切り替えるための設定です。profileの`[play].rule_mode`に保存し、
score DB / replay slotでも別条件として扱います。

現在の値:

- `Beatoraja`
- `Lr2Oraja`
- `Dx`

参照ソース:

- beatoraja: `.local/beatoraja/`
- LR2oraja Endless Dream: `.local/lr2oraja-endlessdream/`

DX MODEも現在のLR2oraja Endless Dream本体に統合されているため、同じ参照ツリーを使います。

## モードの概要

| item | Beatoraja | Lr2Oraja | Dx |
| --- | --- | --- | --- |
| 主な参照 | beatoraja | LR2oraja Endless Dream | LR2oraja Endless Dream DX MODE |
| 判定property | key mode別`JudgeProperty` | `JudgeProperty.LR2` | 9Kは`JudgeProperty.POP`、それ以外は`IIDX` |
| ゲージproperty | key mode別`GaugeProperty` | `GaugeProperty.LR2` | 9KはPOP用、それ以外はIIDX用gauge |
| `#RANK`未指定 | 100% | 75% (`#RANK 2`相当) | 固定窓のため無視 |
| `#RANK 4` | 125% (`PMS`は133%) | 75% (`#RANK 2`相当) | 固定窓のため無視 |
| `#EXRANKxx` / chA0 | importはするがruntimeでは無視 | importはするがruntimeでは無視 | 固定窓のため無視 |
| MultiBad | 無効 | 有効 | 有効 |
| LN start late BAD抑制 | 無効 | 有効 | 有効 |
| BAD消費 | 消費 (`PMS`だけ非消費) | 消費 | 9Kは非消費、それ以外は消費 |
| EmptyPoor combo break | 5K / 10K / 9Kだけbreak | 継続 | 9Kはbreak、それ以外は継続 |
| CN/HCN早離し猶予 | 9Kは200ms、それ以外は0 | 0 | 9Kは200ms、それ以外は0 |
| score / replay保存 | `rule_mode=Beatoraja` | `rule_mode=Lr2Oraja` | `rule_mode=Dx` |

DX MODE固有のIR抑制はBMZ公式IR / rianIRには適用しません。
BMS-IRは対応rule modeの制約によりDXを送信対象にしません。

## 判定幅

### リプレイ視聴速度とPractice

リプレイの入力時刻と判定窓は譜面時間を基準にし、視聴速度による判定窓の
拡大・縮小は行いません。本人・対戦相手のリプレイ採点は譜面時間の1ms間隔で
入力、見逃し、Mine、HCN継続ゲージを進めます。同じリプレイの判定、EX、BP、
コンボ、ゲージは、25〜300%の視聴速度、途中の速度変更、描画間隔に依存しません。
入力に記録された時刻自体は丸めず、そのままノーツ時刻と比較します。

Practiceなどの実入力は、譜面時間の判定窓を再生速度倍に補正して
実時間の判定幅を保ちます。表示オフセットは採点には適用しません。

### タイミンググラフの判定帯

スキンのTimingVisualizer / TimingDistributionGraphは、プレイ中に実際に使う
通常ノートの判定窓を表示します。`#RANK`だけから再推定せず、DXの7K / 9K固定窓、
LR2orajaのランク変換、custom judge・コース制約を適用した窓に追従します。
グラフの判定差分と窓はともに譜面時間で扱うため、Practiceは速度補正後の窓、
replayは視聴速度に依存しない窓を使います。`#EXRANK` / chA0の非適用も各ルールと同じです。

Resultは終了時点の最終判定窓を保持し、終了後のプロフィール設定から再計算しません。
コース全体のグラフは最後にプレイした曲の最終窓を使います。履歴の各判定時点や各曲ごとに
背景の判定帯を切り替える表示ではありません。セッション未生成のplaceholder / debug表示は
rule modeとkey mode・判定ランクから補完し、不明なrule modeはbeatoraja相当とします。
この表示情報は実行時だけ保持し、スコア・replayの保存データや採点処理には使いません。

### Beatoraja

key modeからbeatorajaのplayer ruleに対応する`JudgeProperty`を選びます。

- 5K / 10K: `FIVEKEYS`
- 7K / 14K: `SEVENKEYS`
- 9K: `PMS`
- 4K / 6K / 8K: beatorajaに対応modeが無いため`SEVENKEYS`相当

`#RANK`の倍率は、key modeから選ばれるbeatoraja `JudgeWindowRule`に従います。
5K / 7K / 10K / 14KとBMZ拡張の4K / 6K / 8Kは`NORMAL`、9Kは`PMS`です。

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

未指定は100%、範囲外は各ruleの`#RANK 2` (`NORMAL=75%`, `PMS=70%`)に寄せます。

`#DEFEXRANK`はbeatoraja `BMSPlayerRule.validate`と同じく、そのruleの`#RANK 2`
を基準にします。たとえば`#DEFEXRANK 100`は`NORMAL=75%`, `PMS=70%`です。
BMSON `judge_rank`はraw percentとして扱い、0以下は100%にフォールバックします。

判定窓の倍率適用もbeatoraja `JudgeWindowRule.create`に合わせます。`PMS`では
PGREAT / BAD / MISSが固定、GREAT / GOODだけがrankで変化します。`NORMAL`でも
PGREAT / GREAT / GOODがBADを超えないようにし、狭い上位判定が広い下位判定を
逆転した場合は単調化します。

`PMS`はbeatoraja `judgeVanish[Bad] = false` / `MissCondition.ONE`に従います。
BADはスコア/ゲージへ反映するがノーツを消費しないため、同じノーツを後続の
GOOD以上で再判定できます。BAD後に見逃した場合は内部的にノーツを消費するが、
追加のPOORはスコア/ゲージへ入れません。

任意の`play.play_keysound_on_miss`（既定OFF）は採点を変えず、見逃しPOOR確定時に
通常ノート・LN始端、押したまま見逃したCN/HCN終端のキー音を再生します。
CN/HCN始端を見逃すと始端・終端の2判定を集計するが、発音は始端だけとなります。
非消費BAD済みノートの見逃し、空POOR、早離しPOOR、地雷、不可視ノートに
追加の発音は行いません。通常の自動キー音ON時は重複発音を抑えます。

`#EXRANKxx` / chA0はimport結果として`judge_rank_events`に残すが、
Beatoraja modeのruntime判定窓には反映しません。互換上、ヘッダ側の判定ランクだけを使います。

### LR2oraja

LR2oraja modeはkey modeに関係なく`JudgeProperty.LR2`相当を使います。

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

`#RANK`は元祖LR2互換のfallbackを使います。

| source | behavior |
| --- | --- |
| 未指定 | 75% (`#RANK 2`) |
| `#RANK 0` | 25% |
| `#RANK 1` | 50% |
| `#RANK 2` | 75% |
| `#RANK 3` | 100% |
| `#RANK 4` | 75% (`#RANK 2`) |
| 不正 / 範囲外 | 75% |

`#DEFEXRANK`はLR2orajaの`BMS_DEFEXRANK`と同じくNORMAL 75%を基準にします。
たとえば`#DEFEXRANK 100`は75%、`#DEFEXRANK 125`は整数除算で93%です。

BMSON `judge_rank`はLR2orajaの`BMSON_JUDGERANK`と同じくraw percentとして扱います。
0以下は100%にフォールバックします。

`#EXRANKxx` / chA0はimportしてもruntimeでは無視します。
LR2orajaのjbms-parser経路ではchA0がruntimeの判定ランク変更として使われないためです。

### DX

DX MODEはLR2oraja Endless Dreamの`dxMode`を参照します。custom judge rateは
無効化し、9K以外は`JudgeProperty.IIDX`、9Kは`JudgeProperty.POP`に固定します。

9K以外の通常ノート / scratch:

- PGREAT: `±16666us`
- GREAT: `±33333us`
- GOOD: `±116666us`
- BAD: `±200000us`
- EMPTY POOR FAST: `1000000us`
- EMPTY POOR SLOW: `200000us`

9K以外のLN end / long scratch end:

- PGREAT / GREAT / GOOD: `±116666us`
- BAD: `±200000us`
- EMPTY POOR: なし

9Kの通常ノート:

- PGREAT: `±25000us`
- GREAT: `±50000us`
- GOOD: `±87500us`
- BAD: `±100000us`
- EMPTY POOR FAST: `500000us`
- EMPTY POOR SLOW: `112500us`

9KのCN / HCN end:

- PGREAT: `±120000us`
- GREAT: `±150000us`
- GOOD: `±217000us`
- BAD: `±283000us`
- EMPTY POOR: なし

9KではBADがノーツを消費せず、後続入力でGOOD以上へ再判定できます。
MultiBadで付いたBADも同じく非消費とします。EMPTY POORはコンボを切り、
`MissCondition.ONE`として1ノーツにつき1回まで扱います。

9KのCN / HCNをBAD / POOR相当の早さで離した場合は、200msの
release margin中だけ終端判定を保留します。その間に押し直せば早離しを取り消し、
押し直さなければ保留した判定を確定します。long scratchのmarginは0です。

`#RANK`, `#DEFEXRANK`, `#EXRANKxx` / chA0は固定IIDX / POP窓へ反映しません。
BMZの実装でもDX modeの`judge_percent_at_time`は常に100%を返し、
`judge_windows_for_rule_mode`は倍率を無視します。

## 判定アルゴリズム

判定対象ノーツの選択アルゴリズムはrule modeとは別設定で、profileの
`[judge].judge_algorithm`から選びます。

現在のenumはLR2oraja / beatorajaの並びに合わせます。

- `Combo`
- `Duration`
- `Lowest`
- `Score`

`Duration`は時間差最小、`Lowest`は先に見つかったノーツ優先、`Combo`はcombo継続寄り、
`Score`はscore寄りの比較を行います。BMZではすべてのrule modeで同じ設定値を使います。

## MultiBad

LR2oraja / DX modeでは、押下時に選ばれたノーツ周辺のBAD範囲ノーツへ追加BADを出します。
これはLR2oraja Endless Dreamの`MultiBadCollector`相当です。

Beatoraja modeではMultiBadを出しません。

## ゲージ

### Beatoraja

`GaugeProperty`はkey modeから選びます。

- 5K / 10K: `FiveKeys`
- 7K / 14K: `SevenKeys`
- 9K: `Pms`
- 4K / 6K / 8K: `SevenKeys`
- course constraintのgauge指定がある場合はそれを優先

`#TOTAL`が正値ならそれを使います。未指定または0以下ならbeatoraja互換の既定TOTAL:

```text
max(260.0, 7.605 * total_notes / (0.01 * total_notes + 6.5))
```

### LR2oraja

`GaugeProperty.LR2`相当を使います。

`#TOTAL`が正値ならそれを使います。未指定または0以下ならLR2oraja Endless Dreamの既定TOTAL:

```text
160.0 + (total_notes + clamp(total_notes - 400, 0, 200)) * 0.16
```

HARD / CLASS系のgutsやdeath borderは`crates/bmz-gameplay/src/gauge.rs`の
`lr2oraja_gauge_definitions()`を正とします。

### DX

DX MODEはcourse gauge propertyに関係なく、9KではPOP用gauge定義、
それ以外ではIIDX用gauge定義を使います。

9K以外の主な値:

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

AssistEasy / Easy / Normalの回復量はchart `#TOTAL`ではなくIIDX用TOTALで計算します。

```text
iidx_total = max(260.0, 7.605 * total_notes / (0.01 * total_notes + 6.5))
recovery = base * iidx_total / total_notes
```

Hard / Classは30%未満でダメージを0.5倍にします。

9KのPOP gauge:

| gauge | min / max | init | border | PG/GR/GD | BAD/POOR/EMPTY POOR |
| --- | --- | ---: | ---: | --- | --- |
| AssistEasy | 2 / 120 | 30 | 65 | `POP total * 1.2/1.2/0.6` | `-0.937/-2.929/-2.929` |
| Easy | 2 / 120 | 30 | 85 | `POP total * 1.2/1.2/0.6` | `-0.937/-2.929/-2.929` |
| Normal | 2 / 120 | 30 | 85 | `POP total * 1.2/1.2/0.6` | `-1.992/-5.976/-5.976` |
| Hard | 2 / 120 | 30 | 85 | `POP total * 1.2/1.2/0.6` | `-3.984/-11.952/-11.952` |
| ExHard | 2 / 120 | 30 | 85 | `POP total * 1.2/1.2/0.6` | `-7.968/-23.906/-23.906` |
| Hazard | 0 / 100 | 100 | 0 | `0.15/0.06/0` | `-100/-100/-100` |
| Class | 0 / 100 | 100 | 0 | `0.15/0.15/0.06` | `-1.5/-3/-3` |
| ExClass | 0 / 100 | 100 | 0 | `0.15/0.15/0.03` | `-3/-6/-6` |
| ExHardClass | 0 / 100 | 100 | 0 | `0.15/0.15/0` | `-5/-10/-10` |

AssistEasyからExHardまでの減少量は、LR2orajaED-rianの
`c980834957b03195880544ced49c9ee94890557b`に合わせます。
この補正は9K DXのBAD / POOR / EMPTY POORと、BADの半分を適用する
HCN早離し中の継続ダメージに反映されます。回復量、下限・上限・クリア境界、
Hazard / Class系、9K以外のDXと他のRuleModeには適用しません。

AssistEasyからExHardまでの正の回復量には次のPOP TOTALを使います。
除算と`floor`はLR2oraja Endless Dreamと同じ整数切り捨てを含みます。

```text
total_notes == 0: recovery = 0
total_notes <= 3072:
  pop_total = min(300, floor(3072 / total_notes) * total_notes / 1024 * 100)
total_notes > 3072:
  pop_total = floor(0.097 * total_notes)
recovery = base * pop_total / total_notes
```

さらに9K DXで`total_notes >= 1537`の場合は、POP TOTAL適用前にGOODの
基礎回復量を2倍にします。Classは30%未満でダメージを0.5倍にします。

Practiceでも同じrule mode・判定用key modeのゲージ定義を使います。新規DX設定の初期値と
ゲージ種別切替時の初期値、編集上限をこの定義から取得し、保存済みの初期値は範囲内なら保持します。
9Kで120を設定した場合も、開始時に100へ一律に制限しません。GASでは各ゲージをそれぞれの
上限で制限するため、POPゲージの120とHazard / 段位系の100を同時に保持できます。
変換済み譜面を扱う開始経路では表示レーン数ではなく実セッションの判定用key modeに合わせます。
7→9の7KルールならIIDX、9KルールならPOPとなります。通常のPractice開始は原譜面のkey modeを使います。

Practiceのゲージカテゴリ・判定ランク・TOTALの保存値はDX採点では使わず、設定画面では
固定表示とします。TOTAL式と判定窓は上記の通常DXと共通であり、Practice独自の計算へ置き換えません。

## スコアとリプレイの区別

`RuleMode`はscore DBとreplay slotのキーに含めます。
同じ譜面でもBeatoraja / LR2oraja / DXは別スコアとして保存します。

保存箇所:

- `score_history.rule_mode`
- `score_best.rule_mode`
- `replay_slots.rule_mode`
- replay slot filename suffix
- `course_scores.rule_mode`
- `course_replay_slots.rule_mode`

`RuleMode::Beatoraja`は後方互換のためreplay slot filename suffixを省略します。
`RuleMode::Lr2Oraja` / `RuleMode::Dx`はsuffixを付けます。
コースリザルトも同じcourse hashでBeatoraja / LR2oraja / DXを別スコアとして保存します。
コースの集計キーは`(course_hash, ln_policy, rule_mode)`とし、`ln_policy`には
全譜面の`ChartLnProfile`とコースLN constraintから正規化した`LnScorePolicy`を保存します。

### 9K DX ゲージ補正と過去の記録

上記のゲージ減少量補正で、保存済みのスコア・ランプ・履歴は書き換えません。
replayはゲージ係数の世代を保存しておらず、入力を再生時のルールで採点するため、
補正前の9K DX replayは保存当時とゲージ推移・クリア成否が異なる場合があります。
replay再生結果はローカルスコアを更新せず、IRにも送信しません。

新規プレイは補正後のゲージで保存・IR送信を行います。score / replayの形式と
`RuleMode::Dx`の保存キーは変えず、IRのルール識別子も既存のまま扱う
（BMZ公式IRは`Dx`、rianIRは`body=DX MODE`）です。BMS-IRはDXを
送信対象にしません。
ゲージ係数の世代ではランキングを分離しないため、過去のDXスコアと同じ区分になります。
送信条件は[score-persistence.md](score-persistence.md)を参照します。

## 既知の注意点

- DX MODE固有のIR抑制はBMZ公式IR / rianIRには適用しません。BMS-IRはDX非対応です。
- Beatoraja modeの`#DEFEXRANK`実機挙動は、必要になった時点で改めて
  beatoraja実行結果と突き合わせます。LR2oraja modeは元種別を保持して実装済みです。
- LR2 csvskinは未対応です。現在のrule modeはgameplay / score / gaugeの切替だけを対象にします。

## 実装の入口

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
