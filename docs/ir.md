# BMZ IR (Internet Ranking) API 設計メモ

Codexに最終レビューと実装を引き継ぐための設計まとめです。
本ドキュメントでは、BMZ新BMSプレイヤー向けのIR API、クライアント側Provider trait、認証、ランキング取得、リザルト画面連携、replay拡張、NuxtHub / Drizzle実装方針を整理します。
beatoraja向け既存サービスrianIRとの互換adapterは`docs/rian-ir.md`を参照します。
アシスト、特殊モード、ランプ・履歴・replay・IRの保存条件は
`docs/score-persistence.md`を参照します。

---

## 実装状況 (2026-07-13)

以下は本メモを元に実装済みの範囲と、設計からの主な差分です。
ソースの所在: クライアントは`crates/bmz-player/src/ir/`、サーバーは
`bmz-ir-web/server/`、DB schemaは`bmz-ir-web/server/db/schema.ts`、
DB migrationは`server/db/migrations/sqlite/`です。

### 実装済み API

```http
POST   /api/v1/auth/login                      # email+password → provider_key + access/refresh token
POST   /api/v1/auth/register
POST   /api/v1/auth/logout
POST   /api/v1/auth/refresh                    # refresh token rotation + provider_key
GET    /api/v1/me
POST   /api/v1/scores                          # include=rankings&ranking_scopes=... 対応
POST   /api/v1/scores/delete-backfills         # 本人の古い local_backfill score を batch 削除
GET    /api/v1/scores/{id}                     # スコア詳細 (公開)
GET    /api/v1/charts                          # 譜面一覧 (?q= タイトル検索)
GET    /api/v1/charts/{sha256}                 # 詳細 + play_count/clear_count 集計
GET    /api/v1/charts/{sha256}/ranking         # scope/ln_policy/rule_mode/limit/offset
GET    /api/v1/rivals
POST   /api/v1/rivals                          # { target_player_id, action: add|remove }
DELETE /api/v1/rivals/{player_id}
GET    /api/v1/rivals/{player_id}/comparison   # BMZ score identity 単位の EX/BP 比較
GET    /api/v1/players/{id}                    # プロフィール + best scores
GET    /api/v1/daily                           # player/date/mode=all の日次成果
POST   /api/v1/admin/difficulty-tables/sync    # 管理者による難易度表の即時同期
GET    /api/v1/device-keys                     # 自分の署名鍵一覧
POST   /api/v1/device-keys                     # 公開鍵登録 (同一鍵は再利用)
DELETE /api/v1/device-keys/{id}                # 失効 (revoked_at)
POST   /api/v1/scores/{id}/replay/upload-url   # replay upload endpoint URL
PUT    /api/v1/scores/{id}/replay/upload       # replay body upload (要 Bearer 認証 + 所有者一致)
POST   /api/v1/scores/{id}/replay/verify       # storage 実体の hash 検証
GET    /api/v1/scores/{id}/replay              # download URL metadata (公開)
GET    /api/v1/scores/{id}/replay/raw          # replay body download
```

```http
POST   /api/v1/course-scores                   # コーススコア (kind=dan|course)
GET    /api/v1/courses/{course_hash}           # registry + play_count
GET    /api/v1/courses/{course_hash}/ranking   # global のみ
```

未実装: `PUT /api/v1/charts/{sha256}` (chart upsertはscore submit内で実施)、
難易度表の公開管理APIです。

日次成果は`/daily?player=<id>&date=YYYY-MM-DD&mode=all`で公開し、`player`省略時は
ログイン中の本人を対象にします。成果日の切り替わりはprofileの
`daily_boundary_minutes` (JST 00:00からの分、既定0)で設定できます。難易度表は
運用側allowlistを6時間ごとにD1へ世代同期し、日次API返却時にMD5優先、
SHA-256 fallbackでラベルを付与します。

難易度表を即時同期する管理APIは、ログイン済みユーザーのうち
`NUXT_IR_ADMIN_USER_IDS`（カンマ区切り）に含まれるIDだけが実行できます。未設定時は
全ユーザーを拒否します。Cronと管理APIが重なった場合はD1の期限付きロックで片方を
`already_running`としてスキップします。同期失敗はCloudflare LogsへURLと原因を最大10件、
各エラー500文字まで記録し、全件失敗時は最初の原因を管理APIの502エラーにも含めます。

### クライアント (bmz-player)

- CLI: `bmz ir login|logout|status|ranking|sync|upload-local|download-scores|attest-submitted|cleanup-imported|cleanup-duplicate|rivals|device-key|replay`です。
  eguiプロファイル設定からもログイン可能です。
- 送信: リザルト確定時に譜面時間を`result.duration_ms`へ含め、
  `ir_score_jobs`へenqueue (send_policy判定込み) →
  リザルト画面で即時送信 + アプリ常駐ワーカー (30秒間隔)が残りを処理します。
  結果画面・常駐ワーカーは1バッチ20件、job間3.1秒待ちで送信します。CLIの
  `ir sync` / local backfillは手動検証を速くするため100件 / 200msを使います。
  バッチ取得時に対象jobをまとめて`sending`としてclaimし、リザルト送信と
  常駐ワーカーが並行しても同じjobを二重送信しません。
  CLIの`ir sync` / `ir upload-local --sync`は中断時に未処理jobを一括で
  `sending`に残さないよう1件ずつclaimし、`[N/100]`の進捗を逐次表示します。
  Ctrl+C時に残る`sending`は処理中の最大1件で、5分後に再取得されます。
  リトライは1分 → 5分 → 30分 → 2時間 → 以降24時間です。
  429が`Retry-After`を返した場合は通常のバックオフより優先してその時刻まで待ちます。
  score成功履歴の保存と`kind=replay` jobの投入は同じtransactionで確定します。
  replay upload / verifyの失敗はreplay jobだけを再試行し、成功済みscoreを
  未送信状態へ戻しません。
  成功済みjobはpayloadを空にし、同期後に30日以内または最新500件だけを
  `network.db`に保持します。未送信 / 失敗 / 送信中jobは剪定対象外です。成功判定と
  remote score IDの紐付けはjob/logの保持期間から独立した成功台帳にも保存し、
  pruning後もupload-localの既送信判定とdownload-scoresの既存履歴リンクに使います。
  成功台帳は成功時に元のrequest payload、chart hash、LN policyも同一transactionで保存します。
  成功履歴ごとに元request body分の容量を台帳で保持する一方、job本体の保持期間は従来どおりにします。
  snapshotをjob pruning後の安全な`--resend`に使います。元payloadが消去済みの旧成功履歴は本文を
  再構築せず再送不可として数え、CLIは非ゼロで終了します。IR cleanupではsnapshotも台帳と
  同じtransactionで削除します。
  migrationは残っている成功submissionから台帳を作るが、過去にpruning済みの
  submissionにしかなかったremote ID / 紐付けは復元できません。
- local backfill: `bmz ir upload-local [--dry-run] [--limit N] [--sync] [--all]`で
  `score.db`の既存`score_history`を`local_backfill` sourceのscore submitとしてenqueue
  します。既送信履歴と既にqueueにある履歴は既定でスキップします。`download-scores`で
  同じprovider/accountから取得済みのscoreも、IR上に既存scoreがあるため候補から除外し、
  `--resend`でも再投入しません。別providerまたは別accountへの送信は引き続き候補になります。
  downloaded source markerはBMZ IR / BMS-IR共通の`bmz_ir`で、providerとaccountも照合します。
  値を解釈できない履歴や、譜面情報・replayファイルを読めない履歴はその行だけを飛ばし、
  `invalid`として件数と理由を出力します。他の履歴の投入は続け、1件以上あればCLIは最後に
  非ゼロで終了します。`--all`では同じ実行内で失敗した行を再走査しません。
  course stage / autoplayは既定で除外します。local backfillは未登録chartを作成できるが、既存chart
  の正規メタデータは上書きしません。`--sync`で既存の未完了score jobがある場合は、
  queueを増やさず先に最大100件を同期します。投入batchの既定値は200件です。
  `--sync`の同期で送信に失敗したjobが残った場合や、他プロセスの`sending` jobだけが残って
  進捗できない場合は、`--all`と同じく非ゼロで終了します（失敗jobはqueueに残る）。
  `--all`は`--sync`を含む完走モードで、既存queueの排出と次の投入batchを候補が
  なくなるまで繰り返します。履歴は`(played_at, id)`順に512行ずつ読み、次の投入batchは
  前のbatchが止まった行の次から読みます（同じ実行内で全履歴を読み直さない）。
  dry-run（GUIの件数見積もりを含む）は譜面の解析情報を読まず、replayはhashせず存在だけを確かめます。
  1回の実行で各jobは最大1回だけ送ります。実行開始前から残る`failed` jobは
  retry/backoffを無視して1回再送し、実行中に失敗したjobは取り直しません。送信に失敗したjobは
  queueに残したまま次のjobへ進み、全件を処理した後に失敗件数を報告して非ゼロで終了します。
  失敗jobは常駐workerのbackoffに従う再試行、送信キューからの再送、次回の`--all`で再び送られます。
  次の場合は途中で打ち切って非ゼロで終了します。
  - 401/403・未ログイン・provider設定の不備: 以降のjobも失敗するため即座に止めます。
  - 429: サーバーの送信制限に従い即座に止めます。
  - それ以外の失敗（4xxの内容エラー、5xx、通信エラー）が10件続いた場合
    （`IR_MANUAL_CONSECUTIVE_FAILURE_LIMIT`）です。サーバー障害中に全jobのbackoffを
    伸ばさないためで、成功が1件あれば数え直します。
  他プロセスの`sending` jobだけが残って進捗できない場合も非ゼロで終了し、5分のlease後に再実行します。
  `--resend`は失敗済みjob・成功snapshotを同じidempotency keyとrequest payloadで再queueします。
  再queueしたjobは試行回数を0に戻し、次に失敗したときのbackoffを初回（1分）から数え直します。
  既存pending jobと有効なsending leaseは変更せず、`enqueued`は実際に再queueした件数を示します。
  `--all --resend`は1回の実行内で各履歴を最大1回だけ処理します。`--dry-run --resend`は
  状態を書き換えず、元payloadが不足して再送できない履歴も別カウンターで報告します。
  送信直前にdevice keyで署名し、サーバー側では`signed_backfill`として保存します。
  `source_kind`、1P/2Pの配置、実際に適用したDPオプションも送信し、ランキング用の
  `double_option`バケットとは分けて個別スコアの履歴に保持します。
  upload-localの同期対象と進捗件数は指定provider/accountのscore jobだけに限ります。
  `--include-replay`はscore payloadにreplay upload intentを付け、score成功後にreplay jobを作ります。
  replay jobの実送信はupload-localでは行わず、通常のworkerまたは`ir sync`が処理します。
  local backfillの`pass_notes`は保存済み判定数から生成し、`empty_poor`は実ノーツ判定数に
  含めません。Failedスコアでは未処理ノーツ分を`min_bp` / `min_cb`に含めた状態で送るため、
  download-scoresは同じ`pass_notes`からローカル履歴を復元できます。旧backfillで
  `pass_notes == notes`のまま送信したFailedスコアは、BP/CBの増分が同じ正数で、そこから
  推定した処理済み数が判定数と一致する場合だけ復元します。不一致の履歴は取り込まずエラーにします。
  既に送信済みで署名のないscoreは`bmz ir attest-submitted --all`で、ローカルの
  成功履歴に残るremote score IDへ後付けattestationを送ります。attestationはscore本文を
  再送せず、所有者のdevice keyでscore IDを署名してverificationだけを更新します。
  現行DBでは成功台帳がjob/logのpruning後もremote score IDを保持します。成功台帳の追加前に
  jobをpruningしてremote score IDが失われた旧DBの履歴は復元できず、そのscoreは対象外です。
  その場合は将来のserver側一括attestation APIが必要になります。
  同じユーザー・`idempotency_key`・同じプレイ内容の再送は、保存済みscoreを副作用なしで成功として返します。
  同じkeyで譜面・ルール・成績などが異なる場合は409 Conflictを返します。
  これは廃止済みのevidence形式でqueueに残ったjobを回復するためで、初回送信の署名・
  データ検証も行います。
  per-history ghostは現在の`score_history`には保持していないため送りません。
- download scores: 自分の送信したscoreは新規履歴として取り込まず、既存のローカル履歴へ
  `bmz_ir` sourceを紐付けます。照合は成功台帳のremote score IDを優先し、引けない場合は
  `GET /api/v1/me/scores`が返す投稿時の`idempotency_key`を、成功台帳・未完了jobの
  `submission_key`、local backfillの`score_history_ir_identity`の順に照合します。応答を失った送信、
  score IDの無い応答、送信中のjob、台帳追加前にpruningされた履歴も重複取り込みしません。
  keyで引いた履歴は譜面hashが一致する場合だけ紐付けます。keyを返さないproviderでは従来どおり
  remote score IDだけで照合します。
- download scores: `bmz ir download-scores`は各行の取り込み失敗数を集計してから出力し、
  1件以上あれば通常実行・dry-runとも非ゼロで終了します。通常実行ではほかの有効な行を保存し、
  再実行時に重複登録しません。dry-runでは保存しません。途中のページの取得に失敗した場合も、
  それまでのページで保存した件数を出力してから非ゼロで終了します。
  無効化したproviderからは取得しません。認証情報のaccountが設定の`account_id`と異なる場合も
  取得しません（取り込み元の記録とupload-localの送り返し除外を同じaccountで照合するため）。
- ゲーム内の手動IR操作: 設定 → IRの「スコアの一括操作」と、その下の「IR送信キュー」
  ウィンドウです。処理はCLIと共通の`ir::manual`（`upload_local_batch` / `upload_local_all` /
  `sync_jobs` / `download_scores` / `resend_job` / `resend_failed_jobs`）で、CLIは進捗通知を
  従来の出力へ変換するだけにしています。
  - 対象はprimary providerのみです。ローカルスコアの一括送信はBMZ IRのみ（BMS-IR / rianIRは
    非対応）、スコア取得はrianIRを除きます。未ログインやアカウント未確定の間はボタンを無効にします。
  - 「ローカルスコアを一括送信」は`ir upload-local --all`相当の完走モードで、
    `include_replay`と`include_course_stages`はCLI既定値（どちらもfalse）です。
    確認モーダルを開くと、dry-run相当で候補件数を数え（送信待ちの既存jobも加える）、
    `件数 × IR_CLI_SYNC_JOB_SPACING_MS`を最短時間の目安として表示します。
    失敗jobの扱いと打ち切り条件は`--all`と同じです。lease待ちから再開しても同じ実行開始時刻を使い、
    その実行で失敗したjobは再送しません。失敗が残った完了・連続失敗・認証エラー・送信制限は
    それぞれ別のトーストで通知します。
  - 「IRからスコアを取得」は`ir download-scores`相当で、CLIの既定上限（200件）を付けず全件を対象にします。
    既に取り込んだscoreは除外されるため、中断後の再実行は続きから進みます。GUIでは新規scoreが
    保存された場合、完了・中断・エラーの通知時にSelect一覧、フォルダsummary、player statsを更新します。
    既存履歴へのリンクだけで新規保存がない場合は更新しません。取得結果には取り込み・紐付け・失敗件数を
    表示し、部分失敗は通常完了と区別します。
  - 送信は`IR_CLI_SYNC_BATCH_LIMIT`（100）件ずつ、job間`IR_CLI_SYNC_JOB_SPACING_MS`（200ms）です。
    中断はjob間・batch間・送信間隔のsleep中に確認し、処理中の1件は完了させてから止まります。
  - 書き込み系の操作は同時に1つだけ実行します。要求ごとの世代番号で、古い結果・進捗・一覧読み込みは破棄します。
    専用スレッドのcurrent-thread runtimeで動き、設定ウィンドウを閉じても継続します。
    完了・失敗・中断はトーストで通知します。
  - 実行中は常駐ワーカーを一時停止し、終了・失敗・中断のいずれでも再開します。
  - 他のプロセスや停止前にclaim済みのjobが`sending`で進捗できない場合は、15秒間隔で
    再試行します。進捗なしのまま6分（lease 5分 + 余裕）続くと停止して報告します。待機中も中断できます。
    失敗jobがあった場合は待たずに停止します。
  - 実行中はprofile切替を開始できません（UIも無効）。アプリ終了時は中断を要求して処理中の
    1件を最大3秒待ちます。待ち切れなかったjobは`sending`のまま残り、5分のlease後に再取得されます。
  - 送信キュー画面は`list_ir_score_jobs` / `ir_score_job_status_counts`で読み、status別件数、
    statusフィルタ（既定: 失敗）、1ページ50件のページングを持ちます。並びはfailed → sending →
    pending → succeeded、次に更新日時の新しい順です。DB読み込みは別スレッドで行い、UIスレッドでは開きません。
    一覧は操作の完了後に自動で再読み込みします。
  - 再送（個別・失敗分一括）は状態を直接書き換えず、通常の送信と同じclaim経由で、
    再試行待ち時刻（backoff）を無視して送ります。`Immediate`トランザクション内で`sending`へ更新するため、
    常駐ワーカーや他プロセスが送信中のjobは二重に送らず、「送信中のため送らなかった」件数として数えます。
    失敗分の一括再送は開始時点のfailed一覧を対象に固定します。
  - 決定画面・プレイ画面では、IR設定と同様に操作できません。
- import cleanup: `bmz ir cleanup-imported`は、Beatoraja行と譜面hash、プレイ日時、
  EX、全判定内訳、BP、CB、max combo、random seedが一致する旧`source_kind=Local`
  履歴をdry-runで表示します。`--apply`は選択したprovider/accountの明示的な
  `local_backfill` scoreだけをserver側で削除してaffected `best_scores`を残存history
  から再構築します。ローカル重複に対応する通常プレイまたは旧形式のIR scoreは保持し、
  次にローカルのIR job/submission台帳を削除してから`score.db`の履歴、`score_best`、
  `player_stats`を再集計します。別IR accountへの送信済み候補がある場合は、ローカル削除を止めます。
- exact duplicate cleanup: `bmz ir cleanup-duplicate <HISTORY_ID> --apply`は、同じsource kind内で
  譜面hash、プレイ日時、EX、全判定内訳、BP、CB、max combo、random seedが一致する履歴が
  ちょうど1件だけあることを再確認してから、指定historyを削除します。対応する
  `local_backfill` IR score、network job/submission、ローカル集計も同じ順序で整理します。
- rate limit: score submit / course scoreは15分あたりuser 6000 / IP 12000です。
  replay upload系は1 replayあたりupload-url / upload / verifyの3 requestを使うため、
  15分あたりuser 900 / IP 1800です。429では`Retry-After`を返します。
  backfill cleanupは19 score / request、15分あたりuser 6000 / IP 12000で、clientは
  batch間を200ms空けます。
- ランキング表示: Result / Selectスキンの`NUMBER_IR_RANK(179)` /
  `NUMBER_IR_TOTALPLAYER(180/200)` / `NUMBER_IR_CLEARRATE(181)` /
  `OPTION_IR_LOADING/LOADED/NOPLAYER/FAILED(601..604)`
  （LR2 CSVのSelect / Decideでは600..608を単一の状態として排他的に評価します。
  [スキン仕様](skin.md)の600..608を参照）、
  Selectの`STRING_RIVAL(1)` / `NUMBER_RIVAL_SCORE(271)` /
  `NUMBER_RIVAL_MAXCOMBO(275)` / `NUMBER_RIVAL_MISSCOUNT(276)` /
  `OPTION_(NOT_)COMPARE_RIVAL(624/625)`です。eguiのリザルトオーバーレイもありです。
- ターゲット: `TARGET: RIVAL` (TargetOption::Rival)が選曲時のIRライバル
  ベストEXをプレイ中ゴースト / リザルト差分に使います。
- ライバル: `ir rivals`実行時に`profile.rival.entries` (source=Ir)へ同期します。
- Web: ユーザーページからライバルを登録・解除し、`/rivals`で逆ライバル一覧と
  `chart_sha256 + ln_policy + double_option + rule_mode + scoring`単位のEX/BP比較を表示します。
  譜面ランキングは全体と「自分＋ライバル」を切り替えられます。
- リプレイ: 送信payloadにhash申告 → 送信成功後に自動アップロード + 検証です。
  `bmz ir replay <SCORE_ID>`でダウンロードし
  `bmz --boot-replay-file <PATH>`で再生します。
- provider key: BMZクライアントは`/api/v1/auth/login` /
  `/api/v1/auth/refresh`が返す`provider_key`をcredentials / device key /
  `ir_score_jobs.provider` / `primary_provider`の識別子として使います。
  `IrProviderConfig.provider`は表示名または実装種別として残します。
  `primary_provider`が明示されている場合、ランキング取得・表示はそのproviderだけを使い、
  無効・未設定でも別providerへフォールバックしません。空の旧設定だけは先頭の有効providerを使います。
  submit-only providerはスコアを送信するが、送信応答へのランキング同梱を要求しません。
  ResultのランキングAPI取得はprimaryの今回分の送信完了後に開始し、secondary providerや
  backlogの送信完了を待ちません。primary jobを別の同期taskがclaim済みの場合も、そのjobだけを待ちます。
  eguiのResultオーバーレイは、送信対象なし・成功・失敗をprovider別に表示します。
- BMS-IR: `https://www.bms-ir.org`を3番目の固定providerとして保持します。
  既定は無効で、BMS-IRの数値IDとLR2 game tokenをcredential storeに保存します。
  ログイン後はprimary providerに選択でき、既存のdurable queueから単曲・終端
  course/Dan scoreを送ります。ランキング、登録ライバルとbest同期、BMS-IR難易度表、
  server→localの自己スコア同期をprovider別cacheで扱います。ライバル追加・解除はWebで行います。
  BMS/PMSはMD5、BMSONはBMS-IR互換SHA hashで照合し、Beatoraja/LR2oraja rule、
  実効LN/CN/HCN、Off/Flip/Battle/Battle ASの4/5/6/7/8/9/10/14/24/48Kを送信対象にします。
  BattleとBattle ASは別bucket、G-BATTLEは通常bucketを使います。DX、full autoplay、
  replay再生、Practice、実効assist、replay file送受信、local backfillは対象外です。
  コース中の単曲スコアは`course_stage=true`を付け、保存時と同じ
  NoPlay/FullCombo/Perfect/Maxへの丸めを行ったlampだけを送ります。数値score・judgeは
  単曲結果を維持します。終端course/Dan scoreは別jobとして送ります。
  4/6/8Kもクライアント固有枠ではなくBMS-IRの正規キーモードとして送ります。
  ローカル統合確認用のビルドだけは、`BMZ_BMS_IR_BASE_URL`を`cargo build`時に
  指定すると固定endpointを差し替えられます。通常ビルドではproduction URL固定で、
  profile設定からgame tokenの送信先を変更することはできません。
- random seed: 新規プレイはbeatorajaと同じside別24 bit seedを使い、SPはP1、
  DPは`P1 + P2 * 2^24`を`play_options.seed` / `random_seed`に10進文字列で送ります。
  `seed_scheme=beatoraja_24bit_v1`とBMS `#RANDOM`の選択列を別metadataとして保持します。
  F-RANDOM / MF-RANDOMを含むoptionの送信可否・外部IDへの対応は共通層で拒否せず、
  将来の`IrProvider` capabilityとユーザー設定で決めます。

### 設計からの主な差分

1. **認証**: OAuth/OIDC device flowではなくローカルemail+password
   (`/api/v1/auth/login`, `nuxt-auth-utils` session + bearer token)を採用します。
   device flowは将来課題です。
2. **秘密情報の保存先**: `profile.toml`の`[ir] credential_store = "File" | "Os"`です。
   既定はFile (プロファイル配下0600 JSON)です。開発時のKeychain許可ダイアログを
   避けるためで、`"Os"`にするとkeyring経由でOS credential storeに保存し、
   既存ファイルは初回アクセスで自動移行します。
   LinuxビルドはSecret Service用にlibdbusが必要です。
3. **provider設定**: `IrProviderConfig`に`base_url`と`provider_key`を追加します。
   `providers[0]`はBMZ IR、`providers[1]`はrianIR、`providers[2]`はBMS-IRの
   固定枠とし、無効時もprofileに保持します。4枠目以降はBMZ / rianIR互換プロトコルを選ぶ
   カスタムURL用とします。
   `provider_key`はクライアント側でURLから推測せず、BMZ IRサーバーの
   auth responseから取得します。ローカルIRは`bmz-dev`、production IRは
   `bmz`を返します。サーバー側は`NUXT_IR_PROVIDER_KEY`で明示上書き可能です。
4. **verification**: DBは`unverified / signed_backfill / verified_play`の3値です。
   `replay_uploaded / verified`はスコアのverificationではなく
   `replay_objects.status` (`metadata_only / pending_upload / uploaded /
   verified / rejected`)で管理します。不正な署名はscore submitを400で拒否します。
5. **tamper evidence**: canonical formは「top-level `evidence`を除いた
   payloadをRFC 8785 JSON Canonicalization Scheme (JCS)でcompact JSON化」
   したものです。署名はEd25519(secret, SHA256(canonical))です。
   JSON numberはECMAScript `JSON.stringify()`相当の表現に正規化するため、
   `160.0`と`160`は同じcanonical bytesになります。
6. **ranking response**: `pagination.total` (scope内総数)と
   `ranking.clear_rate` (%)、score submitのscopeごとの`previous_rank`を返します。
   `previous_rank`は送信スコアを除いた自己ベストをレスポンス生成時点の
   全体ランキングへ仮に置いた順位で、初回プレイは`0`です。Result skinの
   `NUMBER_IR_PREVRANK(182)`に反映します。
7. **played_at**: クライアントはunix秒で送ります。サーバーがISOへ正規化して
   timestamptzに保存します。
8. **gauge表記**: 保存値・返却値はBMZの`GaugeType::as_str()`と同じ
   `AssistEasy` / `Easy` / `Normal` / `Hard` / `ExHard` / `Hazard` /
   `Class` / `ExClass` / `ExHardClass`です。APIはlowercaseやlegacy aliasも
   受けるが、DBとresponseではcanonical値へ正規化します。
9. **idempotency / retry**: score / course scoreの重複投稿は既存行を返します。
   course scoreの重複時はbestを再計算せず`best_updated=false`です。
   ローカルjob queueは`sending`のまま5分以上止まったjobも再送対象にします。
   refresh tokenはrotation方式で、BMZクライアント側はプロセス内mutexで
   refreshを直列化します。サーバー側は直近の`rotated` token再利用を並行retry
   とみなし、新tokenを巻き込んだ全session revokeはしません。
10. **course score**: 実装済み (本節のAPI一覧と §19のDDL参照)です。
   コース終了時に`ir_score_jobs` (kind=course)へenqueueし、
   evidence schemaは`bmz-course-score-evidence-v1`です。
   `around_self`は自分の前後5件ずつのウィンドウを返す
   (未ログイン / 自己スコアなしのときはglobalと同じ)です。
   **tablesの専用実装**は未着手です。

### 動作確認の手順 (ローカル)

```bash
bun install
bun run db:migrate
bunx prettier . --check
bun run build
bun run cf:types
bun run dev                            # http://localhost:3000
bmz ir login --email <EMAIL> --base-url http://localhost:3000
bmz ir status                          # key: bmz-dev / connection: OK を確認
# 1 曲プレイするとリザルトで送信 + リプレイ自動アップロード
bmz ir ranking <SHA256> --ln-policy ForceLn
```

---

## 0. 前提・設計ゴール

### 前提

- BMZは新規BMSプレイヤーです。
- IRはbeatoraja IR API互換そのものではなく、BMZ内部モデルを中心に設計します。
- beatorajaの`IRConnection`的な公開境界は参考にします。
- 将来Mocha / MinIR互換Providerを追加する可能性があります。
- BMZ公式IRサーバーも作成中だが、現時点では動作確認・reference implementation寄りです。
- 公式IRサーバーはNuxt + NuxtHub DB / Drizzle ORMで実装中です。
- Cloudflare deployはNuxtHubが生成するwrangler configを使い、D1 bindingは
  `DB`、R2 replay blob bindingは`BLOB`とします。
- リプレイblobは`hub:blob`経由で保存し、ローカルは`.data/blob`、Cloudflare
  buildは`NUXT_HUB_BLOB_BUCKET`のR2 bucketを使います。
- 長期的にBMZ公式IRをサポートし続けるかは未確定です。
- BMZ本体は公式IR APIに密結合しません。
- 譜面hashはSHA256を主キーにします。
- MD5はbeatoraja系互換・外部IR連携用に保持します。
- ランキングの主軸はEX score固定です。
- LN / CN / HCNは`docs/ln.md`の`LnScorePolicy`に従って区別します。
- IRのscore identityはchart SHA256単独ではなく、`chart_sha256` / `ln_policy` / `double_option` / `rule_mode` / `scoring`を含みます。
  `gauge`は投稿時のプレイ設定・表示metadataとして保存するが、ランキング分離キーにはしません。
- replayは将来アップロードする可能性があるため、初期設計からhash / format / statusを持たせます。

### 設計ゴール

- BMZ内部モデルを中心にして、Provider Adapterで外部IR差分を吸収します。
- スコア送信とランキング取得がセットになりやすいBMSプレイヤーのUXに合わせます。
- リザルト画面で全体ランキング / ライバルランキングを切り替え可能にします。
- スコア送信時にランキングを同時取得するかどうかを設定可能にします。
- ランキング取得失敗でスコア送信成功まで失敗扱いにしません。
- replay upload / verificationに将来拡張できます。
- tamper evidenceとしてcanonical hash / client signatureを入れます。
- 完全なチート防止ではなく、「提出内容があとから改変されていない」ことを示す仕組みを作ります。

---

## 1. 全体アーキテクチャ

```txt
BMZ Core
  ├─ Local Score DB
  ├─ IR Job Queue
  ├─ IR Provider Trait
  │    ├─ BMZ Official IR Provider
  │    ├─ Mocha-compatible Provider Adapter
  │    └─ MinIR-compatible Provider Adapter
  └─ Credential Store
       ├─ macOS Keychain
       ├─ Windows Credential Manager
       └─ Linux Secret Service
```

### 基本方針

- BMZ内部では独自の`IrProvider` traitを定義します。
- 公式IRも外部IRもtrait実装として扱います。
- スコア送信は有効な全IRに送ります。
- ランキング取得、ライバル取得、表取得、URL openはPrimary IRのみで行います。
- Secondary IRは原則スコア送信用です。

```txt
全IR:
  - スコア送信

Primary IR:
  - スコア送信
  - ランキング取得
  - ライバル取得
  - テーブル取得
  - URL解決
  - open_ir / F11 系の画面遷移
```

---

## 2. 認証・Token 保存

### 方針

- ブラウザ認証 + token保存です。
- デスクトップアプリではOAuth/OIDCのdevice authorization flowまたはloopback redirectを使います。
- BMZのconfigには秘密情報を置きません。
- refresh tokenはOS credential storeに保存します。

### OS credential store

候補:

- macOS: Keychain
- Windows: Credential Manager
- Linux: Secret Service / libsecret
- Rust: `keyring` crate系を候補にする

### Config に置くもの

IR関連設定はユーザー / profile単位の状態なので、BMZ本体のglobal
`config.toml`ではなく`data/profiles/{profile}/profile.toml`に保存します。

```toml
[ir]
primary_provider = "bmz"

[[ir.providers]]
provider = "bmz"
provider_key = "bmz"
base_url = "https://bmz-player.hyrorre.workers.dev"
enabled = true
account_display_name = "Hyrorre"
send_policy = "always"
role = "primary"

[[ir.providers]]
provider = "rian-ir"
provider_key = ""
base_url = "https://rianir.link/"
enabled = false
account_display_name = ""
send_policy = "always"
role = "submit_only"

[[ir.providers]]
provider = "bms-ir"
provider_key = "bms-ir"
base_url = "https://www.bms-ir.org"
enabled = false
account_display_name = ""
send_policy = "always"
role = "submit_only"

[[ir.providers]]
provider = "bmz"
provider_key = "bmz-dev"
base_url = "http://localhost:3000"
enabled = true
account_display_name = "ExampleUser"
send_policy = "complete_song"
role = "submit_only"
```

先頭3件はeguiで削除・種別変更できません。追加providerは4件目以降へ保存し、
互換プロトコルとカスタム`base_url`をログイン前に設定します。

保存してよいもの:

```txt
provider
provider_key
base_url
account_display_name
account_id
enabled
primary_provider / role
send_policy
last_login_at
last_success_at
```

保存しないもの:

```txt
refresh_token
access_token
client secret
署名秘密鍵
```

### Credential Store のキー例

```txt
service: bmz.ir.bmz
user:    account_id
secret:  refresh_token

service: bmz.ir.bmz-dev
user:    account_id
secret:  refresh_token

service: bmz.ir.device-key.bmz
user:    account_id
secret:  private_key

service: bmz.ir.device-key.bmz-dev
user:    account_id
secret:  private_key
```

---

## 3. BMZ Provider Trait 案

```rust
#[async_trait::async_trait]
pub trait IrProvider: Send + Sync {
    fn provider_id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;

    async fn auth_status(&self) -> IrResult<IrAuthStatus>;

    async fn begin_login(&self) -> IrResult<IrLoginFlow>;
    async fn complete_login(&self, flow: IrLoginFlowResult) -> IrResult<IrAccount>;

    async fn logout(&self) -> IrResult<()>;
    async fn refresh_token_if_needed(&self) -> IrResult<()>;

    async fn get_rivals(&self) -> IrResult<Vec<IrPlayer>>;
    async fn get_tables(&self) -> IrResult<Vec<IrTable>>;

    async fn get_chart_ranking(
        &self,
        chart: &IrChartIdentity,
        query: IrRankingQuery,
    ) -> IrResult<IrRanking>;

    async fn get_player_chart_score(
        &self,
        player: &IrPlayerRef,
        chart: &IrChartIdentity,
    ) -> IrResult<Option<IrScore>>;

    async fn submit_score(
        &self,
        request: IrScoreSubmission,
        options: IrSubmitOptions,
    ) -> IrResult<IrSubmitResponse>;

    async fn submit_course_score(
        &self,
        request: IrCourseScoreSubmission,
    ) -> IrResult<IrSubmitResponse>;

    fn get_chart_url(&self, chart: &IrChartIdentity) -> Option<Url>;
    fn get_course_url(&self, course: &IrCourseIdentity) -> Option<Url>;
    fn get_player_url(&self, player: &IrPlayerRef) -> Option<Url>;
}
```

### AuthManager 分離案

Login flowはOS / UI / ブラウザ起動が絡むため、Providerに閉じ込めすぎません。

```txt
IrProvider
  - OAuth/OIDC設定を返す
  - token endpointを知っている
  - refreshできる
  - API呼び出しできる

IrAuthManager
  - ブラウザを開く
  - loopback serverを一時起動する
  - device codeを表示する
  - credential storeに保存する
```

---

## 4. IR 送信ポリシー

beatorajaの三択を踏襲します。

```rust
pub enum IrSendPolicy {
    Always,
    CompleteSong,
    UpdateScore,
}
```

意味:

```txt
Always:
  共通の保存・送信 eligibility を満たすリザルトを常に送る

CompleteSong:
  最終ゲージが 0 より大きい場合だけ送る

UpdateScore:
  初プレイ、または EX score、clear、max combo、BP、CB のいずれかが改善した場合だけ送る
```

### 注意

- 送信ポリシーはクライアント側UXの制御です。
- 実効アシスト、autoplay、replay、Practice、key mode変換は送信ポリシーより前に除外します。
- サーバー側はサーバー側でbest更新判定を行います。
- クライアントが送ってきたからといって必ずbestとして採用しません。

---

## 5. リザルト画面用ランキング取得設定

BMZ メニューのIR設定にチェックボックスを2つ用意します。

```txt
[ ] スコア送信時に全体ランキングを取得する
[ ] スコア送信時にライバルランキングを取得する
```

### 内部設定名

ユーザー向け文言は上記でよいが、内部名は`prefetch`として扱います。

```toml
[ir.result]
prefetch_global_ranking_on_score_submit = true
prefetch_rival_ranking_on_score_submit = true
```

```rust
pub struct IrResultOptions {
    pub prefetch_global_ranking_on_submit: bool,
    pub prefetch_rival_ranking_on_submit: bool,
}
```

### 推奨挙動

```txt
スコア送信時:
  設定ONのランキングだけ一緒に取得

リザルト画面切り替え時:
  既に取得済みなら即表示
  未取得ならその場で Ranking API を叩く
```

設定パターン:

| 全体取得 | ライバル取得 | スコア送信時 | 切り替え時 |
|---|---|---|---|
| OFF | OFF | スコア送信のみ | 初めて開いたランキングを都度取得 |
| ON | OFF | globalだけ取得 | self_and_rivalsは切替時に取得 |
| OFF | ON | self_and_rivalsだけ取得 | globalは切替時に取得 |
| ON | ON | globalとself_and_rivalsを取得 | 両方即切替可能 |

### リザルト画面状態管理案

```rust
pub enum ResultRankingTab {
    Global,
    SelfAndRivals,
}

pub enum RankingLoadState {
    NotRequested,
    Loading,
    Loaded(IrRanking),
    Failed(String),
}

pub struct ResultRankingState {
    pub global: RankingLoadState,
    pub self_and_rivals: RankingLoadState,
    pub active_tab: ResultRankingTab,
}
```

タブ切り替え時:

```rust
match selected_tab_state {
    RankingLoadState::Loaded(_) => {
        // 即表示
    }
    RankingLoadState::Loading => {
        // ローディング表示
    }
    RankingLoadState::NotRequested | RankingLoadState::Failed(_) => {
        // Ranking APIで取得
    }
}
```

---

## 6. Score Submission API

### Endpoint

```http
POST /api/v1/scores
```

### ランキング同時取得なし

```http
POST /api/v1/scores
```

### 全体ランキングのみ同時取得

```http
POST /api/v1/scores?include=rankings&ranking_scopes=global&ranking_limit=100
```

### ライバルランキングのみ同時取得

```http
POST /api/v1/scores?include=rankings&ranking_scopes=self_and_rivals&ranking_limit=100
```

### 全体 + ライバルランキングを同時取得

```http
POST /api/v1/scores?include=rankings&ranking_scopes=global,self_and_rivals&ranking_limit=100
```

### Query parameters

| parameter | type | description |
|---|---|---|
| `include` | string | `rankings`を指定するとランキングを同時取得する |
| `ranking_scopes` | comma-separated string | `global`, `self_and_rivals`など |
| `ranking_limit` | integer | 各ランキングの最大件数です。初期値は100など |

### Request payload

`play_count` / `clear_count`はScore Submission APIでは送りません。
これらはクライアントが自己申告する値ではなく、IR Serverが`scores`投稿履歴から集計する値として扱います。
Local BMZがローカル表示用に`play_count` / `clear_count`を保持していても、IR送信payloadには含めません。

`device_type`はBMS IRの慣習に合わせ、スコアを記録した主入力デバイスとして必ず送ります。
値は`keyboard` / `controller`の2種類だけにし、`mixed`は作りません。
BMZはプレイ中のhuman press inputを集計し、controller入力数がkeyboard入力数より多ければ`controller`、
それ以外は`keyboard`と判定します。
controllerのスクラッチだけkeyboard入力へ変換している環境でも、鍵盤側がcontrollerなら`controller`になります。
`device_type`はランキング分離キーではなく、表示・検索・検証補助用のscore metadataとして扱います。

```json
{
  "client": {
    "name": "BMZ",
    "version": "0.1.0",
    "platform": "windows-x86_64"
  },
  "chart": {
    "sha256": "chart_sha256...",
    "md5": "chart_md5...",
    "ln_profile": {
      "has_undefined_ln": true,
      "has_defined_ln": false,
      "has_defined_cn": false,
      "has_defined_hcn": false
    }
  },
  "rule": {
    "play_mode": "single",
    "key_mode": "7K",
    "gauge": "Normal",
    "ln_policy": "ForceLn",
    "effective_ln_mode": "ln",
    "judge_algorithm": "bmz_v1",
    "scoring": "bms_ex_score_v1"
  },
  "result": {
    "clear": "hard_clear",
    "played_at": "2026-06-04T12:34:56Z",
    "duration_ms": 123456,
    "judges": {
      "fast": {
        "pgreat": 500,
        "great": 100,
        "good": 20,
        "bad": 3,
        "poor": 2,
        "empty_poor": 1
      },
      "slow": {
        "pgreat": 480,
        "great": 90,
        "good": 15,
        "bad": 4,
        "poor": 2,
        "empty_poor": 1
      }
    },
    "ex_score": 2140,
    "avg_judge_ms": -1.42,
    "max_combo": 1234,
    "notes": 1800,
    "pass_notes": 1800,
    "min_bp": 12,
    "min_cb": 10
  },
  "play_options": {
    "option": "random",
    "seed": "123456789",
    "assist": "none",
    "device_type": "keyboard",
    "skin": "default"
  },
  "replay": {
    "hash": "replay_sha256...",
    "format": "bmz-replay-v1",
    "upload_intent": "later"
  },
  "evidence": {
    "schema": "bmz-score-evidence-v1",
    "canonical_hash": "sha256_of_canonical_submission",
    "client_signature": "base64url_signature",
    "public_key_id": "key_01H..."
  },
  "idempotency_key": "score_01H..."
}
```

### LN policy

BMZ本体は`docs/ln.md`の`LnScorePolicy`をlocal score DBの保存キーに使います。
IRでも同じ値をscore identityの一部として扱います。

送信payloadの`rule.ln_policy`はBMZがprofile policyとchart LN profileから正規化した結果で、保存値は次のいずれかです。

```txt
AutoLn
AutoCn
AutoHcn
ForceLn
ForceCn
ForceHcn
```

`rule.effective_ln_mode`は実際に降らせたLN種別の補助情報で、初期値は`ln` / `cn` / `hcn`のいずれかにします。
ランキング、best score、play_count / clear_countの集計キーは`effective_ln_mode`ではなく`ln_policy`を使います。

`chart.ln_profile`はサーバーが譜面metadataを補完・検証するための参考情報であり、best scoreの分離単位にはしません。
ただしchart registryには保存して構いません。

### Response: ランキングなし

```json
{
  "accepted": true,
  "score_id": "sc_01H...",
  "best_updated": true,
  "updated_fields": {
    "ex_score": true,
    "clear": false,
    "max_combo": true,
    "min_bp": false,
    "min_cb": false
  },
  "server_received_at": "2026-06-04T12:35:01Z"
}
```

`verification`とreplay hashはserver DBに保存されます。リプレイのupload URL
はscore submit responseには含めず、
`POST /api/v1/scores/{id}/replay/upload-url`で別途取得します。

同じplayer / `idempotency_key`で保存済みのプレイ内容と一致する投稿は、既存の
`score_id`を返し、履歴・bestを変更しません。譜面SHA256、ルール、成績、判定内訳、
プレイオプション、replay情報などが異なる場合は409 Conflictを返します。
retryごとに変わり得るevidenceやclient version、譜面の表示用metadataは比較しません。
旧形式のkeyも受け付けるが、衝突時は同じ409を返します。

新しいプレイの送信keyは`bmz-score-v2-<UUID v4>`をenqueue前に一度だけ生成し、
payloadに保存します。HTTP retryでは保存済みkeyを再利用します。ローカルscore IDは
keyの一意性には使いません。通常リザルト送信では既存スコアの再送や既存jobの再enqueueは行いません。
local backfillのkeyも`score_history`ごとの永続UUIDを使います。score DB migrationで既存履歴へ
keyを割り当て、新規履歴は保存transaction内で生成します。同じ履歴を再実行・DB再openしても
keyは変わらず、別プロファイルで同じhistory IDになってもkeyは衝突しません。プロファイルのDBを複製した
後で各profileに別々の履歴を追加した場合も、それぞれ新しいkeyを生成します。dry-runはkeyを
割り当てず、既存queueの再送では保存済みpayloadとkeyをそのまま使います。

### Response: ランキングあり

`rankings`はrequestされたscopeだけ含めます。

```json
{
  "accepted": true,
  "score_id": "sc_01H...",
  "best_updated": true,
  "updated_fields": {
    "ex_score": true,
    "clear": false,
    "max_combo": true,
    "min_bp": false,
    "min_cb": false
  },
  "server_received_at": "2026-06-04T12:35:01Z",
  "rankings": {
    "global": {
      "succeeded": true,
      "data": {
        "scope": "global",
        "sort": "ex_score_desc",
        "chart": {
          "sha256": "chart_sha256..."
        },
        "entries": [],
        "self": {
          "rank": 20,
          "score_id": "sc_01H...",
          "included_in_entries": true
        }
      }
    },
    "self_and_rivals": {
      "succeeded": true,
      "data": {
        "scope": "self_and_rivals",
        "sort": "ex_score_desc",
        "chart": {
          "sha256": "chart_sha256..."
        },
        "entries": [],
        "self": {
          "rank": 3,
          "score_id": "sc_01H...",
          "included_in_entries": true
        }
      }
    }
  }
}
```

### Ranking 部分だけ失敗した場合

スコア送信自体は成功扱いにします。

```json
{
  "accepted": true,
  "score_id": "sc_01H...",
  "best_updated": true,
  "updated_fields": {
    "ex_score": true,
    "clear": false,
    "max_combo": true,
    "min_bp": false,
    "min_cb": false
  },
  "server_received_at": "2026-06-04T12:35:01Z",
  "rankings": {
    "global": {
      "succeeded": true,
      "data": {
        "scope": "global",
        "entries": []
      }
    },
    "self_and_rivals": {
      "succeeded": false,
      "error": "Failed to fetch rival ranking"
    }
  }
}
```

### 重要な処理順序

ランキング同時取得時は、必ずbest更新後にランキングを取得します。

```txt
access token検証
  ↓
payload validation
  ↓
idempotency_key確認
  ├─ 重複なら既存 score を採用
  ↓
chart upsert/check
  ↓
score insert
  ↓
best_scores更新判定
  ↓
include=rankings なら ranking query 実行
  ↓
response返却
```

---

## 7. Ranking API

### Endpoint

```http
GET /api/v1/charts/{sha256}/ranking
```

### Query parameters

```txt
scope=global
limit=100
offset=0
scoring=bms_ex_score_v1
rule_mode=Beatoraja
```

| parameter | type | description |
|---|---|---|
| `scope` | string | `global`, `self_and_rivals`, `rivals`, `self`, `around_self` |
| `limit` | integer | 最大件数 |
| `offset` | integer | pagination用 |
| `ln_policy` | string | 任意です。指定した場合だけ`AutoLn` / `AutoCn` / `AutoHcn` / `ForceLn` / `ForceCn` / `ForceHcn`で絞り込む |
| `rule_mode` | string | 必須です。`Beatoraja` / `Lr2Oraja` / `Dx` |
| `scoring` | string | 初期は`bms_ex_score_v1` |

### Ranking scope

| scope | 内容 |
|---|---|
| `global` | 全体ランキング |
| `self_and_rivals` | 自分 + ライバル |
| `rivals` | ライバルのみです。自分は含めない |
| `self` | 自分だけ |
| `around_self` | 全体ランキング上で自分の周辺 |

ユーザーが希望している「ライバルと自分のみ」は`self_and_rivals`を使います。

### `rival_only` alias

正式には`scope`を推奨します。
互換・分かりやすさ用にaliasを受けても構いません。

```txt
rival_only=true&include_self=true  -> scope=self_and_rivals
rival_only=true&include_self=false -> scope=rivals
```

### Response

```json
{
  "chart": {
    "sha256": "chart_sha256..."
  },
  "rule": {
    "scoring": "bms_ex_score_v1",
    "ln_policy": "ForceLn",
    "effective_ln_mode": "ln",
    "double_option": "Off",
    "rule_mode": "Beatoraja"
  },
  "ranking": {
    "scope": "self_and_rivals",
    "sort": "ex_score_desc",
    "entries": [
      {
        "rank": 3,
        "scope_rank": 1,
        "player": {
          "id": "pl_self",
          "display_name": "Hyrorre"
        },
        "score": {
          "score_id": "sc_self",
          "clear": "clear",
          "ex_score": 3000,
          "max_combo": 1234,
          "min_bp": 12,
          "min_cb": 10,
          "device_type": "controller",
          "played_at": "2026-06-04T12:34:56Z",
          "option": "random",
          "seed": "123456789",
          "verification": "verified_play"
        },
        "stats": {
          "play_count": 42,
          "clear_count": 31
        },
        "relation": {
          "is_self": true,
          "is_rival": false
        }
      },
      {
        "rank": 7,
        "scope_rank": 2,
        "player": {
          "id": "pl_rival",
          "display_name": "RivalPlayer"
        },
        "score": {
          "score_id": "sc_rival",
          "clear": "hard_clear",
          "ex_score": 2800,
          "max_combo": 1000,
          "min_bp": 20,
          "min_cb": 18,
          "device_type": "keyboard",
          "played_at": "2026-06-03T12:00:00Z",
          "option": "mirror",
          "seed": "987654321",
          "verification": "verified_play"
        },
        "stats": {
          "play_count": 12,
          "clear_count": 9
        },
        "relation": {
          "is_self": false,
          "is_rival": true
        }
      }
    ],
    "self": {
      "rank": 3,
      "score_id": "sc_self",
      "included_in_entries": true
    },
    "pagination": {
      "limit": 100,
      "offset": 0,
      "has_more": false
    }
  }
}
```

### 順位の考え方

- `rank`は常に全体ランキング上の順位です。
- `scope_rank`はscope内での表示順位です。
- `self_and_rivals`でも`rank`は全体順位を返します。

例:

```txt
global ranking:
  1位 A
  2位 B
  3位 自分
  4位 C
  5位 D
  6位 E
  7位 ライバル

scope=self_and_rivals:
  rank=3, scope_rank=1 自分
  rank=7, scope_rank=2 ライバル
```

### play_count / clear_count

Ranking APIは必要ならentryごとに`stats.play_count` / `stats.clear_count`を返します。
これらは投稿payload由来ではなく、IR Serverが`scores`投稿履歴から集計します。

```txt
play_count  = 対象 player + chart + rule の accepted score 投稿数
clear_count = そのうち clear_type が Failed / NoPlay 以外の投稿数
```

集計単位はranking queryと同じ`chart_sha256` / `ln_policy` / `double_option` / `rule_mode` / `scoring`を基本にします。
`play_count` / `clear_count`が不要な画面ではresponseから省略して構いません。

### Device type

Ranking API / Chart detail APIはscore表示に`device_type`を含めます。
`device_type`は`keyboard` / `controller`の2値で、`mixed`は返しません。

集計・順位のkeyには含めません。
同じplayer / chart / ln_policy / double_option / rule_mode / scoringのbest scoreが更新された場合、`best_scores.device_type`は
そのbest scoreを記録した投稿の値で上書きします。

### EX score 同点順位

BMS IRらしさを考えると、順位はEX scoreのみで同点同順位にします。

```sql
RANK() OVER (
    ORDER BY bs.ex_score DESC
) AS rank
```

表示順の安定化には追加条件を使います。

```sql
ORDER BY
    rank ASC,
    clear_rank DESC,
    min_bp ASC,
    min_cb ASC,
    max_combo DESC,
    played_at ASC
```

---

## 8. Chart API / Chart Identity

### Endpoint

```http
PUT /api/v1/charts/{sha256}
GET /api/v1/charts/{sha256}
```

`PUT`にしておくと、スコア送信前にchart metadataをupsertできます。

### Chart identity

```rust
pub struct IrChartIdentity {
    pub sha256: String,
    pub md5: Option<String>,

    pub title: String,
    pub subtitle: Option<String>,
    pub genre: Option<String>,
    pub artist: Option<String>,
    pub subartists: Vec<String>,

    pub mode: BmsMode,

    pub level: Option<f32>,
    pub difficulty: Option<String>,
    pub total: Option<f32>,
    pub judge_rank: Option<f32>,

    pub min_bpm: f32,
    pub max_bpm: f32,

    pub notes: u32,
    pub ln_notes: u32,
    pub cn_notes: u32,
    pub hcn_notes: u32,
    pub mine_notes: u32,

    pub has_random: bool,
    pub has_stop: bool,
    pub has_undefined_ln: bool,
    pub has_defined_ln: bool,
    pub has_defined_cn: bool,
    pub has_defined_hcn: bool,
    pub has_ln: bool,
    pub has_cn: bool,
    pub has_hcn: bool,
    pub has_mine: bool,

    pub source_url: Option<String>,
    pub append_url: Option<String>,

    pub headers: BTreeMap<String, String>,
}
```

### Payload example

```json
{
  "sha256": "abc...",
  "md5": "def...",
  "title": "song title",
  "subtitle": null,
  "genre": "Artcore",
  "artist": "artist",
  "subartists": [],
  "mode": "7K",
  "level": 12,
  "difficulty": "ANOTHER",
  "total": 350.0,
  "judge": 100,
  "bpm": {
    "min": 150.0,
    "max": 180.0
  },
  "notes": {
    "total": 1800,
    "ln": 120,
    "cn": 0,
    "hcn": 0,
    "mine": 0
  },
  "features": {
    "random": false,
    "stop": true,
    "undefined_ln": true,
    "defined_ln": false,
    "defined_cn": false,
    "defined_hcn": false,
    "ln": true,
    "cn": false,
    "hcn": false,
    "mine": false
  },
  "urls": {
    "source": null,
    "append": null
  }
}
```

### Chart detail response stats

`GET /api/v1/charts/{sha256}`は必要ならchart全体、またはログイン中playerの
`play_count` / `clear_count`を返して構いません。これもScore Submission payloadの値ではなく、
IR Serverが`scores`投稿履歴から集計します。

例:

```json
{
  "chart": {
    "sha256": "abc..."
  },
  "stats": {
    "global": {
      "play_count": 1200,
      "clear_count": 840
    },
    "self": {
      "play_count": 42,
      "clear_count": 31
    }
  }
}
```

Chart detailにstatsが不要な画面では省略して構いません。

---

## 9. Replay 設計

### 初期方針

- replay本体アップロードはscore submitとは分離し、送信成功後に専用APIで
  upload URLを取得して実行します。
- Score submission payloadにはreplay hash / format / upload_intentを入れます。
- サーバーはreplay hashとstorage objectのhashを照合し、
  `replay_objects.status`で状態を管理します。

### Score submission の replay field

```json
{
  "replay": {
    "hash": "sha256...",
    "format": "bmz-replay-v1",
    "upload_intent": "later"
  }
}
```

`upload_intent`:

| value | description |
|---|---|
| `none` | replayなし |
| `later` | hashだけ送って、あとでアップロード予定 |
| `now` | score submit後に続けてアップロード予定 |

### Replay status

```txt
metadata_only
pending_upload
uploaded
verified
rejected
```

### Replay API

```http
POST /api/v1/scores/{score_id}/replay/upload-url
POST /api/v1/scores/{score_id}/replay/verify
GET  /api/v1/scores/{score_id}/replay
```

Response:

```json
{
  "upload_url": "signed_upload_url",
  "expires_in": 300,
  "required_hash": "sha256..."
}
```

Flow:

```txt
score submit成功
  ↓
score_id取得
  ↓
replay upload job作成
  ↓
upload endpoint URL取得
  ↓
Client uploads replay to BMZ replay upload endpoint
  ↓
Server verifies hash
  ↓
replay status = uploaded / verified
```

---

## 10. Tamper Evidence 設計

### 方針

- 完全なチート防止ではありません。
- 改造クライアントなら偽スコアに署名できます。
- 目的は「提出内容があとから改変されていない」ことを示します。
- 将来replay upload / replay verificationと組み合わせます。

### 署名対象

Canonical JSONまたはMessagePackなどで正規化します。
JSONの素朴な文字列化は避けます。

署名対象に含めるもの:

```txt
BMZ client name/version
platform
chart sha256
chart md5
ln_policy
effective_ln_mode
score fields
judge counts
clear
max combo
minbp
avgjudge
option
seed
assist
gauge
device type
judge algorithm
rule
skin
played_at
replay hash
```

`played_at`はクライアント時計由来なので、署名対象に入れる場合でも信用時刻として扱いません。
ランキングや監査の基準時刻には、サーバー側で付与する`server_received_at` / `created_at`を別に保存します。
クライアント時計が大きくずれている場合は、表示用の参考値として保持しつつwarning / suspicious flagを付けられるようにします。

含めないもの:

```txt
access token
refresh token
サーバー側で付与するID
サーバー受信時刻
ランキング順位
```

### Device key

初回スコア送信時、または`bmz ir device-key` / eguiの署名鍵操作時に
provider keyごとのdevice keyをlazy生成します。

```txt
private key: OS credential store
public key: serverに登録
```

署名:

```txt
canonical_payload = RFC 8785 JCS(payload without top-level evidence)
canonical_submission_hash = SHA256(canonical_payload)
signature = Ed25519(private_key, canonical_submission_hash)
```

サーバー側:

```txt
public keyで検証
canonical hashを保存
署名を保存
受信時刻を保存
```

### Verification status

```txt
unverified       署名なし
signed_backfill  local score.db history を後日device keyで署名
verified_play    通常プレイ結果をdevice keyで署名
```

リプレイの検証状態は`replay_objects.status`として別管理します。

---

## 11. EX Score / Judge model

### EX Score

beatorajaメモに合わせます。

```txt
(epg + lpg) * 2 + egr + lgr
```

BMZ内部関数案:

```rust
pub fn ex_score(j: &JudgeCounts) -> u32 {
    (j.fast_pgreat + j.slow_pgreat) * 2
        + j.fast_great
        + j.slow_great
}
```

### Judge counts

IR payloadは現行BMZの`ScoreState::JudgeCounts`に合わせます。
BMZでは`Miss`という判定名は使わず、見逃しは`Poor`、空押しは`EmptyPoor`として扱います。
`EmptyPoor`にもFAST/SLOWがあるため、IRでも丸めずに保存します。

```txt
fast.pgreat
fast.great
fast.good
fast.bad
fast.poor
fast.empty_poor

slow.pgreat
slow.great
slow.good
slow.bad
slow.poor
slow.empty_poor
```

将来FAST/SLOWに分類できない独立イベントが必要になった場合は、v1 payloadへ`neutral`を足すのではなく、
用途を明確にした別fieldとして追加します。

### BP / CB

BMZ本体では`miss_count`名を廃止し、BMS文脈のBP / CBをそのまま保存・表示名に使います。

```txt
bp = bad + poor + empty_poor
cb = bad + poor
```

`empty_poor`はcomboを切らないため、combo break集計の`cb`からは除外します。
IR v1では最小値として次を扱います。

```txt
min_bp = best play の最小 bp
min_cb = best play の最小 cb
```

BMZのlocal score DBは`score_history` / `score_best` / `replay_slots`に`bp` / `cb`を直接保存します。
本体側の対応方針:

- `ScoreState::bp()`は`bad + poor + empty_poor`、`ScoreState::cb()`は`bad + poor`を返します。
- `score_history` / `score_best` / `replay_slots`に`bp` / `cb`を保存し、表示・リプレイslot metricsでも`miss_count`名を使いません。
- local best更新条件は`ex_score`、`clear_type`、`bp`、`cb`、`max_combo`の順に比較します。
- IR側の`min_bp` / `min_cb`は、送信された`bp` / `cb`の自己ベスト最小値として扱います。

### Device type

BMZのlocal score DBは`score_history` / `score_best`に`device_type`を保存します。
`replay_slots`はscore historyのsnapshot表示に必要なら`device_type`を持ってよいが、IR payloadの元データは
`score_history.device_type`を正とします。

判定ルール:

```txt
human controller press count > human keyboard press count => controller
otherwise                                             => keyboard
```

- `mixed`は保存しません。
- `InputKind::Press`だけを数え、releaseによる二重カウントを避けます。
- `InputSource::Human`だけを数え、autoplay / replay再生入力はローカル実プレイのdevice判定に含めません。
- 旧replayやdevice情報のない入力は`keyboard` fallbackで構いません。

---

## 12. Clear Lamp

サーバー・クライアントで順序を固定します。IR v1はBMZの`ClearType::as_str()`に準拠します。
文字列表現はlocal DBと同じPascalCaseを送ります。

```rust
pub enum ClearType {
    NoPlay = 0,
    Failed = 1,
    AssistEasy = 2,
    LightAssistEasy = 3,
    Easy = 4,
    Normal = 5,
    Hard = 6,
    ExHard = 7,
    FullCombo = 8,
    Perfect = 9,
    Max = 10,
}
```

APIのhuman-friendly aliasとして`hard_clear`などを受ける場合でも、保存時・response時は
`ClearType::as_str()`の値へ正規化します。
BMS系ではアシスト、EASY、HARD、EXHARD、FULLCOMBO、MAXの扱いがズレやすいので、早めに固定します。

---

## 13. Server DB 設計案: NuxtHub / SQLite

### 主なテーブル

```txt
players
charts
scores
best_scores
rival_relationships
replay_objects
device_keys
```

### scores

投稿履歴をすべて保存します。`play_count` / `clear_count`はsubmission payloadから受け取らず、
この`scores`履歴からIR Server側で集計します。
`device_type`は投稿時点の主入力デバイスとして`scores.device_type`に保存します。

集計の基本条件:

```txt
player_id
chart_sha256
ln_policy
double_option
rule_mode
scoring
accepted = true
```

```txt
play_count  = COUNT(*)
clear_count = COUNT(*) FILTER (WHERE clear_rank > Failed)
```

`clear_rank > Failed`は`NoPlay` / `Failed`を除外する意味です。
Chart detailのglobal statsのようにplayer単位でない集計が必要な場合は、`player_id`条件を外します。
負荷が問題になるまでは`scores`から都度集計でよく、必要になったらmaterialized viewやsummary tableを検討します。

`device_type`は集計条件に含めません。
controller / keyboard別ランキングを作る場合はv1のscore identityとは別のfilter / viewとして追加します。

### best_scores

ランキング用に正規化します。

```sql
CREATE TABLE best_scores (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id uuid NOT NULL,
    chart_sha256 text NOT NULL,
    score_id uuid NOT NULL,

    ex_score integer NOT NULL,
    clear_type text NOT NULL,
    clear_rank integer NOT NULL,
    max_combo integer NOT NULL,
    min_bp integer NOT NULL,
    min_cb integer NOT NULL,
    device_type text NOT NULL,

    gauge text NOT NULL,
    ln_policy text NOT NULL,
    double_option text NOT NULL,
    rule_mode text NOT NULL,
    effective_ln_mode text NOT NULL,
    scoring text NOT NULL,

    played_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now(),

    UNIQUE (player_id, chart_sha256, ln_policy, double_option, rule_mode, scoring)
);
```

`scores`側にも同じ`device_type text NOT NULL`を持たせます。
`best_scores.device_type`はbest更新に採用された`scores.device_type`をコピーします。

`effective_ln_mode`は表示・互換・将来の検証用に保存するが、bestのunique keyには使いません。
BMZ local DBと同じく、score分離の正規化キーは`ln_policy`とします。

### scores と best_scores を分ける

```txt
scores:
  投稿履歴全部

best_scores:
  chart_id + player_id + rule ごとの現在ベスト
```

### best 更新条件

EX score ranking固定のため、基本はEX scoreを主軸にします。

```txt
1. EX score が高い
2. 同点なら clear が高い
3. 同点なら minbp が低い
4. 同点なら mincb が低い
5. 同点なら max combo が高い
6. 同点なら played_at が古い/新しい、どちらか仕様で固定
```

ただしbeatoraja的な`IR_SEND_UPDATE_SCORE`は次のいずれかが改善したら送信します。

```txt
EX score
clear
combo
minbp
mincb
```

そのため、将来は項目別bestを分けても構いません。

```txt
best_score_by_ex_score
best_clear
best_combo
best_minbp
best_mincb
```

初期は`best_scores`にランキング用bestを保存するだけで構いません。
BMZ本体のlocal DBは`bp` / `cb`を保存するため、IR payloadでも`min_bp` / `min_cb`をv1から必須項目として扱います。

---

## 14. Ranking SQL 案

### Global ranking

順位はEX scoreのみで同点同順位です。

```sql
WITH ranked AS (
    SELECT
        bs.*,
        p.display_name,
        RANK() OVER (
            ORDER BY bs.ex_score DESC
        ) AS rank
    FROM best_scores bs
    JOIN players p ON p.id = bs.player_id
    WHERE bs.chart_sha256 = $1
      AND bs.ln_policy = $2
      AND bs.double_option = $3
      AND bs.rule_mode = $4
      AND bs.scoring = 'bms_ex_score_v1'
)
SELECT *
FROM ranked
ORDER BY
    rank ASC,
    clear_rank DESC,
    min_bp ASC,
    min_cb ASC,
    max_combo DESC,
    played_at ASC
LIMIT $4 OFFSET $5;
```

### self_and_rivals

全体順位を維持したまま表示対象だけ絞ります。

```sql
WITH ranked AS (
    SELECT
        bs.*,
        p.display_name,
        RANK() OVER (
            ORDER BY bs.ex_score DESC
        ) AS rank
    FROM best_scores bs
    JOIN players p ON p.id = bs.player_id
    WHERE bs.chart_sha256 = $1
      AND bs.ln_policy = $2
      AND bs.double_option = $3
      AND bs.rule_mode = $4
      AND bs.scoring = 'bms_ex_score_v1'
),
targets AS (
    SELECT target_player_id AS player_id
    FROM rival_relationships
    WHERE owner_player_id = $4
      AND relation_type = 'rival'

    UNION

    SELECT $4 AS player_id
)
SELECT *
FROM ranked
WHERE player_id IN (SELECT player_id FROM targets)
ORDER BY
    rank ASC,
    clear_rank DESC,
    min_bp ASC,
    min_cb ASC,
    max_combo DESC,
    played_at ASC;
```

### rivals only

`self_and_rivals`から`UNION SELECT $4`を抜きます。

---

## 15. Rival model

### Table

```sql
CREATE TABLE rival_relationships (
    owner_player_id uuid NOT NULL,
    target_player_id uuid NOT NULL,
    relation_type text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (owner_player_id, target_player_id)
);
```

### 初期 relation_type

```txt
rival
```

### 将来候補

```txt
rival
favorite
blocked
mutual_rival
team_member
```

### API

```http
GET    /api/v1/rivals
POST   /api/v1/rivals
DELETE /api/v1/rivals/{player_id}
GET    /api/v1/rivals/{player_id}/comparison
```

Response:

```json
{
  "rivals": [
    {
      "player_id": "pl_01H...",
      "relation_type": "rival",
      "created_at": "2026-08-21T00:00:00.000Z",
      "profile": {
        "id": "pl_01H...",
        "display_name": "RivalPlayer",
        "bio": null
      }
    }
  ],
  "reverse_rivals": []
}
```

比較APIは、BMZのbest score identity
`chart_sha256 + ln_policy + double_option + rule_mode + scoring`が一致する共通譜面だけを対象に、
EX SCOREとMIN BPの勝敗・差分を返します。

---

## 16. Client local DB 設計案

### ir_accounts

現行実装では`provider`はIRサーバーが返す`provider_key`を保存します。
同じprovider実装名でも、production `bmz`とlocal `bmz-dev`は別provider
として扱います。

```sql
CREATE TABLE ir_accounts (
    provider TEXT NOT NULL,
    account_id TEXT NOT NULL,
    account_display_name TEXT NOT NULL DEFAULT '',
    role TEXT NOT NULL DEFAULT 'submit_only',
    enabled INTEGER NOT NULL DEFAULT 1,
    last_login_at INTEGER,
    last_success_at INTEGER,
    PRIMARY KEY(provider, account_id)
);
```

### ir_score_jobs

`provider`は送信先の`provider_key`です。pending jobはこの値で
`IrProviderConfig.provider_key`を引き、該当する`base_url`へ送信します。

```sql
CREATE TABLE ir_score_jobs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider TEXT NOT NULL,
    account_id TEXT NOT NULL DEFAULT '',
    kind TEXT NOT NULL DEFAULT 'score',
    local_score_id INTEGER NOT NULL,
    chart_sha256 TEXT NOT NULL,
    ln_policy TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    attempt_count INTEGER NOT NULL DEFAULT 0,
    next_attempt_at INTEGER NOT NULL DEFAULT 0,
    last_error TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    submission_key TEXT NOT NULL,
    UNIQUE(provider, account_id, kind, submission_key)
);

CREATE INDEX idx_ir_score_jobs_status_next_attempt
    ON ir_score_jobs(status, next_attempt_at);

CREATE INDEX idx_ir_score_jobs_local_score
    ON ir_score_jobs(local_score_id);
```

Status:

```txt
pending
sending
succeeded
failed
```

`pending` / `failed`は`next_attempt_at <= now`のとき送信対象です。
`sending`は通常はワーカーが処理中の印だが、プロセス終了などで残る可能性が
あるため、`updated_at`から300秒以上経過したものは再送対象にします。

### ir_score_submissions

```sql
CREATE TABLE ir_score_submissions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id INTEGER NOT NULL,
    provider TEXT NOT NULL,
    account_id TEXT NOT NULL DEFAULT '',
    local_score_id INTEGER NOT NULL,
    remote_score_id TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL,
    submitted_at INTEGER NOT NULL,
    response_json TEXT NOT NULL DEFAULT '',
    error TEXT NOT NULL DEFAULT '',
    FOREIGN KEY(job_id) REFERENCES ir_score_jobs(id) ON DELETE CASCADE
);

CREATE INDEX idx_ir_score_submissions_local_score
    ON ir_score_submissions(local_score_id);
```

`ir_score_success_ledger`は成功したjobのidempotency key、local / remote score IDを
`ir_score_jobs`への外部キーなしで保持します。jobの30日 / 最新500件pruningでは削除せず、
upload-local、download-scores、attestation、IR cleanupが成功履歴を参照します。
成功時のrequest payloadと譜面hash / LN policyも台帳へsnapshotするため、pruning後も
元の内容を保った`--resend`ができます。旧履歴のsnapshotが空なら安全に再構築できないため
再送不可とします。cleanupで対応するlocal scoreを削除するときは、submission/jobと同じ
transactionで台帳とpayload snapshotも消します。

### Local score device type

IR実装前準備として、BMZ本体の通常スコア保存にも`device_type`を持たせます。

```sql
ALTER TABLE score_history ADD COLUMN device_type TEXT NOT NULL DEFAULT 'keyboard';
ALTER TABLE score_best ADD COLUMN device_type TEXT NOT NULL DEFAULT 'keyboard';
```

`device_type`は`keyboard` / `controller`の2値に固定します。
local bestが更新された場合、`score_best.device_type`は更新元の`score_history.device_type`で上書きします。
IR job作成時は`score_history.device_type`からpayloadの`play_options.device_type`を生成します。

---

## 17. Client submit flow

### 通常リザルト

```txt
Result Scene
  ↓
Local score save
  ↓
Score comparison
  ↓
Create IR jobs for enabled providers
  ↓
Background worker
  ↓
Refresh access token if needed
  ↓
Submit score
  ↓
Save response
  ↓
If provider == primary:
       use included rankings if present
       otherwise fetch ranking lazily on tab switch
```

### Primary / Secondary の扱い

```txt
Primary IR:
  submit_score + optional rankings

Secondary IR:
  submit_score only
```

### Submit options 構築

```rust
let mut ranking_scopes = vec![];

if settings.prefetch_global_ranking_on_submit {
    ranking_scopes.push(IrRankingScope::Global);
}

if settings.prefetch_rival_ranking_on_submit {
    ranking_scopes.push(IrRankingScope::SelfAndRivals);
}

let submit_options = IrSubmitOptions {
    include_rankings: !ranking_scopes.is_empty(),
    ranking_scopes,
    ranking_limit: Some(100),
};
```

### Trait types

```rust
pub struct IrSubmitOptions {
    pub include_rankings: bool,
    pub ranking_scopes: Vec<IrRankingScope>,
    pub ranking_limit: Option<u32>,
}

pub struct IrSubmitResponse {
    pub accepted: bool,
    pub score_id: String,
    pub best_updated: bool,
    pub updated_fields: IrBestUpdatedFields,
    pub server_received_at: String,
    pub rankings: BTreeMap<IrRankingScope, IrScopedResponse<IrRanking>>,
}

pub struct IrRankingQuery {
    pub scoring: IrScoringRule,
    pub gauge: GaugeType,
    pub ln_policy: LnScorePolicy,
    pub scope: IrRankingScope,
    pub limit: u32,
    pub offset: u32,
}

pub enum IrRankingScope {
    Global,
    SelfAndRivals,
    Rivals,
    SelfOnly,
    AroundSelf,
}
```

---

## 18. Retry / Offline handling

### 方針

- スコア送信はjob queue化します。
- オフライン時や一時失敗時はretryします。
- `idempotency_key`を使い二重投稿を防ぎます。

### Retry schedule example

```txt
1分後
5分後
30分後
2時間後
24時間後
```

### 通信設定の将来案

```txt
IR送信:
  - 常に自動送信
  - リザルト画面のみ送信
  - 手動送信
```

初期は自動送信 + retryで構いません。

---

## 19. Course Score API

コーススコア投稿・ランキングは実装済みです。

### Server DB 設計 (2026-06-11)

`scores` / `best_scores`の規約 (clear_rank、verification、idempotency、
served timestamps)をコースにもそのまま流用します。BMZ localの
metadataは`library.db`の`courses`、プレイ結果はprofile-local
`score.db`の`course_scores` / `course_score_charts`と対応させます。

#### Course identity

- `course_hash`を主キー相当のidentityにします。
  ローカル`course_key`ではなくサーバー間で再現可能な値として、
  「譜面 sha256 の順序付きリスト + constraint 群」をcanonical JSON化した
  もののSHA256とする (tamper evidenceと同じ正規化規則)です。

```txt
course_hash = SHA256(canonical_json({
  "charts": ["sha256-1", "sha256-2", ...],   # プレイ順
  "constraints": {
    "class": ..., "speed": ..., "judge": ..., "gauge": ..., "ln": ...
  }
}))
```

- タイトルや出典URLはidentityに含めない (表示metadata)です。
- BMS-IR向けにはcanonical `course_hash`を置換せず、LR2互換の長い
  `course_key`を別フィールドで併送します。BMS-IRから取得した表はserver提供keyを
  そのまま保持し、それ以外は標準Dan headerと譜面順MD5から生成します。rianIR用hashは
  これとは独立して扱います。
- score identity (best / rankingの分離キー)は
  `course_hash + gauge + ln_policy + rule_mode + scoring`です。
  - `gauge`: 段位 (class系constraint)ではconstraintで固定されるが、
    通常コースではユーザー選択なのでkeyに含めます。
  - `ln_policy`: コース内全譜面の`ChartLnProfile`を集約し、コースLN constraint
    を反映して単曲と同じ規則で正規化した`LnScorePolicy`を使います。
  - `rule_mode`: 単曲スコアと同じく`Beatoraja` / `Lr2Oraja` / `Dx`を
    別ランキングとして扱います。

#### ir_courses (registry)

```sql
create table public.ir_courses (
  course_hash text primary key,
  title text not null default '',
  kind text not null default 'course',          -- 'dan' | 'course'
  charts jsonb not null,                        -- ["sha256", ...] プレイ順
  chart_count integer not null,
  constraints jsonb not null default '{}'::jsonb,
  source_url text,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now(),
  constraint ir_courses_hash_hex check (course_hash ~ '^[0-9a-f]{64}$'),
  constraint ir_courses_kind_known check (kind in ('dan', 'course')),
  constraint ir_courses_chart_count_positive check (chart_count > 0)
);
```

submit時に`scores`のchart upsertと同様にupsertします。
個々の譜面は`charts` registryに既存の仕組みでupsert済みである前提
(コース送信時に未登録chartがあればmetadataなしでsha256だけ登録)です。

#### course_scores (投稿履歴)

```sql
create table public.course_scores (
  id uuid primary key default gen_random_uuid(),
  player_id uuid not null references public.profiles(id) on delete cascade,
  course_hash text not null references public.ir_courses(course_hash),

  client_name text not null,
  client_version text not null,
  platform text not null,

  gauge text not null,                          -- Class / ExClass / Normal ...
  ln_policy text not null,                      -- 正規化済み LnScorePolicy 値
  rule_mode text not null,                      -- Beatoraja / Lr2Oraja / Dx
  scoring text not null,                        -- 'bms_ex_score_v1'

  clear_type text not null,                     -- ClearType::as_str()
  clear_rank integer not null,
  course_clear boolean not null,
  course_failed boolean not null,
  played_entries integer not null,              -- 実際にプレイした譜面数
  trophies jsonb not null default '[]'::jsonb,  -- 達成トロフィー名

  ex_score integer not null,
  max_ex_score integer not null,
  max_combo integer not null,
  bp integer not null,
  judges jsonb not null,                        -- コース合算 JudgeCounts
  gauge_value numeric not null,                 -- 最終ゲージ

  -- 譜面ごとの内訳。course_score_charts を別テーブルにせず jsonb で持つ。
  -- [{ "sha256": ..., "ex_score": ..., "max_combo": ..., "bp": ...,
  --    "gauge_end": ..., "clear": ... }, ...] プレイ順。
  entries jsonb not null,

  played_at timestamptz,
  server_received_at timestamptz not null default now(),
  device_type text not null,
  evidence jsonb not null default '{}'::jsonb,  -- bmz-course-score-evidence-v1
  verification text not null default 'unverified',
  accepted boolean not null default true,
  idempotency_key text not null,

  constraint course_scores_verification_known check (
    verification in ('unverified', 'signed_backfill', 'verified_play')
  ),
  constraint course_scores_device_known check (device_type in ('keyboard', 'controller')),
  unique (player_id, idempotency_key)
);

create index idx_course_scores_course
  on public.course_scores(course_hash, server_received_at desc);
```

`entries`をjsonbにするのはランキング集計に内訳を使わないためです。
譜面単位の分析が必要になったら正規化テーブルへ移します。

#### best_course_scores (ランキング用 best)

```sql
create table public.best_course_scores (
  id uuid primary key default gen_random_uuid(),
  player_id uuid not null references public.profiles(id) on delete cascade,
  course_hash text not null references public.ir_courses(course_hash),
  course_score_id uuid not null references public.course_scores(id),

  ex_score integer not null,
  clear_type text not null,
  clear_rank integer not null,
  course_clear boolean not null,
  max_combo integer not null,
  bp integer not null,
  device_type text not null,

  gauge text not null,
  ln_policy text not null,
  rule_mode text not null,
  scoring text not null,

  played_at timestamptz,
  server_received_at timestamptz not null,
  verification text not null default 'unverified',

  unique (player_id, course_hash, gauge, ln_policy, rule_mode, scoring)
);
```

best更新条件は単曲と同じ
`ex_score > clear_rank > bp(小) > max_combo`の順です。
段位らしさを優先するなら`clear_rank` (合格段位)を第一キーにする案も
あるが、IR全体の「EX score 主軸」を崩さない方を既定にします。
不正な署名は保存せず拒否します。
同じplayer / `idempotency_key`の重複投稿は既存の`course_score_id`を返し、
bestは再計算せず`best_updated=false`とします。

#### API

- `scores` / `best_scores`と同じ方針:
  selectは公開、書き込みはserver routeの認証済みAPIのみです。
- APIは本節冒頭の`POST /api/v1/course-scores`に加えて:

```http
GET /api/v1/courses/{course_hash}                # registry + 集計
GET /api/v1/courses/{course_hash}/ranking        # scope/gauge/ln_policy/rule_mode
```

- evidenceは`bmz-course-score-evidence-v1` schemaで、単曲と同じ
  Ed25519 device key / canonical JSONを使います。

#### クライアント側の対応 (実装済み)

- `CourseResultSummary` (course_session.rs)からpayloadを構築します。
  `course_hash`はcourse定義 (charts + constraints)から計算し、
  ローカル`courses.course_key`とは独立に持ちます。
- `ir_score_jobs.kind = 'course'`で単曲jobと同じqueueを使います。
  `local_score_id`はlocal course score id、`chart_sha256`はcourse hashを
  入れます。

```http
POST /api/v1/course-scores
```

Payload:

```json
{
  "client": {
    "name": "BMZ",
    "version": "0.1.0",
    "platform": "macos"
  },
  "course": {
    "course_hash": "sha256_of_course_definition",
    "title": "Example Course",
    "kind": "dan",
    "charts": ["chart1...", "chart2..."],
    "constraints": {
      "gauge": "Class",
      "ln": "AutoLn"
    }
  },
  "rule": {
    "gauge": "Class",
    "ln_policy": "AutoLn",
    "rule_mode": "Beatoraja",
    "scoring": "bms_ex_score_v1"
  },
  "result": {
    "clear": "Clear",
    "course_clear": true,
    "course_failed": false,
    "played_entries": 2,
    "trophies": [],
    "ex_score": 8000,
    "max_ex_score": 10000,
    "max_combo": 4000,
    "bp": 50,
    "judges": {
      "pgreat": 3000,
      "great": 500,
      "good": 100,
      "bad": 20,
      "poor": 25,
      "empty_poor": 5
    },
    "gauge_value": 82,
    "entries": [
      {
        "sha256": "chart1...",
        "ex_score": 4000,
        "max_combo": 2000,
        "bp": 20,
        "clear": "Clear",
        "gauge_end": 88
      },
      {
        "sha256": "chart2...",
        "ex_score": 4000,
        "max_combo": 2000,
        "bp": 30,
        "clear": "Clear",
        "gauge_end": 82
      }
    ],
    "played_at": 1780569296
  },
  "play_options": {
    "device_type": "keyboard"
  },
  "evidence": {
    "schema": "bmz-course-score-evidence-v1",
    "canonical_hash": "abc...",
    "client_signature": "..."
  },
  "idempotency_key": "course_score_01H..."
}
```

---

## 20. URL Resolver

beatorajaの`getSongURL`, `getCourseURL`, `getPlayerURL`相当です。

```rust
pub trait IrUrlResolver {
    fn chart_url(&self, chart: &IrChartIdentity) -> Option<Url>;
    fn course_url(&self, course: &IrCourseIdentity) -> Option<Url>;
    fn player_url(&self, player: &IrPlayerRef) -> Option<Url>;
}
```

BMZ公式IRの例:

```txt
https://ir.example.com/charts/{sha256}
https://ir.example.com/courses/{course_id}
https://ir.example.com/players/{player_id}
```

---

## 21. API 一覧: 初期実装優先度 (履歴)

この節は初期計画の履歴です。現在の実装済みAPIは冒頭の「実装状況」を正とします。

### Must have

```http
GET  /api/v1/me
PUT  /api/v1/charts/{sha256}
GET  /api/v1/charts/{sha256}
POST /api/v1/scores
GET  /api/v1/charts/{sha256}/ranking
GET  /api/v1/rivals
POST /api/v1/rivals
DELETE /api/v1/rivals/{player_id}
```

### Later

```http
POST /api/v1/scores/{score_id}/replay
GET  /api/v1/scores/{score_id}/replay
POST /api/v1/scores/{score_id}/replay/upload-url
GET  /api/v1/tables
GET  /api/v1/tables/{table_id}
POST /api/v1/course-scores
GET  /api/v1/courses/{course_id}/ranking
```

---

## 22. Nuxt + NuxtHub / Drizzle 実装方針

### Nuxt server routes

```txt
/server/api/v1/me.get.ts
/server/api/v1/charts/[sha256].put.ts
/server/api/v1/charts/[sha256].get.ts
/server/api/v1/scores.post.ts
/server/api/v1/charts/[sha256]/ranking.get.ts
/server/api/v1/rivals/index.get.ts
/server/api/v1/rivals/index.post.ts
/server/api/v1/rivals/[playerId].delete.ts
```

### Auth

- `nuxt-auth-utils` cookie sessionとBMZクライアント向けbearer access tokenを検証します。
- `users`と`profiles`を紐付けます。
- BMZクライアントはemail/password loginでaccess/refresh tokenを取得します。

### NuxtHub DB

- Schemaは`bmz-ir-web/server/db/schema.ts`のDrizzle schemaを正とします。
- MigrationはNuxtHub CLI / drizzle-kitの既定に合わせて
  `server/db/migrations/sqlite/`に生成します。
- server routeは`hub:db`のDrizzle client経由でDBを読みます。

### Transaction

Score submissionでは次をtransaction的に扱います。

```txt
score insert
best_scores upsert
ranking select
```

Nuxt server routeだけでつらくなったらRPC化します。

---

## 23. Provider Adapter 方針

BMZ内部では情報をリッチに持ちます。

```txt
BMZ Score
  - avg_judge_ms
  - replay_hash
  - device_type
  - rule
  - skin
  - signature
```

外部IRが受け取れない項目はAdapterで明示的に落とします。

```rust
impl MochaAdapter {
    fn to_mocha_score(&self, score: &IrScoreSubmission) -> MochaScorePayload {
        // BMZ固有項目は捨てる or meta/commentに詰める
    }
}
```

情報を落とす箇所はログ・コメントで明示します。

---

## 24. Codex 実装タスク案

### Phase 1: BMZ client model / trait

- `IrProvider` traitを追加します。
- `IrSubmitOptions` / `IrSubmitResponse` / `IrRankingQuery` / `IrRankingScope`を追加します。
- `IrChartIdentity` / `IrScoreSubmission` / `IrRanking`を定義します。
- リザルト画面用の`ResultRankingState`を追加します。
- IR設定項目を`profile.toml`側に追加します。

### Phase 2: Local DB / Job Queue

- BMZ本体のscore保存に`bp` / `cb`集計を追加します。
- `score_history` / `score_best` / replay slot metricsの`bp` / `cb`をIR payload / best updateに接続します。
- BMZ本体の入力pipelineに`keyboard` / `controller`のdevice kindを残し、human press inputの多数決で`device_type`を決めます。
- `score_history` / `score_best`に`device_type`を保存し、IR payloadの`play_options.device_type`へ接続します。
- ローカル表示用の`play_count` / `clear_count`をlocal score DBから集計または保存します。ただしIR送信値には使いません。
- `ir_accounts`
- `ir_score_jobs`
- `ir_score_submissions`
- retry / idempotency管理

### Phase 3: BMZ Official Provider

- OAuth/OIDC login flow連携です。
- token refreshです。
- Credential Store保存です。
- `POST /api/v1/scores`実装です。
- `GET /api/v1/charts/{sha256}/ranking`実装です。

### Phase 4: NuxtHub / Drizzle IR server

- `players`, `charts`, `scores`, `best_scores`, `rival_relationships`, `device_keys`, `replay_objects`を作成します。
- Score submission route実装です。
- `ln_policy`をscore / best / ranking / statsの分離キーとして実装します。
- `play_count` / `clear_count`はsubmission payloadから受け取らず、`scores`投稿履歴から集計します。
- Ranking route実装です。
- Ranking / Chart detail responseで必要に応じて`play_count` / `clear_count`を返します。
- Rival route実装です。
- `include=rankings&ranking_scopes=...`に対応します。

### Phase 5: Replay / Evidence

- canonical payload hash実装です。
- Ed25519 device key署名です。
- server-side signature verificationです。
- replay hash保存です。
- replay upload / download / verify API実装です。
- device key登録 / 一覧 / 失効API実装です。

---

## 25. 未決事項 / 要レビュー

Codexにレビューしてほしい点です。

1. `rankings` responseのwrapper形式
   - `rankings.global.succeeded/data`の形でよいか。
   - それとも`rankings.global = null | IrRanking`にしてerrorは別fieldにするか。

2. `best_scores`の採用条件
   - ランキング主軸はEX score固定です。
   - 同点時にclear / minbp / mincb / comboをbest更新判定に含めるか。
   - ランキング順位はEX scoreのみで同点同順位にします。
   - LN/CN/HCNの分離は`docs/ln.md`の`LnScorePolicy`準拠で固定し、`best_scores`のunique keyに含めます。

3. `scope_rank`の必要性
   - 初期から返すか。
   - UI側で計算するか。

4. replay upload API
   - 実装済みです。score submit responseにはupload URLを含めず、
     `POST /api/v1/scores/{id}/replay/upload-url`で取得します。
   - replay objectのhash検証は
     `POST /api/v1/scores/{id}/replay/verify`で行います。

5. tamper evidence
   - canonical JSONを採用するか。
   - MessagePack等のbinary canonical formatを採用するか。
   - Ed25519 keyの生成・保存・rotate・revokeをどう実装するか。
   - クライアント時計由来の`played_at`とサーバー受信時刻のズレをどう表示・警告するか。

6. NuxtHub / Drizzle route分担
   - submit + best update + ranking fetchをroute/service内で書きます。
   - D1 / Turso移行を見据え、DB固有RPCには寄せません。

---

## 26. 最終結論

```txt
BMZ IR API v1:
  - SHA256 primary
  - MD5 compatibility
  - EX score ranking fixed
  - Judge payload follows BMZ ScoreState: fast/slow + empty_poor, no miss field
  - Clear payload follows BMZ ClearType::as_str()
  - LN/CN/HCN score identity follows BMZ LnScorePolicy from docs/ln.md
  - BP means bad + poor + empty_poor; CB means bad + poor
  - min_bp / min_cb are required from v1 and are derived from submitted bp / cb
  - device_type is required from v1 as keyboard/controller only; mixed is not used
  - BMZ classifies device_type by human press input majority and stores it in local score_history / score_best
  - play_count / clear_count are not sent by Score Submission API
  - IR Server aggregates play_count / clear_count from scores history
  - Ranking API / Chart detail API may return play_count / clear_count when needed
  - Local BMZ may keep local-only play_count / clear_count, but never uses them as IR submitted values
  - Score submit response can optionally include rankings
  - Multiple ranking scopes can be requested at submit time
  - Ranking API supports global / self_and_rivals / rivals / self / around_self
  - Result screen caches rankings by scope
  - Missing ranking is fetched lazily when switching tabs
  - Ranking fetch failure does not fail score submission
  - Replay hash/format/status are included from v1
  - Replay upload is reserved for later
  - Tamper evidence uses canonical hash + client signature
  - played_at is client-clock data; server_received_at is the authoritative server timestamp
  - BMZ official IR is a reference implementation, not the core abstraction
  - BMZ core depends on IrProvider trait, not on official IR API directly
```

Recommended score submit variants:

```http
POST /api/v1/scores
POST /api/v1/scores?include=rankings&ranking_scopes=global&ranking_limit=100
POST /api/v1/scores?include=rankings&ranking_scopes=self_and_rivals&ranking_limit=100
POST /api/v1/scores?include=rankings&ranking_scopes=global,self_and_rivals&ranking_limit=100
```

Recommended lazy fetch variants:

```http
GET /api/v1/charts/{sha256}/ranking?scope=global&limit=100&rule_mode=Beatoraja
GET /api/v1/charts/{sha256}/ranking?scope=self_and_rivals&limit=100&rule_mode=Beatoraja
```

## 開発環境と実装の入口

Node.js / Bunを用意し、リポジトリrootから操作します。
コマンドの定義は[package.json](../package.json)、ディレクトリ対応とruntime設定は
[nuxt.config.ts](../nuxt.config.ts)を確認します。

### アプリ構成

| 場所 | 責務 |
|---|---|
| [bmz-ir-web/app](../bmz-ir-web/app/) | Nuxt UIです。pages、components、layouts、composables等 |
| [bmz-ir-web/server](../bmz-ir-web/server/) | Nitro API、middleware、services、repositories等 |
| [bmz-ir-web/shared](../bmz-ir-web/shared/) | app/server共通の型・schema・定数・純粋関数です。secretやDB queryは置かない |
| [bmz-ir-web/public](../bmz-ir-web/public/) | root URLから配信する静的ファイル |
| [Rust IR client](../crates/bmz-player/src/ir/) | client、credentials、device_key、sync、secret_store |
| [Rust IR CLI](../crates/bmz-player/src/ir_cmd.rs) | login / logout / status / ranking / sync / rivals / device-key / replay |

Result / Selectの連携は`crates/bmz-player/src/screens/result_ir.rs` / `select_ir.rs`、
skin IDの解決は`crates/bmz-render/src/skin/`を確認します。
秘密情報の保存先はprofileの`[ir] credential_store = "File" | "Os"`で選びます。
実装済みAPIとローカルでの操作例は、この文書冒頭の「実装状況」を参照してください。

### ローカルの準備

```bash
# Install dependencies
bun install

# Apply local SQLite migrations
bun run db:migrate

# Build Cloudflare Worker output and web dependency notices
bun run build

# Generate Cloudflare Worker bindings/types
bun run cf:types
```

`bun run build`はNuxt buildとWeb依存ライセンス生成を実行します。
Worker設定が必要な`cf:types`はbuild後に実行します。
開発サーバーは`bun run dev`です。現行scriptは`TMPDIR=/tmp nuxt dev`を使うため、
実行環境のshellと一時ディレクトリがこの指定に対応していることを確認します。

### DB / blob

- schemaの正は[bmz-ir-web/server/db/schema.ts](../bmz-ir-web/server/db/schema.ts)です。
  NuxtHub DB + Drizzle ORMを使います。
- migrationは[server/db/migrations/sqlite](../server/db/migrations/sqlite/)に集約します。
  schema変更時の生成は`bun run db:generate`、ローカル適用は`bun run db:migrate`です。
- Cloudflare向け設定はbuildが生成する`.output/server/wrangler.json`を使います。
  D1 bindingは`DB`、R2 bindingは`BLOB`です。配布先の定義は[wrangler.jsonc](../wrangler.jsonc)を確認します。
- replay blobは`hub:blob`経由で保存し、ローカルは`.data/blob`を使います。
  配布先のbinding・環境変数は設定と[.env.example](../.env.example)を確認します。
- `.env`、認証token、production dataはコミットしません。
  production / remoteへの破壊的書き込みは、対象と内容への明示的な承認後に実行します。

### 検証

変更対象のファイルに対してPrettierを確認し、型検査と関連テストを実行します。

```bash
bunx vue-tsc --noEmit
bun run test:ir
```

翻訳変更では`bun run test:i18n`も使います。Webのみの変更にRust全テストは原則不要です。
共通の作業・コミット規約は[AGENTS.md](../AGENTS.md)を参照します。
