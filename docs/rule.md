# ルールモード

BMZの`RuleMode`は、判定窓、判定ランク、ゲージ、コンボ、score / replay
の保存キーを切り替えるための設定です。profileの`[play].rule_mode`に保存し、
score DB / replay slotでも別条件として扱います。

現在の値:

- `Beatoraja`
- `Lr2Oraja`
- `Dx`
- `Legacy` (画面では LEGACY)

互換確認の基準は次のコミットです。

- beatoraja `93e704d9eedcb3b52e558f6581c00d6077ceb639`。[JudgeManager](https://github.com/exch-bms2/beatoraja/blob/93e704d9eedcb3b52e558f6581c00d6077ceb639/src/bms/player/beatoraja/play/JudgeManager.java)
- LR2orajaの `lr2oraja` ブランチ `c1c8e4e0148abd318a9375de1631227cc383d0e4`。[JudgeManager](https://github.com/wcko87/lr2oraja/blob/c1c8e4e0148abd318a9375de1631227cc383d0e4/src/bms/player/beatoraja/play/JudgeManager.java)
- LR2oraja Endless DreamのDX MODE `3bc4b9b40dc6a44f7ebf0594a6ec1b8ffb7370fe`。[JudgeManager](https://github.com/rian-eimu/lr2oraja-endlessdream/blob/3bc4b9b40dc6a44f7ebf0594a6ec1b8ffb7370fe/core/src/bms/player/beatoraja/play/JudgeManager.java)

## モードの概要

| item | Beatoraja | Lr2Oraja | Dx |
| --- | --- | --- | --- |
| 主な参照 | beatoraja | LR2oraja | LR2oraja Endless Dream DX MODE |
| 判定property | key mode別`JudgeProperty` | `JudgeProperty.LR2` | 9K（PMS 5K含む）は`JudgeProperty.POP`、それ以外は`IIDX` |
| ゲージproperty | key mode別`GaugeProperty` | 既定は`GaugeProperty.LR2`、コース・Practiceの明示指定を優先 | 9K（PMS 5K含む）はPOP用、それ以外はIIDX用gauge |
| `#RANK`未指定 | 75%（PMSは70%） | 75% (`#RANK 2`相当) | 固定窓のため無視 |
| `#RANK 4` | 125% (`PMS`は133%) | 75% (`#RANK 2`相当) | 固定窓のため無視 |
| `#EXRANKxx` / chA0 | importはするがruntimeでは無視 | importはするがruntimeでは無視 | 固定窓のため無視 |
| MultiBad | 無効 | 有効 | 有効 |
| LN start late BAD抑制 | 無効 | 有効 | 有効 |
| BAD消費 | 消費 (`PMS`だけ非消費) | 消費 | 9Kは非消費、それ以外は消費 |
| EmptyPoor combo break | 5K / 10K / 9Kだけbreak | 継続 | 9Kはbreak、それ以外は継続 |
| CN/HCN早離し猶予 | 9Kは200ms、それ以外は0 | 0 | 9Kは116,667μs、それ以外は0 |
| score / replay保存 | `rule_mode=Beatoraja` | `rule_mode=Lr2Oraja` | `rule_mode=Dx` |

DX MODE固有のIR抑制はBMZ公式IR / rianIRには適用しません。
BMS-IRは対応rule modeの制約によりDXを送信対象にしません。

## LEGACY

LEGACYは従来のLR2系に近い単曲判定を選べるモードです。タイミング判定には入力とノートをそれぞれ整数ミリ秒に変換した値を使います。ただし見逃しと地雷の判定は、OpenLR2と同じく整数ミリ秒の時刻と倍精度のノート時刻を比べます。整数ミリ秒のわずかに手前の時刻では、倍精度の引き算の丸めにより、µs単位の時刻で比べた場合より1ミリ秒遅く確定することがあります。キーとスクラッチには同じ判定幅を適用します。

LEGACYのBMS読み込みは、OpenLR2（`LR2_bmsload.cpp`の`ParseBmsFile`）と同じ手順で譜面の本文を読み直します。

- 行は`fgets`と同じく102,399文字ずつ読みます。それより長い行は途中で切れ、残りは別の行として扱います。UTF-16の譜面は行が`#`で始まらないため、何も読み込みません。
- 全チャンネルの0以外のオブジェクトを、ファイルの順に1つの配列へ入れます。値の変換はOpenLR2と同じく、進数の範囲外の文字を0として数えます（無効なオブジェクトにはしません）。チャンネル番号の上の桁は16進数として読み、`G`以降の文字は0です。末尾には小節線のオブジェクト（0〜999小節）と、BGAのない譜面ではBGAの基本レイヤを1つ加えます。
- チャンネルは`channelConvert`で変換します。`x6`はスクラッチ、`x7`は無視、`x8` / `x9`は6・7鍵です。`.pms`と`#FP/DSC`の譜面もOpenLR2の変換表のとおりに変換します。
- 配列はUCRTの`qsort`を移植した並べ替えで、`CMP_NotesByBmsTiming`の順に並べます。この比較関数は位置の差を1/100000小節単位に切り捨てるため、1行を数千以上に分割した譜面では、並び順が位置の順になりません。BMZは同じ並べ替えの手順を再現するので、並び順もOpenLR2と同じになります。
- この並び順でオブジェクトをたどり、位置が変わるたびに`240 / BPM * 小節長 * (位置の差) * 1000`ミリ秒を倍精度で加えます。位置が戻る場合は負の差を加えます。0小節目の通常・不可視ノートによる1小節の挿入も再現し、通常のBMZの1秒余白は追加しません。
- 拡張BPM（`#BPMxx`）とSTOP（`#STOPxx`）は譜面の進数で番号を読み、`#BPMxx`は`atof`、`#STOPxx`は`atol`で値を読みます（0以下のSTOPは無視）。未定義の番号は-1として扱うため、拡張BPMはBPM 1、STOPは負の停止になります。STOPの長さは、次に位置が変わるときに`(値 / 192) * (240000 / BPM)`で計算し直して加えます。`#BPM`のない譜面は、OpenLR2ではBPM 0（時刻が無限大）になりますが、BMZはBMZの読み込みのBPMを使います。
- キーモードは、LR2が曲を登録するとき（`LR2_songmanage.cpp`の`ParseBMSMETA`）と同じ規則で決めます。5鍵から始め、データ行のチャンネル番号だけを見ます。オブジェクトがすべて`00`の行も数えます。`x8` / `x9`（不可視・LNを含む）で7鍵、2P側のチャンネルでダブルになり、`28` / `29`は14鍵です。`#IF 1`以外の`#IF`の中の行は数えません。`#4K` / `#6K`などのBMZの判定は使いません。

判定幅には`#RANK`を使い、`#DEFEXRANK`と通常BMSの`#LNMODE`によるCN/HCN指定は適用しません。新規プレイのBMS分岐・対応するRANDOM配置にはLR2乱数を使い、seedをリプレイへ保存します。

プレイするノートもOpenLR2と同じ手順で組み立てます（`.pms`と`#FP/DSC`の譜面を除く）。

- LNチャンネルの組と`#LNOBJ`の終点は、上の時刻計算と同じ並び順で、鍵ごとに1つの「始点待ち」を共有します。LNチャンネルのオブジェクトは、`#LNOBJ`の終点を待っている通常ノートも始点として閉じます。始点待ちのない`#LNOBJ`のノートは通常ノートとして残ります。終点のないLNは通常ノートとして判定します。
- その後、配列を`CMP_NotesByRealTiming`（時刻の差を整数ミリ秒に切り捨てる比較）で同じ`qsort`により並べ替え、その順にレーンへ置きます。レーン内は時刻順に並べ直しません。差が1ミリ秒未満のノートは時刻と逆の順になることがあり、判定もその順に進みます。
- OpenLR2は`realTiming < realTiming_ln`のオブジェクトをLNとして処理し、LNでないオブジェクトの`realTiming_ln`は-1です。そのため、負のSTOPなどで-1ミリ秒より前になったノートと地雷は、-1ミリ秒に終点のあるLNになります。
- レーンごとに、直前のLNの範囲内（終点の時刻を含む）のオブジェクトは捨てます。同じ時刻の2つ目のオブジェクトも捨てますが、それがLNの始点なら先のノートと置き換えます。地雷もこの処理に加わり、ノーツ数には数えません。
- 捨てたノートの音は鳴らしません。

| BMSの #RANK | PGREAT | GREAT | GOOD |
| --- | ---: | ---: | ---: |
| 0 | 8 ms | 24 ms | 40 ms |
| 1 | 15 ms | 30 ms | 60 ms |
| 2、4、未指定 | 18 ms | 40 ms | 100 ms |
| 3 | 21 ms | 60 ms | 120 ms |

BADは200 ms以内です。早押しでBAD範囲外かつ1,000 ms未満なら空POORとなり、ノートは残ります。タップにBADが付くとそのノートを消費し、同じ押下で次のノートも通常判定に入る場合は続けて判定します。LNは終点よりGOODの幅を超えて早く離すとBADになり、それ以外では始点の判定を使います。

判定エンジンは、押下を評価する時点で同じレーンの先頭ノートが期限切れなら、そのノート1個をPOORとして押下を消費します。タップBADでの後続判定とは区別します。

通常ノートと標準LNのセッションでは、1Pスクラッチ・1P鍵盤・2Pスクラッチ・2P鍵盤の順で入力と見逃しを処理します。アイドル状態のレーンでは1回のポーリングで1ノートまで見逃しを確定します。全レーンを処理してから閉店を判定するため、同じポーリング内のBAD連鎖や別レーンの打鍵も採点します。保持中LNの再押下は始点の仮判定を更新し、大きく遅れた再押下は空POORになります。

BMZは基本的に1ms周期で進行し、同じ整数ミリ秒内に後から届いた入力も処理します。過去のOpenLR2リプレイには、判定に使った時計142の値や全ポーリング時刻は保存されていません。そのため、保存された時計41の入力だけから元の判定列の全面的一致は保証できません。`tools/legacy-compare`には全件比較と、実際のOpenLR2ポーリング引数を採取して照合する計測経路があります。

NO GOOD / NO GREATとLEGACYを併用した場合は、LNの始点見逃し期限と早離し許容に、有効なPGREAT / GREAT / GOOD帯の最大幅を使います。これはBMZのコース制約との併用仕様です。HLNの見逃しは専用の保持状態処理が担当します。

スクラッチの上下2方向は、OpenLR2と同じく1つのキーとして扱います。一方の方向を離しても、もう一方を押している間は保持中のままで、LNの早離しになりません。押している方向とは別の方向の新しい押下は、通常の押下として判定します。LNを最後まで処理したとき、終点のキー音は鳴らしません。

地雷は、通常ノートと標準LNのセッションでOpenLR2と同じくレーン内のノート列に並べて処理します。

- 地雷がレーンの先頭にある間は、その後ろのノートを判定しません。地雷より前の押下でも後ろのノートの空POORにはならず、キー音も鳴りません。
- 地雷の時刻よりGREAT幅未満早い押下（`地雷時刻 - 押下時刻 < GREAT`）で被弾します。被弾した押下は消費され、同じ押下で後ろのノートは判定しません。
- 地雷の時刻を過ぎた最初のポーリングで、押下中または保持中なら被弾します。その時点で離していれば被弾せずに通過します。
- タップBADの連続判定で、次の地雷がBAD幅内にあれば、同じ押下で地雷も処理します。
- 被弾すると、ダメージ値をそのままゲージから引きます。POORの件数とBPに1を加えます。コンボ、EXスコア、処理済みノート数は変わりません。

OpenLR2の処理済みノート数（`note_current`）の上限とレーン末尾の空きスロットも再現します（`judge/engine/legacy_cap.rs`）。

- 単ノートは、処理済みノート数（空POOR以外の判定の数）がノーツ数未満の間だけ判定します。LNが同じ時刻の地雷を置き換えた場合もノーツ数は1減るため、判定できるノートがノーツ数より多い譜面があります。上限に達すると、見逃した単ノートは確定せず、そのレーンの後ろのノートも判定しません。LNの処理には上限がありません。
- OpenLR2のレーンの配列は100個ずつ増え、最後のオブジェクトの後ろに必ず空きスロットがあります。空きスロットはキー音のない-1ミリ秒の単ノートです。地雷だけのレーンでは、最後の地雷の後からポーリングごとに空きスロットを1つずつPOORにします。最後のノートへのBADの連続判定が、200ミリ秒未満の時刻に空きスロットへ進む場合も同じです。どちらも上限に達するまで続きます。
- 空きスロットを使い切ると、OpenLR2は配列の外のメモリを読むため、結果は定まりません。BMZはそこで止め、`legacy_read_past_lane_buffer`で報告します。

GROOVE / HARD / HAZARD / EASYのゲージは、OpenLR2の単曲ゲージ（`judge_damage`と`ApplyJudgeNote` / `ApplyJudgeMine`）と同じ式・同じ順序で、倍精度で計算します。HAZARDはOpenLR2のDEATHにあたります。ASSIST EASY / EX-HARD / 段位系はOpenLR2にないBMZのゲージで、LR2oraja方式のままです。

- TOTALはOpenLR2と同じく整数です。`#TOTAL`は`atol`と同じ読み方で小数点以下を捨て、1未満または未指定ならOpenLR2の既定式の値を整数に切り捨てて使います。
- PGREAT〜GOODの増加量は`TOTAL / ノーツ数`を単精度で割った値です（EASYはその1.2倍）。HARDの減少倍率はOpenLR2の式（ノーツ数による値と`100 / int((TOTAL - 80) / 16)`の大きいほう）です。
- 判定前のHPを整数に切り捨て、さらに偶数に切り下げた値が30以下なら、HARD / HAZARDのBAD・POOR・空POORのダメージは0.6倍です。
- GROOVE / EASYは2未満になりません。HARD / HAZARDは2未満になると0と表示して閉店し、次の判定でHPが0に固定されます。地雷ではこの0.6倍と固定がありません。
- 1回のポーリングでは、レーンを処理した順に判定と地雷をゲージへ反映し、全レーンの処理後に閉店を確認します。
- クリアの判定には倍精度のHPを使います。LEGACYのスコアとリプレイは`rule_mode=Legacy`のキーでローカルに保存し、IRの送信やランキング照会の対象から外します。OpenLR2の保存済み実プレイとの一括照合を実施していますが、全条件の完全一致の確認は継続中です。BMZ拡張のHCN/HLN・動的分岐等と、BPM反映を含む譜面は別の実行経路を使い、地雷も従来の通過判定になります。

## 判定幅

### リプレイ視聴速度とPractice

リプレイの入力時刻と判定窓は譜面時間を基準にし、記録時のFREQによる判定窓の
補正を再現します（FREQ記録のない旧リプレイは100%）。視聴速度による判定窓の
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

BMSの未指定・範囲外は各ruleの `#RANK 2`（NORMALは75%、PMSは70%）になります。BMSONの `judge_rank` の既定値は100%です。

`#DEFEXRANK`はbeatoraja `BMSPlayerRule.validate`と同じく、そのruleの`#RANK 2`
を基準にします。たとえば`#DEFEXRANK 100`は`NORMAL=75%`, `PMS=70%`です。
BMSON `judge_rank`はraw percentとして扱い、0以下は100%にフォールバックします。

判定窓の倍率適用もbeatoraja `JudgeWindowRule.create`に合わせます。`PMS`では
PGREAT / BAD / MISSが固定、GREAT / GOODだけがrankで変化します。`NORMAL`でも
PGREAT / GREAT / GOODがBADを超えないようにし、狭い上位判定が広い下位判定を
逆転した場合は単調化します。

`PMS`はbeatoraja `judgeVanish[Bad] = false` / `MissCondition.ONE`に従います。
BADはスコア/ゲージへ反映するがノーツを消費しないため、同じノーツを後続の
GOOD以上で再判定できます。通常LNの始点で出た非消費BADも、この時点で判定数・BP・コンボ・ゲージへ反映します。
`MissCondition.ONE` ではBADと空POORの履歴があるノーツをGOOD以上の窓で取り直せます。
消費済みノーツは候補から除外します。履歴のあるノーツを見逃すとノーツを消費し、
処理済み数を増やしてghostにPOORを記録します。判定数・BP・コンボ・ゲージへの追加は抑止します。

任意の`play.play_keysound_on_miss`（既定OFF）は採点を変えず、見逃しPOOR確定時に
通常ノート・LN始端、押したまま見逃したCN/HCN終端のキー音を再生します。
CN/HCN始端を見逃すと始端・終端の2判定を集計するが、発音は始端だけとなります。
非消費BAD・空POOR済みノートの見逃し、空POOR、早離しPOOR、地雷、不可視ノートに
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

DX MODEは9K以外で`JudgeProperty.IIDX`、9Kで`JudgeProperty.POP`を使います。
EXPAND JUDGEの任意倍率と再押下猶予倍率は設定に保持され、DXの実効窓には使いません。
上流の`BMSPlayer`に従い、設定中の倍率が100%を超える場合のAssist・スコア保存・IR送信の制限は
他ルールと同じです。コースのNO_GOOD／NO_GREATはDXにも適用します。
参照側の`POP`対象はPOPN_5K / POPN_9Kです。BMZはPMS 5K譜面も9K（`KeyMode::K9`）として
取り込むため、同じPOP判定・POPゲージになります。beatoraja 5K（BEAT_5K）はIIDXのままです。

9K以外の通常ノート / scratch:

- PGREAT: `±16666us`
- GREAT: `±33333us`
- GOOD: `±116666us`
- BAD: `±200000us`
- EMPTY POOR FAST: `500000us`（LR2oraja Endless Dream rian 1.5.0の短縮に追従）
- EMPTY POOR SLOW: `200000us`

9K以外のLN end / long scratch end:

- PGREAT / GREAT / GOOD: `±116666us`
- BAD: `±200000us`
- EMPTY POOR: なし

9Kの通常ノート:

- PGREAT: `±25000us`
- GREAT: `±50000us`
- GOOD: `±116667us`
- BAD: `±183333us`
- EMPTY POOR FAST: `529167us`（候補走査の上限は排他的）
- EMPTY POOR SLOW: `183333us`

9KのLN / CN / HCN end（早離し側だけ広い）:

- PGREAT / GREAT / GOOD: FAST（早離し）`233333us`、SLOW `116667us`
- BAD: FAST `366667us`、SLOW `183333us`
- EMPTY POOR: なし

FAST側のPGREAT / GREAT / GOOD幅は、参照側の注記どおり判定幅116667us＋離し猶予116667usです。

9KではBADがノーツを消費せず、後続入力でGOOD以上へ再判定できます。
MultiBadで付いたBADも同じく非消費とします。EMPTY POORはコンボを切り、
`MissCondition.ONE`として1ノーツにつき1回まで扱います。

9KのLN / CN / HCNをBAD / POOR相当の早さで離した場合は、116667usの
release margin中だけ終端判定を保留します。その間に押し直せば早離しを取り消し、
押し直さなければ保留した判定を確定します。long scratchのmarginは0です。

v0.5.2以前に記録した9K DX MODEのリプレイは、記録時の判定で再現するため旧POP窓で再生します
（通常ノートGOOD `±87500us`・BAD `±100000us`・EMPTY POOR FAST / SLOW `500000us` / `112500us`、
LN end PGREAT / GREAT / GOOD / BAD `±120000` / `150000` / `217000` / `283000us`、release margin `200000us`）。
リプレイファイルの`judge_revision`がないものを旧判定として扱い、新しく記録するリプレイには`judge_revision = 1`を書きます。

`#RANK`, `#DEFEXRANK`, `#EXRANKxx` / chA0は固定IIDX / POP窓へ反映しません。
BMZの実装でもDX modeの`judge_percent_at_time`は常に100%を返し、
`judge_windows_for_rule_mode`は倍率を無視します。

### コース制約と任意倍率

ランクを適用した後、PGREAT／GREAT／GOODの任意倍率にコース制約を反映します。
NO_GOODはGOODの倍率を0%、NO_GREATはGREATとGOODの倍率を0%にします。
各側でBAD幅を上限として、PGREATから順に幅が狭くならないよう補正します。
beatoraja／LR2orajaでは、上流と同じく鍵盤側の任意倍率をスクラッチとlong scratch endにも使います。
スクラッチ側の任意倍率は設定に保持しますが、実効窓には使いません。
LR2orajaの `#RANK 2` では、通常ノーツのPGREAT／GREAT／GOOD窓は
NO_GOODで18／40／40ms、NO_GREATで18／18／18msです。

## 判定アルゴリズム

判定対象ノーツの選択アルゴリズムはrule modeとは別設定で、profileの
`[judge].judge_algorithm`から選びます。

現在のenumはLR2oraja / beatorajaの並びに合わせます。

- `Combo`
- `Duration`
- `Lowest`
- `Score`

`Duration`は時間差最小、`Lowest`は先に見つかったノーツ優先、`Combo`はcombo継続寄り、
`Score`はscore寄りの比較を行います。すべてのルールで同じ設定値を使います。
DURATIONは未消費のBADと空POORを同じ走査で比較し、同じ距離では先に見つかった候補を維持します。
通常・スクラッチの窓から求めた全体のFAST側走査上限は排他的です。各判定窓の端は含みます。

リプレイに判定アルゴリズムの記録があれば、再生・動画出力・G-BATTLEでその記録を使います。
記録がなければプロフィールの設定を使います。

## MultiBad

LR2oraja / DX modeでは、押下時に選ばれたノーツ周辺のBAD範囲ノーツへ追加BADを出します。
これはLR2oraja Endless Dreamの`MultiBadCollector`相当です。

Beatoraja modeではMultiBadを出しません。
9KのDX POPでは、非消費BADの履歴があるノーツにも、上流と同じくMultiBadのBADを再度付けます。
主判定が空POORでも、先行ノーツが追加BADの条件を満たせば両方を記録します。
HLNの始点がMultiBadの対象になった場合は、押下が終端時刻以降のときだけBADにします。
終端前のHLNは未処理のまま残し、後の押下や見逃しで通常どおり判定します。
LR2oraja / DX modeでは、上流と同じくLNの始点（HLNを含む）にGOOD幅より遅いBADを付けません。
MultiBadで消費されたCN／HCNの始点に対応する未処理終端は、終端時刻を通常ノーツの
late BAD幅より過ぎた時点でPOORにします。

## HCN中の空POOR

BMZはbeatoraja／LR2oraja／DX MODEの全3ルールで、同一レーンの実効HCN本体を通過中に
発生する空POORを抑止します。保持復帰の押し直しに追加ペナルティを与えないための仕様です。
DX MODEはLR2oraja Endless Dreamの`state.passing != null && judge >= 4`の分岐と同じ扱いで、
beatoraja／LR2orajaについては上流と異なる意図的な互換例外です。通常プレイ・リプレイ・コースで共通です。
対象区間と維持する判定は[LNの仕様](ln.md#実行時の判定)を参照してください。

## Mine

地雷の時刻にレーンが押されていれば通過時に1回被弾します。スクラッチの上下を重ねて押した場合は、
片方向を離しても、もう一方を離すまで押下が続きます。
同時刻の入力は、その時刻の全Press／Releaseを反映した保持状態で通過を判定します。
譜面のダメージをゲージから直接引き、被弾音を再生します。BP、コンボ、処理済み数は増減しません。
BSSの同方向Release後に吸収した逆方向Pressは、そのPressと同時刻の地雷を起動しません。
押したまま後続の地雷を通過すれば被弾します。

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

コースで明示されたゲージpropertyを使います。指定がなければ`GaugeProperty.LR2`です。

`#TOTAL`が正値ならそれを使います。未指定または0以下ならLR2oraja Endless Dreamの既定TOTAL:

```text
160.0 + (total_notes + clamp(total_notes - 400, 0, 200)) * 0.16
```

BMSONの正のTOTALは、実効LN種別を反映した採点ノーツ数でルール別の既定TOTALを求め、
その値にかける百分率です。BMSONの未指定・0以下は既定の100%相当です。
正のBMS `#TOTAL` は絶対値です。

Beatorajaの `GAUGE_LR2` ではHARD／CLASS／EXCLASSの減少量が30%未満で0.6倍になります。
`ModifyDamage` はbeatorajaとLR2orajaでそれぞれの `GrooveGauge` の式を使います。

LR2orajaのLR2 propertyではEXHARDCLASSを含む閉店ゲージが2%未満で閉店し、2%ちょうどなら継続します。
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

Hard / ExHard / Hazard / Class / ExClass / ExHardClassのdeath borderは2%です。
残量が2%未満になった時点で0%（閉店）にします（LR2oraja Endless Dream rian 1.5.0に追従）。
9KのPOP gaugeには適用しません。

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
- judge engine / MultiBad: `crates/bmz-gameplay/src/judge/engine/mod.rs`
- judge algorithm enum: `crates/bmz-gameplay/src/judge/model.rs`
- score / combo policy: `crates/bmz-gameplay/src/score.rs`
- gauge definitions / TOTAL: `crates/bmz-gameplay/src/gauge.rs`
- play session wiring: `crates/bmz-player/src/screens/play_session.rs`
- profile UI / settings: `crates/bmz-player/src/ui.rs`, `crates/bmz-player/src/config/settings_registry.rs`
- score DB / replay persistence: `crates/bmz-player/src/storage/score_db.rs`,
  `crates/bmz-player/src/storage/replay.rs`

### LEGACYのBAD連続判定で次がLNになる場合

LEGACYはOpenLR2の`ProcSinglenote`による再帰と同様に、タップのBADで次ノートを連続判定する場合、次のLNも通常ノートとしてその入力で確定する。LN保持は開始せず、終点での二重加算もしない。連続判定されたLNがBADなら、さらに次ノートへ同じ通常ノート処理を続ける。この経路のLNには通常ノートのBAD幅を使う。通常のLN入力や他のルールのLN処理は従来どおり。
