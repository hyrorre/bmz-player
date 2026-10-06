# スコア保存・アシスト・IR送信条件

この文書は、通常プレイ・アシスト・譜面オプション・特殊モードについて、
BMZ がクリアランプ、数値ベスト、履歴、リプレイ、IR を更新する条件を整理する。
beatoraja 互換を判断するときはこの文書を入口とし、IR 固有の詳細は
`docs/ir.md` と `docs/rian-ir.md` を参照する。

レビュー基準日は 2026-08-27。BMZ の現行実装と、`.local/beatoraja/` の
参照ソースを静的に比較した結果である。未解決の差は「互換差分」に明記する。

## 用語

- **ランプ更新**: `score_best.clear_type` と対応する gauge 情報の更新。
- **数値更新**: EX score、BP、CB、max combo、判定内訳、ghost などの更新。
- **履歴保存**: BMZ の `score_history` に1プレイ分を追加すること。
- **clear-only**: ランプ、曲別 play / clear count、profile 全体の player stats を
  更新し、数値ベスト・履歴・リプレイ・IRを更新しない保存経路。
- **実効アシスト**: 設定が ON であるだけでなく、その譜面または判定窓に実際の
  効果が発生したアシスト。BMZ の `AssistRuntime.effective_mask` / `level` に相当する。

## 単曲の保存判定

BMZ はプレイ終了時に次の順で保存可否を決める。

1. full autoplay、replay 再生、Practice、key mode 変換済みなら、ランプを含めて
   ローカル score を保存せず、リプレイと IR も生成しない。
2. 上記に該当せず、実効アシストが `LightAssist` または `Assist` なら、クリア成立時の
   lamp を `LightAssistEasy` / `AssistEasy` に制限し、clear-only 更新だけを行う。
3. 実効アシストが無ければ、履歴と数値ベストを更新し、設定された replay slot を
   評価する。その後、provider と `send_policy` の条件を満たす IR job を enqueue する。

| プレイ条件 | ランプ・回数・全体統計 | 数値ベスト | 履歴 | replay | BMZ公式IR | rianIR | BMS-IR |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 通常手動、実効アシストなし | 更新 | 更新 | 保存 | 条件付き保存 | 送信候補 | 送信候補 | 送信候補 |
| 実効 `LightAssist` / `Assist` | assist lamp・回数・player statsを更新 | 更新しない | 保存しない | 保存しない | 送信しない | 送信しない | 送信しない |
| full autoplay / `AUTOPLAY BATTLE` | 保存しない | 保存しない | 保存しない | 保存しない | 送信しない | 送信しない | 送信しない |
| replay 再生 | 保存しない | 保存しない | 保存しない | 保存しない | 送信しない | 送信しない | 送信しない |
| Practice | 保存しない | 保存しない | 保存しない | 保存しない | 送信しない | 送信しない | 送信しない |
| key mode 変換 | 保存しない | 保存しない | 保存しない | 保存しない | 送信しない | 送信しない | 送信しない |
| `BATTLE` / `BATTLE AS` | 専用 bucket を更新 | 更新 | 保存 | 条件付き保存 | 送信候補 | 送信しない | 専用bucketへ送信候補 |
| `G-BATTLE` | 元譜面の通常 bucket を更新 | 更新 | 保存 | 条件付き保存 | 送信候補 | 送信候補 | 通常bucketへ送信候補 |

アシストを有効にしていても `Failed` / `NoPlay` は assist lamp へ昇格しない。

`BATTLE AS` の scratch autoplay は部分 autoplay であり、BMZ の full autoplay 判定には
ならない。したがって現状は `BattleAutoScratch` bucket へ通常スコアを保存する。

`G-BATTLE` は相手リプレイを表示専用レーンで進め、プレイヤー側だけを採点する BMZ
独自モードである。beatoraja の `BATTLE` / `BATTLE AS` と同じものではない。

## アシスト設定の実効レベル

以下は BMZ と beatoraja で一致している。複数設定が有効な場合は最も強いレベルを
採用し、コースでは全ステージ中の最も強いレベルを使う。

| 設定 | 実効条件 | レベル | 保存結果 |
| --- | --- | --- | --- |
| EXPAND JUDGE | key / scratch の PGREAT・GREAT・GOOD、または LN margin のいずれかが 100% 超 | Assist | clear-only |
| CONSTANT (`SCROLL=REMOVE`) | BPM、SCROLL、STOP の変化を実際に除去 | Light Assist | clear-only |
| JUDGE AREA | 表示のみ | なし | 通常保存 |
| LEGACY NOTE (`LONGNOTE=REMOVE`) | LN を実際に通常ノートへ変換 | Assist | clear-only |
| MARK NOTE | 表示のみ | なし | 通常保存 |
| BPM GUIDE | 可変 BPM 譜面で表示 | Light Assist | clear-only |
| NO MINE (`MINE=REMOVE`) | Mine を実際に除去 | Light Assist | clear-only |
| EXTRA NOTE | BGM ノートを実際に可視ノートへ追加 | Assist | clear-only |
| `SCROLL=ADD` | SCROLL 変化を追加 | なし | 通常保存 |
| `LONGNOTE=ADD LN` | 通常ノートを LN に変換 | なし | 通常保存 |
| `LONGNOTE=ADD CN/HCN/ALL` | CN / HCN を実際に追加 | Assist | clear-only |
| `MINE=ADD RANDOM/NEAR/BLANK` | Mine を追加 | なし | 通常保存 |

重要なのは「設定値」ではなく「実際に効果が出たか」である。たとえば固定 BPM 譜面の
BPM GUIDE、Mine の無い譜面の NO MINE、100% 以下の EXPAND JUDGE はスコアを無効化しない。

## 配置オプション

BMZ は `apply_chart_assists` の結果へ、実際に適用された1P/2P配置のアシストレベルを
合成する。次の5種は beatoraja と同じ `LightAssist` とし、単曲・コースとも共通の
clear-only 経路を使う。

| 配置 / DOUBLE | BMZ | beatoraja | 判定 |
| --- | --- | --- | --- |
| NORMAL / MIRROR / RANDOM / R-RANDOM / S-RANDOM | 通常保存・IR候補 | 通常保存・IR候補 | 一致 |
| FLIP | 通常 bucket に通常保存・IR候補 | 通常保存・IR候補 | 概ね一致 |
| SPIRAL | Light Assist、clear-only、IRなし | Light Assist、clear-only、IRなし | 一致 |
| H-RANDOM | Light Assist、clear-only、IRなし | Light Assist、clear-only、IRなし | 一致 |
| ALL-SCR | Light Assist、clear-only、IRなし | Light Assist、clear-only、IRなし | 一致 |
| RANDOM-EX | Light Assist、clear-only、IRなし | Light Assist、clear-only、IRなし | 一致 |
| S-RANDOM-EX | Light Assist、clear-only、IRなし | Light Assist、clear-only、IRなし | 一致 |
| F-RANDOM / MF-RANDOM | 通常保存・IR候補 | 同名の直接比較対象なし | BMZ / provider 固有方針 |
| BATTLE / BATTLE AS | 専用 bucket に通常保存。BMZ公式IR候補、rianIRは除外 | Light Assist、clear-only、IRなし | **設計差** |

SPIRAL など5種は provider 判定より前に共通の実効アシストで除外されるため、
BMZ公式IR、rianIR、BMS-IRのいずれにも enqueue されない。scratch のない mode で scratch 系配置が
NORMAL に fallback した場合は、適用後の配置を基準にするため Light Assist にならない。

BATTLE は BMZ では `Off` / `Battle` / `BattleAutoScratch` の score bucket を分離して
ローカル保存する設計である。BMZ公式IRもその bucket を payload に持つ。rianIR は
BATTLE 専用ランキングを持たないため client 側で除外する。BMS-IR は `Battle` と
`BattleAutoScratch` を互いに分離したnative rankingとして保存し、通常bucketへ混ぜない。

## Key mode 変換

BMZ は `SpToDp` と `SevenToSix` を `score_save_disabled` とし、ランプ・回数を含めて
何も保存しない。`SevenToNine` は `seven_to_nine_rule_mode` で規則を選ぶ。

- `7K`（既定）: 表示と入力は9Kへ変換するが、判定窓・ゲージ・スコアidentityは元の
  7Kを使う。通常7Kとしてランプ・スコア・replayを保存し、IR送信候補にできる。
  replayの入力レーンも通常7K形式で保存し、再生時は現在の7K→9K設定へ投影する。
- `9K`: 9Kの判定窓・ゲージを使い、ランプ・回数・スコア・replay・IRをすべて
  保存しない。Light Assistのclear-only保存にはしない。

replay再生時は、replayを保存しない `SpToDp`、`SevenToSix`、9K規則の`SevenToNine`を
無効化する。7K規則の`SevenToNine`だけは変換を維持し、通常7K形式で保存された入力を
現在の7K→9K設定へ投影する。

beatoraja の 7K→9K `ModeModifier` は Light Assist であり、数値・replay・IRは更新しないが
`LightAssistEasy` ランプと play / clear count は更新する。BMZの `9K` 規則はそれより
厳しい完全非保存、`7K` 規則は元譜面のルールとidentityを維持するBMZ独自仕様である。

## 通常プレイのベスト更新条件

BMZ の score identity は chart SHA-256、LN policy、DOUBLE bucket、RuleMode の組である。
通常保存では全プレイを `score_history` に追加し、次の規則で `score_best` を更新する。

- clear lamp は数値スコアと独立して、より高い `ClearType` だけを採用する。
- BP は最小値、CB は最小値、max combo は最大値を独立して保持する。
- 判定内訳、ghost、replay path、device、`played_at` を代表する履歴は、
  `(EX score 高、BP 低、CB 低、max combo 高)` の辞書順で選ぶ。
- play count は通常保存と clear-only の両方、clear count は `NoPlay` / `Failed` 以外で増える。

beatoraja は lamp、EX score、average judge、min BP、max combo をそれぞれ独立更新し、
判定内訳と ghost は strict な EX score 更新時に置き換える。このため、同 EX score で
BP / CB / combo だけが改善した場合の代表判定内訳・ghost・option/seed は BMZ と一致しない。
また BMZ は average judge を best 項目として保存せず、代わりに CB を保持する。

## 最終プレイ日時

skin数値ref `243..249` はBESTの `played_at` と別に最終保存日時を返す。
単曲はScoreKey（SHA-256 / LN policy / DOUBLE bucket / RuleMode）単位で、
non-autoplay履歴の最大 `played_at` と、clear-only保存日時の大きい方を採用する。
自己ベスト未更新やFailedも対象。保存しないAutoplay / Replay / Practice / 保存禁止の
変換などは更新しない。Selectの一覧ロード時にバッチ取得し、描画時にはDBを読まない。
Resultは今回保存に成功したattemptの日時を使い、非保存時は欠損とする。
コースはcourse hash / LN policy / RuleMode単位で `replay_only = 0` のcourse attemptの
最大日時を使う。コースのclear-only attemptも含む。

score.db migration 31は、履歴を持たないclear-only日時を保存する
`score_unrecorded_plays` 表と履歴検索indexを追加する。既存BEST日時は変更しない。
通常履歴は都度MAXを集計するため、import日時補正や履歴cleanupに追従し、削除済み
履歴の日時が残らない。clear-only日時は履歴cleanupでも保持する。

移行時には既存履歴から最新日時を取得し、履歴由来のBESTを持たない行からは
保存済み `played_at` をclear-only日時として移す。旧版ではclear-onlyを繰り返しても
この値を更新しなかったため、旧DBの最初の日時しか残っていない場合や、通常履歴と
混在する過去アシストの最終日時は復元できない。新たな保存以降は更新される。
移行は追加のみでスコア・リプレイを削除しない。戻す場合は起動前に採ったscore.dbの
整合したbackupへ復元する。`user_version` の手動巻き戻しは行わない。

## IR送信条件

単曲 IR は、通常の履歴保存が成功して `score_history_id > 0` になった後でだけ enqueue
される。実効アシスト、autoplay、replay、Practice、保存禁止のkey mode変換はこの時点より
前に除外されるため、`send_policy=Always` でも送信されない。7K規則の`SevenToNine`だけは
元の7K譜面として送信候補になる。

| `send_policy` | BMZ の単曲条件 | beatoraja の単曲条件 |
| --- | --- | --- |
| `Always` | 常に送信候補 | 常に送信候補 |
| `CompleteSong` | 最終 gauge value が 0 より大きい | 最終 gauge log が 0 より大きい |
| `UpdateScore` | 初プレイ、または EX / lamp / max combo / BP / CB のいずれか改善 | EX / lamp / max combo / min BP のいずれか改善 |

BMZ の CB 改善だけでも `UpdateScore` が送信候補になる点が差である。これは client 側の
送信量制御であり、server の best 更新規則とは別である。

その後、provider 固有条件を適用する。現行 rianIR provider は `BATTLE` / `BATTLE AS` を
除外し、譜面時間の妥当性も検査する。BMZ公式IRとBMS-IRは BATTLE bucket を表現できる。
BMS-IRはBeatoraja/LR2oraja ruleと実効LN/CN/HCNも分離し、DXは送らない。

## アシスト時の補助データ差

単曲の目に見える主要動作は、BMZ も beatoraja も「assist lamp と play / clear count
だけを best に反映し、数値・replay・IRを更新しない」で一致する。ただし内部の補助
データは同一ではない。

beatoraja の `PlayDataAccessor.writeScoreData(..., updateScore=false)` は、lamp と回数に加え、
最新 play date、lamp 改善時の score log、全プレイの score data log、player 全体の
判定数・play count・clear count・play time を更新する。また単曲 trophy 判定自体は
`updateScore` で抑止されず、gauge 条件によって EASY trophy が追加され得る。

BMZ の clear-only も profile 全体の `player_stats` へ、判定数・play / clear count・
play time・max combo を加算する。一方、`score_history`、リプレイ、IRは作らず、既存
best 行の `played_at` も更新しない。日次統計は `score_history` から算出するため
アシストプレイを含まず、単曲 trophy DB も現在持たない。player totals は一致したが、
日次統計・play log・trophy・最新日時には引き続き互換差がある。

## コース

コースは全ステージの実効アシストの最大値を使う。途中 FAILED はアシストより優先して
最終 lamp を `Failed` とする。完走時は Assist が1曲でもあれば `AssistEasy`、
Light Assist だけなら `LightAssistEasy` になる。この優先順位は beatoraja と一致する。

| コース条件 | BMZ の保存・送信 |
| --- | --- |
| 通常手動、全譜面解決済み | course attempt と数値を保存し、条件を満たす stage 履歴リンク・replay・trophy も保存。release course なら IR job を enqueue |
| いずれかの stage が実効 assist | 各stageの曲ランプ・回数・player statsを更新し、数値を 0 とした course attempt で lamp と回数だけ保存。stage 履歴リンク、replay、trophy、IRなし |
| autoplay / replay playback / key mode 変換 | course attempt 自体を保存せず、IRなし |
| FAILED | 未プレイ譜面を含めた BP で `Failed` attempt を保存。非アシストなら通常の数値保存・IR候補 |

BMZ のコース IR は provider の単曲 `send_policy` を参照せず、course definition が
`release=true` で provider 条件を満たせば enqueue する。beatoraja も CourseResult の
`CompleteSong` / `UpdateScore` 分岐が実質無効で、`updateCourseScore` と release のみで
送信可否が決まるため、この点は一致する。

保存構造は異なる。beatoraja は course best 行を clear-only 更新するのに対し、BMZ は
assist course も数値 0 の attempt 行として残し、best score と best lamp を別クエリで
選ぶ。表示上の best 数値を汚さないことはテストで固定している。

## レビュー所見と今後の判断

優先度順の未解決事項は次のとおり。

1. **BATTLE の仕様選択**: beatoraja 準拠の Light Assist に戻すか、BMZ 固有の専用
   score bucket として維持するかを明示する。後者なら BMZ公式IRのランキングも
   DOUBLE bucket で必ず分離する。
2. **アシスト履歴・日次統計**: beatoraja の scoredatalog 相当を必要とするか決める。
   profile 全体統計とは分離した assist attempt log を追加する余地がある。
3. **通常 best と `UpdateScore`**: CB と同 EX score の tie-break を BMZ 固有仕様として
   維持するなら、その差を IR server / UI と共有する。

## 実装参照

BMZ:

- アシスト適用: `crates/bmz-player/src/assist.rs`
- アシスト状態: `crates/bmz-gameplay/src/session/state.rs`
- 配置適用: `crates/bmz-player/src/screens/play_session/arrange/`
- 単曲終了・IR判定: `crates/bmz-player/src/screens/play_finish.rs`
- 単曲保存: `crates/bmz-player/src/storage/play_result.rs`
- best / clear-only: `crates/bmz-player/src/storage/score_db/write.rs`
- コース集約: `crates/bmz-player/src/screens/course_session.rs`
- コース保存・IR: `crates/bmz-player/src/app/course_flow/finish.rs`、`ir.rs`
- rianIR eligibility / payload: `crates/bmz-player/src/ir/rian_ir/request.rs`

beatoraja:

- アシストと配置の score flag: `.local/beatoraja/src/bms/player/beatoraja/play/BMSPlayer.java`
- modifier の assist level: `.local/beatoraja/src/bms/player/beatoraja/pattern/`
- 単曲・コース IR / replay: `.local/beatoraja/src/bms/player/beatoraja/result/MusicResult.java`、
  `CourseResult.java`
- DB更新: `.local/beatoraja/src/bms/player/beatoraja/PlayDataAccessor.java`、`ScoreData.java`
