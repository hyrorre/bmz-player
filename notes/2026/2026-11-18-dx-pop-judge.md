# DX MODEのPOP判定を参照側の変更へ追従

LR2oraja Endless Dream rianフォークで変更された`JudgeProperty.POP`（DX MODEのPMS判定）を確認し、
BMZの`dx_pop_*`へ反映しました。現在の判定値は[ルール](../../docs/rule.md#dx)、
LNのrelease marginは[LN](../../docs/ln.md)を参照します。疑似BSS / MSS関連の参照側変更は今回の対象外です。

## 参照側

参照ツリーは`~/workspace/repos/lr2oraja-ed-rian`のHEADです（ハッシュは未記録）。
対象モードの拡大はコミット`d56d231c`です。読んだファイルは次のとおりです。

- `core/src/bms/player/beatoraja/play/JudgeProperty.java`（`POP`）
- `core/src/bms/player/beatoraja/play/JudgeManager.java`（判定選択、LN終端、`longnoteMargin`）
- `core/test/bms/player/beatoraja/play/JudgePropertyPopLongNoteEndTest.java`
- `GrooveGauge.java`、`BMSPlayer.java`、`PracticeConfiguration.java`、
  `SkinHitErrorVisualizer.java`、`SkinTimingVisualizer.java`（POPN_5Kの条件）

値はすべてus、組は`{LATE側下限, EARLY側上限}`です。

| 項目 | 旧 | 新 |
| --- | --- | --- |
| note PG / GR | `{-25000,25000}` / `{-50000,50000}` | 変更なし |
| note GD | `{-87500,87500}` | `{-116667,116667}` |
| note BD | `{-100000,100000}` | `{-183333,183333}` |
| note MS | `{-112500,500000}` | `{-183333,529167}` |
| LN終端 PG / GR / GD | `{-120000,120000}` / `{-150000,150000}` / `{-217000,217000}` | 3段とも`{-116667,233333}` |
| LN終端 BD | `{-283000,283000}` | `{-183333,366667}` |
| `longnoteMargin` | 200000 | 116667 |
| 対象モード | POPN_9K | POPN_5K / POPN_9K |

LN終端のEARLY側は「判定幅116667＋離し猶予116667＝233333」という注記付きの表の値です。
`JudgeManager`では、この表で段位を決めたうえで、BD以下の早離しを従来どおり`longnoteMargin`の間保留します
（`judge >= 3 && dmtime > 0`）。表へ猶予を畳んだことで、margin処理が無効になるわけではありません。
`IIDX_RULE`は全段位がfixjudgeのため、#RANKでは拡縮しません（参照側テストで固定）。

## BMZでの表し方

BMZの`JudgeWindow`はPG / GR / GDがFAST / SLOW共通の単一値です。新しいLN終端はPG / GR / GDも非対称のため、
`JudgeWindows`に`long_note_end_fast` / `long_scratch_end_fast`（`Option<JudgeWindow>`）を追加しました。

- `long_note_end`はSLOW側（終端以降の離し）の窓です。POPではPG / GR / GD 116667、BD SLOW 183333です。
  `bad_fast_us`はSLOW側だけを見る利用者向けにEARLY側のBD 366667と揃えます。
- `long_note_end_fast`はFAST側（早離し）の窓です。POPではPG / GR / GD 233333、BD 366667です。
  FAST側でしか参照しないため、SLOW側のフィールドもFAST側と同じ値にしました。
  こうするとassistの`scale_window`のBAD上限（`min(bad_fast, bad_slow)`）がFAST側の値になります。
- engineの`long_end_window`へ離し時刻差を渡し、負（早離し）で`Some`ならFAST側の窓を使います。
  LN / CN / HCN / BSS終端の分類箇所はすべてこのhelperを通ります。
- release marginは参照側と同じく別に116667を設定します。参照側も表とmarginの両方を適用するため、
  BMZでも二重計算にはなりません。窓の境界と保留の判定は従来の`classify_normal_delta`と保留処理のままです。
- #RANK、再生速度、custom judge、コースの判定制約を適用する関数は、FAST側の窓にも同じ変換をかけます。
  DXは従来どおり#RANKを無視します。
- serdeの`default`と`skip_serializing_if`を付けました。リプレイのkaleid evidenceに保存された旧`judge_windows`は
  FAST側の窓なしで読めるため、旧POP窓のまま再現されます。POP以外の出力は変わりません。

`JudgeWindow`自体にFAST / SLOW別のPG / GR / GDを持たせる案もありました。
ただし構造体リテラルと描画側の参照が多く、非対称が必要なのはLN終端だけのため見送りました。

## PMS 5Kの経路

BMZはPMS譜面を5K / 9Kとも`KeyMode::K9`として取り込みます（`KeyMode::detect_pms_from_lanes`、
BMSONの`popn-5k`も`K9`）。判定窓（`judge_windows_for_keymode_and_rule_mode`）、DX POPゲージ
（`gauge_definitions_for_rule_mode_and_keymode`、開始30）、Practiceの開始ゲージ、タイミング表示は
すべてこの`K9`で選ぶため、PMS 5Kは追加変更なしで参照側のPOPN_5Kと同じ扱いです。
beatoraja 5K（`K5`）はIIDX判定・IIDXゲージのままで、テストで固定しました。

## 変更

- `crates/bmz-gameplay/src/judge/window.rs`: `dx_pop_*`の値、FAST側LN終端窓、marginとドキュメントコメント
- `crates/bmz-gameplay/src/judge/model.rs`: `JudgeWindows`のFAST側LN終端窓
- `crates/bmz-gameplay/src/judge/engine/mod.rs` / `hln.rs`: 離し時刻差による窓の選択
- `crates/bmz-player/src/assist.rs`、`src/screens/play_session/build.rs`: custom judge・コース制約をFAST側にも適用
- テスト: POP窓と#RANK固定、K5のIIDX維持、CN終端の境界（-233333 / -233334 / -366667 / -366668 /
  +116667 / +116668 / +183333 / +183334）とmargin、LN終端の早離し境界、旧値前提の既存テスト
- `docs/rule.md`、`docs/ln.md`

## 検証

この記録の時点では、実装担当側ではcargoを実行していません。fmt / check / Clippy / testは別途確認します。
実機でのDX MODE PMSプレイは未確認です。
