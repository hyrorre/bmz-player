# スコア保存・アシスト・IR送信条件

この文書は、通常プレイ・アシスト・譜面オプション・特殊モードについて、
BMZがクリアランプ、数値ベスト、履歴、リプレイ、IRを更新する条件を整理します。
beatoraja互換を判断するときはこの文書を入口とし、IR固有の詳細は
[BMZ IR](ir.md)と[rianIR互換プロバイダー](rian-ir.md)、新規単曲・コース各曲送信と段位更新の[Bokutachi](bokutachi.md)を参照します。
KaleidBeat拡張の結果の扱いは[KaleidBeat拡張の結果](#kaleidbeat拡張の結果)にまとめています。

レビュー基準日は2026-08-27です。BMZの現行実装と、`.local/beatoraja/`の
参照ソースを静的に比較した結果です。未解決の差は「互換差分」に明記します。

## 判定リビジョンと処理済み数

判定数・BP・コンボ・ゲージと、消費した採点ノーツ数は別に集計します。
たとえば9Kで同じノーツに非消費BAD、続いてPGREATが出ると、BADとPGREATを各1回、
処理済み数は1件としてghostにPGREATを記録します。FAILED時の保存BPは
ライブBPに総採点ノーツ数と処理済み数の差を加えます。ジャストは保存・IRの判定内訳ではFAST側です。

新規BMZリプレイには `judge_revision = 4` と記録時の実効窓倍率を保存します。
リビジョン4では、次の変更をまとめて適用します。

- HCNとHLNのゲージは入力と譜面の境界時刻で区切って計算し、増減を譜面時刻順に適用します。
- 負の入力補正を使うときは、譜面の種類を問わず見逃しや地雷、長押しのゲージの採点を遅らせます
  （[入力と時計](gameplay-runtime.md#入力と時計)）。通常の入力は補正済み判定時刻だけで再生できます。
- 入力の判定時刻まで既に採点を終えていた場合は、その境界を入力ごとの `applied_after` に保存し、
  再生でも境界の後にその入力を渡します。項目のない旧記録からは、遅れて届いた入力の採点境界を復元できません。
- 分岐評価には `score_position`（採点済み時刻と処理済み入力数）を保存します。本人・対戦相手の再生で、
  その位置まで入力を渡してから記録済みの枝に切り替えます。項目のない旧記録では分岐の直前まで採点して再生します。

リビジョン3では、実効HCN本体内の空POOR抑止を本人・対戦相手共通の判定処理で適用します。
リビジョンなしの既存リプレイは0として読み込み、DX POPでは当時の窓を使います。
既存のスコア・ランプ・履歴値は保存時の値を維持します。旧リプレイの再採点では
現行のノーツ消費・地雷・HCNの空POOR抑止とゲージ計算を使うため、当時の結果と異なる場合があります。
再生結果から過去の記録やIRスコアを更新しません。

全ノーツ処理後の早期退出と手動中断では、終了時刻（`ended_at`）と最後に採点を終えた時刻（`scored_through`）を保存します。
初回の採点前に中断した場合は `scored_through = -1` です。この項目がない旧リプレイは終了時刻まで採点します。
採点の終端が1msの採点間隔の途中にある場合も、その時刻の入力・HCNゲージ・地雷の通過まで反映します。
実プレイは入力オフセットを加えた判定時刻に入力を処理し、リプレイは記録した判定時刻で入力を渡します
（[入力と時計](gameplay-runtime.md#入力と時計)）。終端時刻までに判定時刻を迎えなかった入力は記録しません。

## 用語

- **ランプ更新**: `score_best.clear_type`と対応するgauge情報の更新です。
- **数値更新**: EX score、BP、CB、max combo、判定内訳、ghostなどの更新です。
- **履歴保存**: BMZの`score_history`に1プレイ分を追加します。
- **clear-only**: ランプ、曲別play / clear count、profile全体のplayer statsを
  更新し、数値ベスト・履歴・リプレイ・IRを更新しない保存経路です。
- **実効アシスト**: 設定がONであるだけでなく、その譜面または判定窓に実際の
  効果が発生したアシストです。DXで100%を超えるEXPAND JUDGEは上流の保存条件に従いAssist扱いです。BMZの`AssistRuntime.effective_mask` / `level`に相当します。

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

プロファイルの累計プレイ時間には、譜面時刻0から完走または中断までの再生時間を加算します。完走時は譜面の長さ、手動中断・途中FAILED・採点確定前の早期退出では譜面の末尾を上限とした停止時刻を使います。どちらもFREQの再生速度で実時間に換算し、秒未満を切り捨てます。例えば120秒の譜面をFREQ +12（2倍速）で遊ぶと、119秒地点での中断は59秒、完走は60秒です。保存対象の単曲とコース各曲に適用します。

単曲の履歴リプレイを書けない場合も、スコア・ランプ・回数を保存します。自動保存とリザルトでの手動保存は、どちらもプレイごとの履歴リプレイを参照します。スロットの更新に失敗した場合は、そのスロットの前の記録を保持します。リザルトではリプレイの保存失敗を通知します。旧形式のスロット専用ファイルは、既存の参照から引き続き再生できるよう保持します。

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
両側のスクラッチは[採点の対象外](#auto-scratchの採点)です。リプレイには人間の入力を
記録し、再生時も両側のスクラッチを採点から外します。

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

EXTRA NOTEは[BMZ独自の配置制限](extra-note.md)を使います。DEPTHが正でも追加ゼロなら通常保存・IR候補です。
その場合は履歴のassist maskにもEXTRA NOTE bitを残しません。実際に追加された場合だけ
Assistのclear-only経路を使い、FAILEDを含むランプ・回数を保存します。

AUTO SCRATCHはLR2 / LR2oraja Endless Dreamの同名オプションに合わせます。両者ともスコアを保存せず、
クリアはイージー（Endless DreamはASSIST EASY）扱いにします。リプレイ再生では自動演奏せず、
全体AUTOPLAYではもともと保存しません。DP OPTIONの`BATTLE`と併用した場合は`BATTLE AS`
として専用bucketに通常保存し、アシスト扱いにしない（LR2もD-BATTLE中のAUTO SCRATCHを
イージー扱いにしない）です。CLIの`--auto-scratch on`は従来どおりスコア・リプレイを保存しません。

### AUTO SCRATCHの採点

AUTO SCRATCHとBATTLE ASは、beatoraja / LR2oraja Endless Dreamの`AutoplayModifier`と
同じく、自動演奏するスクラッチレーンのノーツを採点の対象外にします。全譜面オプションを
適用した後に変換するため、配置でスクラッチへ移ったノーツも対象です。

- 判定・コンボ・EXスコア・ゲージの増減・BP・総ノーツ数に含めません。地雷のダメージもありません。
- キー音は、ノーツの位置でBGMとして鳴らします。見えないノーツはレーンに残します。
- Kaleidのフリーゾーンは区間ごと外します。
- ノーツは表示だけに残し、[非表示の設定](settings.md#auto-scratchのノーツ表示)で隠せます。
  beatorajaはノーツを表示しません。LR2はAUTO用のノーツ画像で表示し、判定しません。

BMZ v0.5.3までにプレイした`BATTLE AS`は、スクラッチをPGREATとして数えています。score DB
migration v41は、これらを履歴に残したまま集計から外します。

- `BattleAutoScratch` bucketのBMZでプレイした履歴を`normal_best_eligible = 0`にします。ベスト・ランプ・
  曲別のプレイ回数とクリア回数・デバイス別ベスト・LR2のクリアしたオプション
  （option 118..149）の集計に使いません。プロファイル全体の統計と日別の統計には残します。
- beatorajaなどから取り込んだBATTLE ASは、残る履歴からベスト・ランプ・曲別回数を再集計します。
  開発版のv40で除外された取り込み履歴も復元します。除外済みの旧BMZ履歴を重複整理で削除した場合、
  プロファイル全体の統計から削除分を引き、曲別回数は維持します。
- ローカルライバルの参照先にもv41以降が必要です。v39・v40ではプロファイルの更新を案内します。
- replay slotはリプレイを残し、比較用の値（EX・BP・CB・最大コンボ・ランプ）を空にします。
  slotの規則が無効でなければ、次のプレイで更新されます。
- 旧BMZ履歴はIRの一括送信から外します。network DB migrationは、旧BMZ採点の送信待ち・失敗・
  送信中のBATTLE ASジョブを`ineligible`にします。v11はv9/v10で除外された取り込みジョブを
  送信待ちに戻します。旧BMZ採点のジョブを個別再送しても、送信前に`ineligible`に戻ります。
- BMZ IRに送信済みの旧BMZ採点はbestとランキングから外します。D1 migration 0021は、0020で
  除外された取り込み記録を復元し、有効なBATTLE ASのbestを再構築します
  （[IR](ir.md)の`auto_scratch_scoring`）。BMS-IRに送信済みの記録は変えられません。
- コースの記録（`course_scores`）は変えません。

元に戻す場合は、対象の履歴の`normal_best_eligible`を1へ戻し、ベストを履歴から集計し直します。

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
この保存・送信の仕様はBMZ独自仕様として維持します。

### H-RANDOMの閾値

H-RANDOM / ALL-SCRの鍵盤は、同じレーンの直前ノーツから閾値以内の間隔にならないよう配置します。
閾値はbeatorajaの既定`hranThresholdBPM = 120`を`ceil(15000 / BPM)`で換算した125msです。
境界と等しい間隔も避けます。7K→6K変換のスクラッチ・KEY4の移動先、7K→8K変換のスクラッチの移動先と、
7K→9K変換のスクラッチ振り分けも同じ閾値を使います。

新しいプレイは閾値をreplayへ記録します。閾値の記録が無い過去のreplayでは、
当時の配置を再現するため、H-RANDOM / ALL-SCRは100ms、7K→9Kは125msのまま扱います。
beatorajaのreplay取込は`config_player.json`の`hranThresholdBPM`から閾値を求め、
見つからなければ125msを記録します。

### 判定アルゴリズムと入力時刻

BMZで保存するリプレイに、プレイ時の判定アルゴリズムを記録します。beatorajaからの取込では
`.brd`の`config.judgetype`を記録し、`config`内で省略された場合はbeatoraja既定の`Combo`を使います。
`config`がない場合や未対応の値の場合は、再生時のプロフィール設定を使います。

beatorajaの入力時刻は先頭ノーツを1秒まで繰り下げる余白を除いて記録されています。
インポート済みリプレイの再生・動画出力では、再生対象の譜面に適用した余白を入力時刻にも足します。

#### 既存リプレイの更新と旧版での再生

修正前に取り込んだ単曲・コースのリプレイは、元の`.brd`を再インポートすると更新されます。
最初の再インポートでは、先頭余白が0 msの譜面も含め、対象ファイルを書き直します。
更新した件数は「置換」に数えられます。上書きを指定していない場合は、BMZで保存したスロットを保護します。
コースは取り込んだ全ステージを保存してからスロットを切り替えます。途中で変換や保存に失敗した場合は、以前のコースリプレイを再生できます。

修正前のBMZは、リプレイに記録した判定アルゴリズムの代わりにプロフィール設定を使います。
また、インポートしたリプレイは、先頭余白がある譜面で入力がその分早く再生されます。

## Key mode 変換

BMZは`SpToDp`、`SevenToSix`、試験的な`SevenToEight`を`score_save_disabled`とし、
ランプ・回数を含めて何も保存しません。`SevenToNine`は`seven_to_nine_rule_mode`で規則を選びます。

- `7K`（既定）: 表示と入力は9Kへ変換するが、判定窓・ゲージ・スコアidentityは元の
  7Kを使います。通常7Kとしてランプ・スコア・replayを保存し、IR送信候補にできます。
  replayの入力レーンも通常7K形式で保存し、再生時は現在の7K→9K設定へ投影します。
- `9K`: 9Kの判定窓・ゲージを使い、ランプ・回数・スコア・replay・IRをすべて
  保存しません。Light Assistのclear-only保存にはしません。

replay再生時は、replayを保存しない`SpToDp`、`SevenToSix`、`SevenToEight`、9K規則の`SevenToNine`を
無効化します。7K規則の`SevenToNine`だけは変換を維持し、通常7K形式で保存された入力を
現在の7K→9K設定へ投影します。

beatorajaの7K→9K `ModeModifier`はLight Assistであり、数値・replay・IRは更新しないが
`LightAssistEasy`ランプとplay / clear countは更新します。BMZの`9K`規則はそれより
厳しい完全非保存、`7K`規則は元譜面のルールとidentityを維持するBMZ独自仕様です。

### 7K→8Kの配置

元譜面の鍵盤をフレーズごとに左右へ移します。例えば、同じ小節の前半の
`1→2→3→4`をそのまま、後半の`7→6→5→4`を`8→7→6→5`に置くと、元の階段の形を保って8本すべてを使えます。

同時刻の鍵盤を一組として読み、間隔250ms以内の縦連、2組の交互打ち、
隣の鍵へ一段ずつ動く階段をまとまりとして扱います。小節線や途中のスクラッチで分割しません。
それ以外の並びは最大4打点までまとめ、次のパターンの始まりや250msを超える休符で区切ります。

高密度の和音列では、使う鍵の左右順を保ちながら、鍵間隔を1か所だけ1レーン広げる配置も候補にします。
これにより、元の7鍵がすべて登場するフレーズでも中央のレーンを空けられます。
対象は2打点以上、異なる鍵が6本以上、合計8打以上で、最初の打点から先の密度が毎秒16打以上の和音列です。
単音の階段やトリルは形を維持します。幅広い同じ和音または2組の交互和音が16打点以上続く場合は、
8打点ごとに配置を選び直し、末尾の8〜15打点はまとめます。

各フレーズで使用する鍵の順序と間隔を優先し、同時のスクラッチにも場所を割り当てます。
鍵盤・スクラッチのLNとフリーゾーンは終点まで同じレーンを確保します。
保持中の鍵で平行移動できない場合は鍵間隔のずれが小さい候補を選びます。
許容範囲の候補では、境界で生じる縦連を避け、すぐ繰り返す同じフレーズは同じ指で弾ける配置を優先します。
高密度の和音列の反復では、同じ指を優先する期間を8打点を目安に区切ります。
そのうえでレーン別の打鍵履歴、元の鍵盤位置、前の配置からの移動量を合わせて評価し、同点なら元の鍵間隔を優先します。
履歴は配置予定の通常ノーツとLN始点を数え、4小節ごとに0.75倍します。元の小節線が2本未満の場合は8秒ごとです。

スクラッチの配置先は実際の鍵盤・LN・フリーゾーン・前後の打鍵と照合します。
予定のレーンが使えないときは他の空きレーンを試し、H-RANDOM閾値を満たす場所がない場合は音をBGMに回します。
スクラッチレーンの不可視ノーツは配置もBGM化もせずに取り除きます（7K→6K変換と同じ）。鍵盤レーンの不可視ノーツは、その時点の配置に従って移します。

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
nullableな`score_history.source_min_bp`へ別に保存します。
最小BPを達成した架空の履歴は追加しません。元DBに独立最小CBは無いため、
履歴CBは判定内訳からの計算を維持します。最大コンボとランプの取り込みも従来どおりです。

処理済みノーツはPGREAT〜POORの判定数の和（総ノーツ数を上限）から求め、残りを
履歴BP・CBに加算します。beatoraja系のEmpty Poorはノーツを消費しません。
集計行のランプがクリア済みでも、最高EXの判定内訳は途中終了時のものかもしれないため、
この補完を行います。例えば2006ノーツ中GREAT 1・POOR 2なら、BP・CBは2005です。

LR2形式はPoorにEmpty Poorを含むため、処理済みノーツの和は上限の推定値です。
既知のsource minbpがあれば、それをLR2行のBEST BPへの寄与として採用し、推定BPで
下回らせません。不明なら判定由来BPを使います。LR2のPoorのうち、他の判定を除いた
残ノーツ数を超える分だけはEmpty Poorと確定できるため分離し、両者の合計を維持します。
それ以外の判定内訳は変更しません。
LR2のCBは未処理分を含めて「総ノーツ数 − PGREAT/GREAT/GOODの合計」で計算し、
Empty Poorをコンボ切断に含めません。LR2の正確な履歴BPや、ランプから確定できない
独立した最小CBは元DBだけでは復元できません。

外部スコアの`FullCombo` / `Perfect` / `Max`ランプはCB 0を達成した証拠として扱い、
独立BEST CBだけを0にします。履歴CB・判定内訳・代表履歴の比較には補完前と同じ
判定由来の値を使います。根拠は保存済み履歴のランプに残し、通常ベスト対象の履歴を
削除した場合は残存履歴から再集計します。Empty Poorがあり得るため、Perfect / Maxでも
ランプだけを理由にBPを0にしません。

通常保存と履歴cleanup後の再集計は、各履歴のBPと、判明しているsource minbpの
両方から最小値を取ります（LR2の既知source minbpは上記の例外）。例えばEX 2100の判定内訳BPが20、元DBのminbpが5なら、
履歴BPは20、source minbpは5、BEST BPは5になります。既存BMZ BEST BPが3なら3を
維持します。代表履歴のタイブレークには履歴BPの20を使い、source minbpは使いません。

ローカルプレイと不明な値は`source_min_bp = NULL`です。LR2の負の未記録値（-1）、
beatoraja / LR2oraja系の`Integer.MAX_VALUE`、SQLのNULLは独立BPとして使いません。
BP 0は有効な記録です。Empty Poorを含むため、BPがノーツ数を超えるという理由だけでは
無効にしません。beatoraja形式で`score`がなく`scoredatalog`だけある場合も、既存の
fallback経路で取り込みます。この場合のminbpは、その行のプレイに対応する元の値です。

同じインポートを繰り返した場合、source minbpが未保存なら補完し、小さくなった場合は
改善を採用します。後の元DBで悪化・不明になっても、以前確認できた値を維持します。
LR2で未知だったsource minbpを初めて取得した場合は、過小な推定BPを置き換えるため
BEST BPが増える場合があります。他の履歴のより良いBPは維持します。
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

score DB migration v37は旧取り込みの未処理ノーツ欠落を一度だけ修復します。
外部DB由来で、BP・CBが未補完の判定合計と一致する単曲履歴を対象とし、保存済み判定数から
残ノーツを加算します。影響するScoreKeyのBEST BP・CBを、通常ベスト対象の残存履歴と
source minbpから再集計するため、誤って小さかった値は増える場合があります。
ローカル履歴・IR取得履歴・コース履歴は対象外です。LR2の確定できるEmpty Poor分離を
履歴とその履歴を代表とするBESTにも適用します。履歴ID、他の判定内訳、代表履歴、ghost、
replay、ランプ、回数、IR識別情報、過去時点のprevious-best snapshotは維持します。
移行はトランザクション内で実行され、失敗時は変更とバージョンを戻します。
既に失われたsource minbpや由来がLocalと記録された旧インポートは推測修復しません。
修正版起動前のscore.dbをバックアップすれば旧版へ戻せます。

score DB migration v38は、通常ベスト対象の外部履歴にFC以上のランプがあるScoreKeyの
BEST CBだけを0へ修復します。v37適用済みのDBも対象です。履歴のCB・BP・判定内訳、
BESTの他の値や代表履歴は変更しません。ランプだけのclear-only行や通常ベスト対象外の
履歴、ローカル履歴を根拠として補正することはありません。

上流コードと形式ごとの確認範囲は[調査記録](../notes/2026/2026-11-04-import-source-min-bp.md)を参照します。

### beatoraja系の累計プレイ統計

スコアインポートでは、beatoraja / LR2oraja EDの `score.db` にある
`player` の最新日付の1行を累計として読み取ります。累計プレイ・クリア回数、秒単位の
プレイ時間、判定内訳、最大コンボを取り込みます。譜面が未登録でも累計を取り込めます。

score DB migration v39は、プロファイルごとに1件の累計を保存する
`imported_player_stats` を追加します。このテーブルを先行版のv37で作成したDBでは、
保存済みの累計を保持しながら、未適用のBP・CB修復を実行します。
有効な累計を再インポートすると保存値を置き換えます。
`player` がないスコアDBと履歴用の `scoredatalog` は楽曲スコアのみ取り込みます。
累計の読取に失敗した場合は、スコアの結果とともにエラーを表示します。

プレイヤー統計を取得するとき、既存の `player_stats` に含まれるbeatoraja系の
スコア行のプレイ回数・クリア回数・判定内訳を差し引いて、元DBの累計を足します。
履歴を持たないBMZのclear-only分も保持します。プレイ時間は保存済みの時間と
元DBの時間を足し、最大コンボは大きい方を採用します。出所が `Local` の旧移行行は
通常プレイと同様に扱います。日別統計はBMZでのプレイ履歴から算出します。

既存プロファイルへのDB移行では空の表を追加します。以前に取り込んだプロファイルは
元の `score.db` を再インポートすると、累計を補完できます。

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
| `CompleteSong` | 手動中断しておらず、最終gauge valueが0より大きい | 最終gauge logが0より大きい |
| `UpdateScore` | 初プレイ、またはEX / lamp / max combo / BP / CBのいずれか改善 | EX / lamp / max combo / min BPのいずれか改善 |

BMZのCB改善だけでも`UpdateScore`が送信候補になる点が差です。これはclient側の
送信量制御であり、serverのbest更新規則とは別です。

その後、provider固有条件を適用します。現行rianIR providerは`BATTLE` / `BATTLE AS`を
除外し、譜面時間の妥当性も検査します。BMZ公式IRとBMS-IRはBATTLE bucketを表現できます。
BMS-IRはBeatoraja/LR2oraja ruleと実効LN/CN/HCNも分離し、DXは送りません。

## KaleidBeat拡張の結果

KaleidBeat拡張の譜面と、拡張に部分対応した結果も、本書の既存のモード・アシスト制限を満たせば
ローカル保存とBMZ公式IR送信の対象です。
score.db 36の`score_kaleid_plays`、Replay 8の形式、provider別の条件は[KaleidBeat拡張](kaleid.md)を参照してください。

- 原本の譜面ハッシュと、既存のLN・rule等の区分へ集約します。
- 動的分岐で採択した枝の違いでは、ランキングを分けません。
- 独立したベストごとに、元のプレイと履歴の証拠を保持します。

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
コースリザルトのCBにも途中終了曲の未処理ノーツと未開始曲の全ノーツを加算し、空POORは除きます。
保存済みのFAILEDコースベストは、プレイした全ステージの履歴が紐付いていれば、コースのBPと空POOR数から同じCBを復元します。
2026-07-16より前のattemptはBPに判定数だけを保存しているため、紐付いたステージのCB合計を下回る値にはしません。
履歴が一部のステージにしか紐付かない場合は、紐付いたステージのCB合計を使います。
1曲目の開始前キャンセルではcourse attemptを作りません。Autoplay / Replay等の保存除外条件は上表に従います。

BMZのコースIRはproviderの単曲`send_policy`を参照せず、course definitionが
`release=true`でprovider条件を満たせばenqueueします。beatorajaもCourseResultの
`CompleteSong` / `UpdateScore`分岐が実質無効で、`updateCourseScore`とreleaseのみで
送信可否が決まるため、この点は一致します。

Bokutachiのコース各曲は単曲の送信方針に従い、原則NO PLAY、フルコンボ系だけFULL COMBOとして送ります。
COMPLETE SONGはその曲の実際の完奏状態を使います。9Kは各曲に保存された入力機器別に送信・BEST比較します。
PMSのUPDATE SCOREは通常BEST更新対象の`source_kind=Local`履歴だけを比較し、外部プレイヤー由来の取り込み成績を除外します。
今回保存した履歴を除き、EX・ランプ・コンボ・BP・CBを独立に集計します。全体BESTと他IRの比較元は変えません。
Bokutachiのコース全体は合計スコアを登録せず、適格なBMS 7K／14K・4曲クリアだけ段位更新を行います。
この段位更新には単曲・コース合計のBEST更新や各曲送信の成功を要求しません。
条件・段位APIの配置制限・未対応範囲は[Bokutachi](bokutachi.md)を参照してください。

保存構造は異なります。beatorajaはcourse best行をclear-only更新するのに対し、BMZは
assist courseも数値0のattempt行として残し、best scoreとbest lampを別クエリで
選びます。表示上のbest数値を汚さないことはテストで固定しています。

## レビュー所見と今後の判断

優先度順の未解決事項は次のとおりです。

1. **アシスト履歴・日次統計**: beatorajaのscoredatalog相当を必要とするか決めます。
   profile全体統計とは分離したassist attempt logを追加する余地があります。
2. **通常bestと`UpdateScore`**: CBと同EX scoreのtie-breakをBMZ固有仕様として
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
