# スコア保存・アシスト・IR送信条件

この文書は、通常プレイ・アシスト・譜面オプション・特殊モードについて、
BMZがクリアランプ、数値ベスト、履歴、リプレイ、IRを更新する条件を整理します。
beatoraja互換を判断するときはこの文書を入口とし、IR固有の詳細は
`docs/ir.md`と`docs/rian-ir.md`を参照します。

レビュー基準日は2026-08-27です。BMZの現行実装と、`.local/beatoraja/`の
参照ソースを静的に比較した結果です。未解決の差は「互換差分」に明記します。

## 用語

- **ランプ更新**: `score_best.clear_type`と対応するgauge情報の更新です。
- **数値更新**: EX score、BP、CB、max combo、判定内訳、ghostなどの更新です。
- **履歴保存**: BMZの`score_history`に1プレイ分を追加します。
- **clear-only**: ランプ、曲別play / clear count、profile全体のplayer statsを
  更新し、数値ベスト・履歴・リプレイ・IRを更新しない保存経路です。
- **実効アシスト**: 設定がONであるだけでなく、その譜面または判定窓に実際の
  効果が発生したアシストです。BMZの`AssistRuntime.effective_mask` / `level`に相当します。

## 単曲の保存判定

BMZはプレイ終了時に次の順で保存可否を決めます。

1. full autoplay、replay再生、Practice、key mode変換済みなら、ランプを含めて
   ローカルscoreを保存せず、リプレイとIRも生成しません。
2. 上記に該当せず、実効アシストが`LightAssist`または`Assist`なら、クリア成立時の
   lampを`LightAssistEasy` / `AssistEasy`に制限し、clear-only更新だけを行います。
3. 実効アシストが無ければ、履歴と数値ベストを更新し、設定されたreplay slotを
   評価します。その後、providerと`send_policy`の条件を満たすIR jobをenqueueします。

| プレイ条件 | ランプ・回数・全体統計 | 数値ベスト | 履歴 | replay | BMZ公式IR | rianIR | BMS-IR |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 通常手動、実効アシストなし | 更新 | 更新 | 保存 | 条件付き保存 | 送信候補 | 送信候補 | 送信候補 |
| 実効`LightAssist` / `Assist` | assist lamp・回数・player statsを更新 | 更新しない | 保存しない | 保存しない | 送信しない | 送信しない | 送信しない |
| full autoplay / `AUTOPLAY BATTLE` | 保存しない | 保存しない | 保存しない | 保存しない | 送信しない | 送信しない | 送信しない |
| replay再生 | 保存しない | 保存しない | 保存しない | 保存しない | 送信しない | 送信しない | 送信しない |
| Practice | 保存しない | 保存しない | 保存しない | 保存しない | 送信しない | 送信しない | 送信しない |
| key mode変換 | 保存しない | 保存しない | 保存しない | 保存しない | 送信しない | 送信しない | 送信しない |
| `BATTLE` / `BATTLE AS` | 専用bucketを更新 | 更新 | 保存 | 条件付き保存 | 送信候補 | 送信しない | 専用bucketへ送信候補 |
| `G-BATTLE` | 元譜面の通常bucketを更新 | 更新 | 保存 | 条件付き保存 | 送信候補 | 送信候補 | 通常bucketへ送信候補 |

アシストを有効にしていても`Failed` / `NoPlay`はassist lampへ昇格しません。

### ASSIST EASYゲージ

ASSIST EASYゲージのみの使用は実効アシスト扱いとせず、通常の数値スコア保存とIR送信の
対象になります。クリアランプは`LightAssistEasy`（L-ASSIST）とします。beatoraja /
LR2oraja Endless Dreamの`ClearType.getClearTypeByGauge(ASSISTEASY)`と同じです。
Gauge Auto Shiftで最終的なクリア判定ゲージがASSIST EASYになった場合も同じ扱いです。

`AssistEasy`（ASSIST）は、EXPAND JUDGEなどの実効Assistを使ったプレイ専用のランプです。
ゲージ選択だけでは付きません。

| 条件 | ランプ | 数値スコア保存 | IR送信 |
| --- | --- | --- | --- |
| ASSIST EASYゲージのみ | `LightAssistEasy` | 可 | 可 |
| GASでASSIST EASYに移行 | `LightAssistEasy` | 可 | 可 |
| BPM GUIDEなどの実効Light Assist | `LightAssistEasy` | 不可（clear-only） | 不可 |
| EXPAND JUDGEなどの実効Assist | `AssistEasy` | 不可（clear-only） | 不可 |

判定の優先順位はbeatoraja互換です。FAILED、実効アシスト、FULL COMBO以上
（`FullCombo` / `Perfect` / `Max`）、ゲージのクリア種別の順に決めます。したがって
ASSIST EASYゲージでもFULL COMBO以上はそのコンボランプになり、実効アシストがあれば
FULL COMBOでも`LightAssistEasy` / `AssistEasy`になります。

ランプの比較順は既存のID順（`AssistEasy = 2` < `LightAssistEasy = 3` < `Easy = 4`）の
ままです。修正前にASSIST EASYゲージで保存された`AssistEasy`ランプは、本来の実効Assist
由来のランプと区別できないため、自動では書き換えません。次にASSIST EASYゲージで
クリアした時点で`LightAssistEasy`へ通常どおり更新されます。

`BATTLE AS`のscratch autoplayは部分autoplayであり、BMZのfull autoplay判定には
なりません。したがって現状は`BattleAutoScratch` bucketへ通常スコアを保存します。

`G-BATTLE`は相手リプレイを表示専用レーンで進め、プレイヤー側だけを採点するBMZ
独自モードです。beatorajaの`BATTLE` / `BATTLE AS`と同じものではありません。

## アシスト設定の実効レベル

以下はBMZとbeatorajaで一致しています。複数設定が有効な場合は最も強いレベルを
採用し、コースでは全ステージ中の最も強いレベルを使います。

| 設定 | 実効条件 | レベル | 保存結果 |
| --- | --- | --- | --- |
| EXPAND JUDGE | key / scratchのPGREAT・GREAT・GOOD、またはLN marginのいずれかが100%超 | Assist | clear-only |
| CONSTANT (`SCROLL=REMOVE`) | BPM、SCROLL、STOPの変化を実際に除去 | Light Assist | clear-only |
| JUDGE AREA | 表示のみ | なし | 通常保存 |
| LEGACY NOTE (`LONGNOTE=REMOVE`) | LNを実際に通常ノートへ変換 | Assist | clear-only |
| MARK NOTE | 表示のみ | なし | 通常保存 |
| BPM GUIDE | 可変BPM譜面で表示 | Light Assist | clear-only |
| NO MINE (`MINE=REMOVE`) | Mineを実際に除去 | Light Assist | clear-only |
| EXTRA NOTE | BGMノートを実際に可視ノートへ追加 | Assist | clear-only |
| `SCROLL=ADD` | SCROLL変化を追加 | なし | 通常保存 |
| `LONGNOTE=ADD LN` | 通常ノートをLNに変換 | なし | 通常保存 |
| `LONGNOTE=ADD CN/HCN/ALL` | CN / HCNを実際に追加 | Assist | clear-only |
| `MINE=ADD RANDOM/NEAR/BLANK` | Mineを追加 | なし | 通常保存 |
| AUTO SCRATCH | 全譜面オプション適用後、自分側のスクラッチレーンに採点対象ノーツがある | Assist | clear-only |

AUTO SCRATCHはLR2 / LR2oraja Endless Dreamの同名オプションに合わせます。両者ともスコアを保存せず、
クリアはイージー（Endless DreamはASSIST EASY）扱いにします。リプレイ再生では自動演奏せず、
全体AUTOPLAYではもともと保存しません。DP OPTIONの`BATTLE`と併用した場合は`BATTLE AS`
として専用bucketに通常保存し、アシスト扱いにしない（LR2もD-BATTLE中のAUTO SCRATCHを
イージー扱いにしない）です。CLIの`--auto-scratch on`は従来どおりスコア・リプレイを保存しません。

重要なのは「設定値」ではなく「実際に効果が出たか」です。たとえば固定BPM譜面の
BPM GUIDE、Mineの無い譜面のNO MINE、100%以下のEXPAND JUDGEはスコアを無効化しません。

## 配置オプション

BMZは`apply_chart_assists`の結果へ、実際に適用された1P/2P配置のアシストレベルを
合成します。次の5種はbeatorajaと同じ`LightAssist`とし、単曲・コースとも共通の
clear-only経路を使います。

| 配置 / DOUBLE | BMZ | beatoraja | 判定 |
| --- | --- | --- | --- |
| NORMAL / MIRROR / RANDOM / R-RANDOM / S-RANDOM | 通常保存・IR候補 | 通常保存・IR候補 | 一致 |
| FLIP | 通常bucketに通常保存・IR候補 | 通常保存・IR候補 | 概ね一致 |
| SPIRAL | Light Assist、clear-only、IRなし | Light Assist、clear-only、IRなし | 一致 |
| H-RANDOM | Light Assist、clear-only、IRなし | Light Assist、clear-only、IRなし | 一致 |
| ALL-SCR | Light Assist、clear-only、IRなし | Light Assist、clear-only、IRなし | 一致 |
| RANDOM-EX | Light Assist、clear-only、IRなし | Light Assist、clear-only、IRなし | 一致 |
| S-RANDOM-EX | Light Assist、clear-only、IRなし | Light Assist、clear-only、IRなし | 一致 |
| F-RANDOM / MF-RANDOM | 通常保存・IR候補 | 同名の直接比較対象なし | BMZ / provider固有方針 |
| BATTLE / BATTLE AS | 専用bucketに通常保存です。BMZ公式IR候補、rianIRは除外 | Light Assist、clear-only、IRなし | **設計差** |

SPIRALなど5種はprovider判定より前に共通の実効アシストで除外されるため、
BMZ公式IR、rianIR、BMS-IRのいずれにもenqueueされません。scratchのないmodeでscratch系配置が
NORMALにfallbackした場合は、適用後の配置を基準にするためLight Assistになりません。

BATTLEはBMZでは`Off` / `Battle` / `BattleAutoScratch`のscore bucketを分離して
ローカル保存する設計です。BMZ公式IRもそのbucketをpayloadに持ちます。rianIRは
BATTLE専用ランキングを持たないためclient側で除外します。BMS-IRは`Battle`と
`BattleAutoScratch`を互いに分離したnative rankingとして保存し、通常bucketへ混ぜません。

## Key mode 変換

BMZは`SpToDp`と`SevenToSix`を`score_save_disabled`とし、ランプ・回数を含めて
何も保存しません。`SevenToNine`は`seven_to_nine_rule_mode`で規則を選びます。

- `7K`（既定）: 表示と入力は9Kへ変換するが、判定窓・ゲージ・スコアidentityは元の
  7Kを使います。通常7Kとしてランプ・スコア・replayを保存し、IR送信候補にできます。
  replayの入力レーンも通常7K形式で保存し、再生時は現在の7K→9K設定へ投影します。
- `9K`: 9Kの判定窓・ゲージを使い、ランプ・回数・スコア・replay・IRをすべて
  保存しません。Light Assistのclear-only保存にはしません。

replay再生時は、replayを保存しない`SpToDp`、`SevenToSix`、9K規則の`SevenToNine`を
無効化します。7K規則の`SevenToNine`だけは変換を維持し、通常7K形式で保存された入力を
現在の7K→9K設定へ投影します。

beatorajaの7K→9K `ModeModifier`はLight Assistであり、数値・replay・IRは更新しないが
`LightAssistEasy`ランプとplay / clear countは更新します。BMZの`9K`規則はそれより
厳しい完全非保存、`7K`規則は元譜面のルールとidentityを維持するBMZ独自仕様です。

## 通常プレイのベスト更新条件

BMZのscore identityはchart SHA-256、LN policy、DOUBLE bucket、RuleModeの組です。
通常保存では全プレイを`score_history`に追加し、次の規則で`score_best`を更新します。

- clear lampは数値スコアと独立して、より高い`ClearType`だけを採用します。
- EX scoreは最高値、BPは最小値、CBは最小値、max comboは最大値を独立して保持します。
  EX score更新時もBP / CB / comboを悪化させず、EX scoreが低いプレイからも
  各項目の改善を採用します。等値なら既存値を維持します。
- 判定内訳、ghost、replay path、device、`played_at`を代表する履歴は、
  `(EX score 高、BP 低、CB 低、max combo 高)`の辞書順で選びます。
- play countは通常保存とclear-onlyの両方、clear countは`NoPlay` / `Failed`以外で増えます。

代表履歴のタイブレークには`best_score_history_id`が指す実際の履歴のBP / CB / comboを
使います。`score_best`の独立ベスト値は異なるプレイから集約されるため、比較には使いません。
全条件が同じ場合は既存の代表履歴を維持し、判定内訳・ghostなども上書きしません。
旧データで代表履歴が欠損している場合、同EXでの代表情報の置換は行わず、EXが上がった
時点で新しい代表履歴を設定します。独立ベスト値とランプはその間も通常どおり更新します。
clear-onlyだけの中立行はBP / CBの0を実績とみなさず、最初の通常保存で初期化します。
この初期化はEX scoreが0の通常プレイにも適用します。

通常プレイ、beatoraja / LR2 / LR2oraja系のインポート、IR履歴ダウンロードは同じ
保存処理を通ります。履歴cleanup時の再集計も、独立したMIN / MAXと代表履歴の選択を
分離します。選曲・リザルト・スキン向けBESTには、この独立ベスト値が渡されます。

beatorajaはlamp、EX score、average judge、min BP、max comboをそれぞれ独立更新し、
判定内訳とghostはstrictなEX score更新時に置き換えます。このため、同EX scoreで
BP / CB / comboだけが改善した場合の代表判定内訳・ghost・option/seedはBMZと一致しません。
またBMZはaverage judgeをbest項目として保存せず、代わりにCBを保持します。

### 既存ユーザーの数値ベスト修復

修正版の初回起動時、score DB migration v33が保存済み`score_history`から最小BP・
最小CB・最大コンボを修復します。過去のEX更新や同点タイブレークで失われた独立ベストを、
同じchart SHA-256 / LN policy / DOUBLE bucket / RuleMode内の
`MIN(bp)` / `MIN(cb)` / `MAX(max_combo)`で再集計します。旧形式のRuleModeは
既存の履歴再集計と同じBeatoraja互換bucketへ正規化します。

修復はSQLiteトランザクション内で一度だけ実行し、起動ごとの全件再集計はしません。
インポート済み履歴も対象に含み、外部IRの再ダウンロードやスコア再送信は発生しません。
更新するのは`score_best.bp` / `cb` / `max_combo`だけです。EX score、ランプ、代表履歴ID、
判定内訳、ghost、リプレイ、device、日時、回数、履歴とIRの送信識別情報は維持します。

clear-onlyのみのキーや履歴が残っていないキーは変更しません。履歴から確認できない
過去の記録は復元できません。これら3項目は既存スキーマで`NOT NULL`であり、
履歴の0は有効な記録として扱います。nullableなgauge valueや旧best snapshotのNULLを
数値ベストへ流用しません。

### 外部スコアDBの最小BP

beatoraja / LR2 / LR2oraja / LR2oraja ED系の`score`は、最高EXの判定内訳と、
異なるプレイで達成した最小BP・最大コンボ・最高ランプを持つ集計行です。
インポート時は`score_history.bp`を判定内訳から従来どおり計算し、元DBの`minbp`を
nullableな`score_history.source_min_bp`へ別に保存します。判定数を変更したり、
最小BPを達成した架空の履歴を追加したりしません。元DBに独立最小CBは無いため、
CBは判定内訳からの計算を維持します。最大コンボとランプの取り込みも従来どおりです。

通常保存と履歴cleanup後の再集計は、各履歴のBPと、判明しているsource minbpの
両方から最小値を取ります。例えばEX 2100の判定内訳BPが20、元DBのminbpが5なら、
履歴BPは20、source minbpは5、BEST BPは5になります。既存BMZ BEST BPが3なら3を
維持します。代表履歴のタイブレークには履歴BPの20を使い、source minbpは使いません。

ローカルプレイと不明な値は`source_min_bp = NULL`です。LR2の負の未記録値（-1）、
beatoraja / LR2oraja系の`Integer.MAX_VALUE`、SQLのNULLは独立BPとして使いません。
BP 0は有効な記録です。Empty Poorを含むため、BPがノーツ数を超えるという理由だけでは
無効にしません。beatoraja形式で`score`がなく`scoredatalog`だけある場合も、既存の
fallback経路で取り込みます。この場合のminbpは、その行のプレイに対応する元の値です。

同じインポートを繰り返した場合、source minbpが未保存なら補完し、小さくなった場合は
改善を採用します。後の元DBで悪化・不明になっても、以前確認できた値を維持します。
BP補正だけでは履歴の追加やplay / clear countの増加、判定内訳・代表履歴ID・ghost・
リプレイの更新を行いません。device補正・空ghost補完は既存規則に従い、BP補正と同じ
トランザクションで行います。根拠を持つ履歴を削除すれば、その履歴だけが提供していた
最小BPは残さず、残存履歴から再計算します。

beatoraja系の`score`では、EXが同じでもBP改善時にoption / seedが変わります。
そのため、集計行の取り込み元を`score_history_sources`の`score_db_aggregate`として
保存します。厳密一致に失敗した場合だけ、この由来が確認できる候補についてseed・
1P/2P配置・実適用DOUBLEの違いを許容して照合します。譜面SHA-256 / LN policy /
DOUBLE bucket / RuleMode / source kindと、EX・判定内訳・履歴BP/CB・コンボ・
ランプ・ゲージなどの一致は引き続き要求し、既存のseedや配置情報は書き換えません。

確認済み集計候補が1件ならBP等を補完し、同じ結果を再インポートしても履歴や回数を
増やしません。確認済み候補が複数なら、代表履歴の優先順位で勝手に選ばず、未対応理由を
表示してスキップします。由来が未確認の旧履歴しか候補にない場合も同様です。これらの
場合はBPも推測補正せず、重複履歴も追加しません。旧履歴へ厳密一致できた場合には
集計行との対応を記録し、以後のoption / seed変更を照合できるようにします。
`scoredatalog`・通常プレイ・IR履歴にはこの補完照合を適用せず、既存の照合規則を維持します。
由来の記録は履歴と同じトランザクションで保存し、履歴削除時に一緒に削除されます。
既存の出所テーブルを使うため、追加マイグレーションやIR識別情報の変更はありません。

score DB migration v34は、このNULL可の列を追加します。既存のEX・ランプ・判定内訳・
ghost・リプレイ・IR識別情報・回数・履歴値は変更せず、過去のminbpを推測して埋めません。
**旧バージョンで失われたインポート最小BPを復旧するには、修正版で元のスコアDBを
同じインポート種別・設定で再インポートしてください。** 対応する既存履歴へ値を補完し、
BEST BPを改善します。外部IRへの自動送信は行いません。

上流コードと形式ごとの確認範囲は[調査記録](../notes/2026/2026-11-04-import-source-min-bp.md)を参照します。

## 最終プレイ日時

skin数値ref `243..249`はBESTの`played_at`と別に最終保存日時を返します。
単曲はScoreKey（SHA-256 / LN policy / DOUBLE bucket / RuleMode）単位で、
non-autoplay履歴の最大`played_at`と、clear-only保存日時の大きい方を採用します。
自己ベスト未更新やFailedも対象です。保存しないAutoplay / Replay / Practice / 保存禁止の
変換などは更新しません。Selectの一覧ロード時にバッチ取得し、描画時にはDBを読みません。
Resultは今回保存に成功したattemptの日時を使い、非保存時は欠損とします。
コースはcourse hash / LN policy / RuleMode単位で`replay_only = 0`のcourse attemptの
最大日時を使います。コースのclear-only attemptも含みます。

score.db migration 31は、履歴を持たないclear-only日時を保存する
`score_unrecorded_plays`表と履歴検索indexを追加します。既存BEST日時は変更しません。
通常履歴は都度MAXを集計するため、import日時補正や履歴cleanupに追従し、削除済み
履歴の日時が残りません。clear-only日時は履歴cleanupでも保持します。

移行時には既存履歴から最新日時を取得し、履歴由来のBESTを持たない行からは
保存済み`played_at`をclear-only日時として移します。旧版ではclear-onlyを繰り返しても
この値を更新しなかったため、旧DBの最初の日時しか残っていない場合や、通常履歴と
混在する過去アシストの最終日時は復元できません。新たな保存以降は更新されます。
移行は追加のみでスコア・リプレイを削除しません。戻す場合は起動前に採ったscore.dbの
整合したbackupへ復元します。`user_version`の手動巻き戻しは行いません。

## IR送信条件

score.db migration 32は、各履歴のlocal backfill用識別キーを
`score_history_ir_identity`に追加します。既存履歴にはUUID v4を割り当て、
新規履歴では保存と同じtransactionで生成します。同じ履歴の再送ではキーを維持し、
別profileで同じ数値のhistory IDになっても衝突しません。DBを複製した場合、
複製済み履歴のキーは共通のまま、その後に追加した履歴はそれぞれ別のキーになります。
履歴を削除すると対応するキーも削除します。スコア値・BEST・リプレイは移行で変更しません。
旧版へ戻す場合は、書き込みを停止して整合したscore.db / network.dbのbackupへ復元し、
schema versionだけを巻き戻しません。送信済みの識別情報と再送本文の保持は
[IRの仕様](ir.md)を参照します。

単曲IRは、通常の履歴保存が成功して`score_history_id > 0`になった後でだけenqueue
されます。実効アシスト、autoplay、replay、Practice、保存禁止のkey mode変換はこの時点より
前に除外されるため、`send_policy=Always`でも送信されません。7K規則の`SevenToNine`だけは
元の7K譜面として送信候補になります。

| `send_policy` | BMZの単曲条件 | beatorajaの単曲条件 |
| --- | --- | --- |
| `Always` | 常に送信候補 | 常に送信候補 |
| `CompleteSong` | 最終gauge valueが0より大きい | 最終gauge logが0より大きい |
| `UpdateScore` | 初プレイ、またはEX / lamp / max combo / BP / CBのいずれか改善 | EX / lamp / max combo / min BPのいずれか改善 |

BMZのCB改善だけでも`UpdateScore`が送信候補になる点が差です。これはclient側の
送信量制御であり、serverのbest更新規則とは別です。

その後、provider固有条件を適用します。現行rianIR providerは`BATTLE` / `BATTLE AS`を
除外し、譜面時間の妥当性も検査します。BMZ公式IRとBMS-IRはBATTLE bucketを表現できます。
BMS-IRはBeatoraja/LR2oraja ruleと実効LN/CN/HCNも分離し、DXは送りません。

## アシスト時の補助データ差

単曲の目に見える主要動作は、BMZもbeatorajaも「assist lampとplay / clear count
だけをbestに反映し、数値・replay・IRを更新しない」で一致します。ただし内部の補助
データは同一ではありません。

beatorajaの`PlayDataAccessor.writeScoreData(..., updateScore=false)`は、lampと回数に加え、
最新play date、lamp改善時のscore log、全プレイのscore data log、player全体の
判定数・play count・clear count・play timeを更新します。また単曲trophy判定自体は
`updateScore`で抑止されず、gauge条件によってEASY trophyが追加され得ます。

BMZのclear-onlyもprofile全体の`player_stats`へ、判定数・play / clear count・
play time・max comboを加算します。一方、`score_history`、リプレイ、IRは作らず、既存
best行の`played_at`も更新しません。日次統計は`score_history`から算出するため
アシストプレイを含まず、単曲trophy DBも現在持ちません。player totalsは一致したが、
日次統計・play log・trophy・最新日時には引き続き互換差があります。

## コース

コースは全ステージの実効アシストの最大値を使います。途中FAILEDはアシストより優先して
最終lampを`Failed`とします。完走時はAssistが1曲でもあれば`AssistEasy`、
Light Assistだけなら`LightAssistEasy`になります。この優先順位はbeatorajaと一致します。

| コース条件 | BMZの保存・送信 |
| --- | --- |
| 通常手動、全譜面解決済み | course attemptと数値を保存し、条件を満たすstage履歴リンク・replay・trophyも保存します。release courseならIR jobをenqueue |
| いずれかのstageが実効assist | 各stageの曲ランプ・回数・player statsを更新し、数値を0としたcourse attemptでlampと回数だけ保存します。stage履歴リンク、replay、trophy、IRなし |
| autoplay / replay playback / key mode変換 | course attempt自体を保存せず、IRなし |
| FAILED | 未プレイ譜面を含めたBPで`Failed` attemptを保存します。非アシストなら通常の数値保存・IR候補 |

コース2曲目以降の開始前に手動で中断した場合も、完了済みステージだけで`Failed` attemptを保存します。
未開始曲のstage結果・プレイ回数・replayは追加せず、コース全体のノーツ数と未プレイ分のBPを使います。
1曲目の開始前キャンセルではcourse attemptを作りません。Autoplay / Replay等の保存除外条件は上表に従います。

BMZのコースIRはproviderの単曲`send_policy`を参照せず、course definitionが
`release=true`でprovider条件を満たせばenqueueします。beatorajaもCourseResultの
`CompleteSong` / `UpdateScore`分岐が実質無効で、`updateCourseScore`とreleaseのみで
送信可否が決まるため、この点は一致します。

保存構造は異なります。beatorajaはcourse best行をclear-only更新するのに対し、BMZは
assist courseも数値0のattempt行として残し、best scoreとbest lampを別クエリで
選びます。表示上のbest数値を汚さないことはテストで固定しています。

## レビュー所見と今後の判断

優先度順の未解決事項は次のとおりです。

1. **BATTLEの仕様選択**: beatoraja準拠のLight Assistに戻すか、BMZ固有の専用
   score bucketとして維持するかを明示します。後者ならBMZ公式IRのランキングも
   DOUBLE bucketで必ず分離します。
2. **アシスト履歴・日次統計**: beatorajaのscoredatalog相当を必要とするか決めます。
   profile全体統計とは分離したassist attempt logを追加する余地があります。
3. **通常bestと`UpdateScore`**: CBと同EX scoreのtie-breakをBMZ固有仕様として
   維持するなら、その差をIR server / UIと共有します。

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

- アシストと配置のscore flag: `.local/beatoraja/src/bms/player/beatoraja/play/BMSPlayer.java`
- modifierのassist level: `.local/beatoraja/src/bms/player/beatoraja/pattern/`
- 単曲・コースIR / replay: `.local/beatoraja/src/bms/player/beatoraja/result/MusicResult.java`、
  `CourseResult.java`
- DB更新: `.local/beatoraja/src/bms/player/beatoraja/PlayDataAccessor.java`、`ScoreData.java`
