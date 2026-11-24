# LEGACY and OpenLR2 parity

This document records the evidence that BMZ Player's LEGACY mode loads charts, judges, moves the gauges and builds the CustomIR submission exactly as OpenLR2 does.
It is written for operators of IRs that accept OpenLR2 CustomIR scores, such as Stellaverse, to decide whether BMZ scores can be treated on the same basis.
The Japanese original is [legacy-openlr2-parity.md](legacy-openlr2-parity.md). The LEGACY rules are in [rule.md](rule.md#legacy) (Japanese), the tools in [tools/legacy-compare/README.md](../tools/legacy-compare/README.md) and the history of the work in the [work notes](../notes/2026/2026-11-23-legacy-comparison.md).

## Result

BMZ was compared with the unmodified OpenLR2 library at zero tolerance. Within the scope below, no difference caused by BMZ remains.

| Comparison | Size | Equal | Different |
|---|---:|---:|---:|
| New real play (judge calls recorded by an instrumented OpenLR2) | 1 play, 2,028 judgements | 2,028 judgements, EX 3,760 | 0 |
| Saved replays (judgements, mines, gauges, submission) | 234 plays | 233 | 0 |
| Chart loading (every chart in song.db) | 98,051 charts | 98,036 | 11 (key mode only) |
| Generated charts and plays (judgements, mines, gauges, submission) | 6,500 charts | 6,095 | 0 |

- The remaining saved replay is played on the P-ATTACK gauge, which BMZ does not have.
- 4 of the remaining charts cannot be loaded by OpenLR2 itself. The 11 key mode differences are explained under [Known differences](#known-differences-and-undefined-behaviour).
- The remaining 405 generated charts are ones OpenLR2 itself cannot play, plays longer than one hour, charts without notes, and OpenLR2's undefined behaviour.

## Method

### The OpenLR2 oracle

The reference is a console program linked against the prebuilt OpenLR2 v261101 library (`OpenLR2Lib.lib`) as is (`tools/legacy-compare/openlr2_oracle.cpp`).
The OpenLR2 source is not modified. Without a window or audio, the oracle calls:

- `ParseBmsFile`: chart loading, arrangement, judge windows and gauge tables
- `ProcNoteOnTiming`: every lane each millisecond, scratch first, then keys
- `LogGraphPlayData` / `LogGraphPlayerDataToEnd` / `CheckClear`: graphs and the clear type
- `PLAYSCORE::EncodeGhostData`: the ghost

Judgements and mine hits are taken from the replay hooks in `ApplyJudgeNote` / `ApplyJudgeMine`, so they come in the order OpenLR2 itself records them.
Given the inputs of a saved replay, the oracle reproduced all 984 judgements OpenLR2 had saved, in order.

### BMZ

The BMZ side is a runner built on the same chart loader, LEGACY judge engine, gauges and score code as the player (`crates/bmz-gameplay/examples/legacy_compare.rs`).
As in the player's LEGACY mode, it judges each lane in order every millisecond and applies each lane's judgements and mine hits to the score and gauges in turn.
The submission fields are built from the judgement events by `bmz_gameplay::lr2_score::Lr2ScoreRecorder`.

### What was compared

| Comparison | Content |
|---|---|
| New real play | An instrumented OpenLR2 recorded the time and input actually passed to `ProcNoteOnTiming`; the same calls were given to BMZ and every judgement compared |
| Saved replays | The inputs of 234 saved replays were given to both the oracle and BMZ |
| Chart loading | For every SP/DP chart in song.db: note count, judge windows, gauge tables and the time (in µs) of every note, LN end and mine |
| Generated charts and plays | Random charts and key inputs given to both; 6,500 charts over four seeds |

For saved replays and generated plays, the following are compared at zero tolerance:

- the poll time, lane and value of every judgement
- the poll time, lane and damage of every mine hit
- after each poll: the gauges (double precision), EX, combo, max combo, judgement counts and notes processed
- the time play closes
- every CustomIR submission field below

Compared directly with the judgements OpenLR2 saved, only 158 of the 234 replays match.
OpenLR2 saves inputs with one timer (41) but judges with another (142), so the actual judge times cannot be recovered from a saved replay.
The oracle mismatches the 76 others in exactly the same way, and BMZ matches the oracle on all 234.
BMZ's correctness is therefore shown against the oracle and against the instrumented real play.

## CustomIR submission

On the result screen OpenLR2 passes `IRScoreV1` to the CustomIR module (`LR2_customir.cpp`). The fields decided by the play, and their status:

| Field | Source in OpenLR2 | Status |
|---|---|---|
| Early/late PGREAT to POOR, combo breaks, FAST / SLOW, notes played (total and 20 lanes) | per-branch counts in `ProcSinglenote` / `ProcLongnote` | equal |
| Notes per lane (mines included) | `bmsobj_note[lane].count` | equal |
| Max combo, score, EX, rate | `ApplyJudgeNote` | equal |
| Final value of all six gauges | `ApplyJudgeNote` / `ApplyJudgeMine`, zeroed at the end of play | equal |
| Clear type | `CheckClear` | equal |
| Note count, `song_runtime` | `ParseBmsFile` | equal |
| Graphs (six gauges, combo, EX; 1000 samples each) | `LogGraphPlayData` / `LogGraphPlayerDataToEnd` | equal |
| Ghost | `PLAYSCORE::EncodeGhostData` | equal |
| Rate graph | the personal best / target score | not compared (not a result of the play) |
| Song data, settings, input device | song list and settings | not compared (not a result of the play) |

LR2 counts these differently from BMZ's own score, so BMZ builds the submission separately with LR2's rules:

- A PGREAT is never FAST / SLOW. A passed POOR is a late POOR and a combo break; an EMPTY POOR is an early POOR and not a note.
- A long note counts GREAT to BAD as FAST / SLOW when its head is hit, and is scored at its end with the head's early/late.
- All six gauges move whichever gauge was chosen. If play ends with notes left, all six become 0.

BMZ does not submit to Stellaverse yet.
An implementation feeds every judgement event and mine hit of a LEGACY play to `Lr2ScoreRecorder`, calls `log_graph` after each poll and `finish` at the end, and gets exactly the values compared here.

## Scope

| Area | Covered |
|---|---|
| Keys | 5, 7, 10 and 14 keys (SP/DP) |
| Arrangement | normal, MIRROR, RANDOM (including a fixed key) |
| Gauges | GROOVE, HARD, DEATH, EASY |
| Charts | BMS / BME / BML: LN channels, `#LNOBJ`, mines, invisible notes, `#RANDOM`, extended BPM, STOP, measure length |
| Play | single songs |

Not covered:

- other arrangements such as S-RANDOM, scratch arrangement, assist options, battle, DP flip, playback speed
- playing on P-ATTACK / G-ATTACK (BMZ has no such gauges; the values of both gauges in the submission are equal on plays with the other gauges)
- courses, GAS, maniac options
- PMS (9 keys), BMSON, CN / HCN of `#LNMODE`

## Known differences and undefined behaviour

### Key mode

For 11 of the 98,051 charts, BMZ's key mode differs from `mode` in song.db: BMZ has 5 keys, song.db 7 keys.

- 10 of them have no note channel at all, and one has only the scratch channel (16).
- OpenLR2's `ParseBMSMETA` (`LR2_songmanage.cpp`) gives 5 keys for them, as BMZ does.
- This song.db may carry values registered before OpenLR2. The cause has not been confirmed.
- The 10 charts without notes produce no judgements. On the scratch-only chart, OpenLR2 plays with song.db's 7 keys, so the lane layout may differ.

### Undefined behaviour in OpenLR2

OpenLR2 keeps each lane's notes in a buffer of 100 slots and may judge the free slots as notes at -1 ms (a lane of only mines, or the judgements that follow a BAD on the last note). BMZ reproduces these judgements too.

Once the free slots run out, OpenLR2 reads past the buffer. The result changes from run to run; the output can be corrupted or the program can crash from heap corruption.
BMZ detects the same situation and marks it, and the comparison excludes such plays as undefined. 14 of the 6,500 generated plays were affected; it is rare in real charts.

### Charts that cannot be compared

- Charts OpenLR2 itself cannot load (`ParseBmsFile` crashes with an access violation on two charts)
- Charts whose BPM becomes 0, giving infinite or NaN times in OpenLR2
- Charts without notes. OpenLR2 does not set judge windows or gauge tables, so only object times are compared, not plays.

## Reproducing

This needs the OpenLR2 source (`D:\Tools\openlr2-research`) with its prebuilt libraries, and an OpenLR2 song.db with saved replays.
Run the commands from the repository root. Details of each are in [tools/legacy-compare/README.md](../tools/legacy-compare/README.md).

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools\legacy-compare\build_openlr2_oracle.ps1
```

```powershell
cargo build -p bmz-gameplay --example legacy_compare --locked --offline
```

```powershell
python tools/legacy-compare/oracle_vs_bmz.py --results .local/legacy-parity-mines --oracle .local/openlr2-oracle/openlr2_oracle.exe --runner target\debug\examples\legacy_compare.exe --output .local/oracle-vs-bmz-submission --tolerance 0
```

```powershell
python tools/legacy-compare/loader_vs_oracle.py --oracle .local/openlr2-oracle/openlr2_oracle.exe --runner target\debug\examples\legacy_compare.exe --output .local/loader-all --jobs 10
```

```powershell
python tools/legacy-compare/fuzz.py --oracle .local/openlr2-oracle/openlr2_oracle.exe --runner target\debug\examples\legacy_compare.exe --output .local/fuzz-1 --count 500 --seed 1 --jobs 10
```

`--results` takes the directory where `compare.py` listed the saved replays. The generated plays use seeds 1 to 4 with 500, 1,000, 2,000 and 3,000 charts for the table above.
