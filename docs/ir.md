# BMZ IR（Internet Ranking）

BMZ公式IRのAPIと、BMZ Player側のIRクライアントの現在の仕様をまとめた文書です。
rianIRとの互換は[rianIR互換プロバイダー](rian-ir.md)、アシスト・特殊モードを含むスコアの保存・送信条件は[スコア保存・アシスト・IR送信条件](score-persistence.md)を参照してください。
設計時の案・未決事項・実装タスクの経緯は[BMZ IRの設計記録](../notes/2026/2026-06-04-bmz-ir-design.md)にあります。

## 概要

- 公式IRサーバーは、Nuxt + NuxtHub DB（Cloudflare D1）/ Drizzle ORMで実装しています。リプレイの本体はR2（NuxtHub blob）に保存します。
- BMZ Playerは公式IR APIに直接依存せず、IRごとのprovider実装を通して送信・取得します。公式IRもrianIR・BMS-IRも、同じqueueとproviderの仕組みで扱います。
- スコアは有効なすべてのIRへ送ります。ランキング・ライバル・難易度表の取得とIRページを開く操作は、primary providerだけで行います。
- クライアントの単曲・コースランキング通信は`ir/ranking.rs`に集約し、SelectとResultで共有します。画面ごとのキャッシュ・取得タイミング・表示状態は各画面で管理します。
- 譜面はSHA-256を主キーにします。MD5はbeatoraja系との互換と外部IRとの連携のために保持します。
- ランキングの主軸はEXスコアです。
- スコアを分ける単位（score identity）は`chart_sha256` / `ln_policy` / `double_option` / `rule_mode` / `scoring`です。`gauge`はプレイ設定・表示用に保存しますが、ランキングを分ける条件にはしません。
- LN / CN / HCNの区別は[LN / CN / HCNの扱い](ln.md)の`LnScorePolicy`に従います。
- 改ざん検出（evidence）は、提出内容が後から変わっていないことを示すための仕組みです。改造したクライアントによる不正を防ぐものではありません。

## API一覧

認証が必要なAPIは、Webのcookie sessionか、BMZ Player向けのbearer access tokenで呼び出します。

### 認証とアカウント

```http
POST   /api/v1/auth/register
POST   /api/v1/auth/login                      # email + password → provider_key + access / refresh token
POST   /api/v1/auth/refresh                    # refresh tokenのrotation + provider_key
POST   /api/v1/auth/logout                     # 提示した有効なtokenのセッショングループをログアウト
GET    /api/v1/me
GET    /api/v1/profile
PUT    /api/v1/profile                         # display_name / bio / daily_boundary_minutes
PUT    /api/v1/account/email
PUT    /api/v1/account/password
DELETE /api/v1/account
GET    /api/v1/sessions                        # ログイン中のsession一覧
DELETE /api/v1/sessions/{id}
GET    /api/v1/device-keys                     # 自分の署名鍵の一覧
POST   /api/v1/device-keys                     # 公開鍵の登録（同じ鍵は再利用）
DELETE /api/v1/device-keys/{id}                # 失効（revoked_at）
```

### スコアとリプレイ

```http
POST   /api/v1/scores                          # include=rankings&ranking_scopes=...に対応
GET    /api/v1/scores/{id}                     # スコアの詳細（公開）
POST   /api/v1/scores/{id}/attestations        # 送信済みscoreへの後付けの署名
POST   /api/v1/scores/delete-backfills         # 本人のlocal_backfill scoreをまとめて削除
GET    /api/v1/me/scores                       # 自分の送信履歴（download scores用）
POST   /api/v1/scores/{id}/replay/upload-url   # リプレイのupload先URLを発行
PUT    /api/v1/scores/{id}/replay/upload       # リプレイ本体のupload（Bearer認証と所有者の一致が必要）
POST   /api/v1/scores/{id}/replay/verify       # 保存した実体のhashを検証
GET    /api/v1/scores/{id}/replay              # download URLのmetadata（公開）
GET    /api/v1/scores/{id}/replay/raw          # リプレイ本体のdownload
```

### 譜面・コース・プレイヤー

```http
GET    /api/v1/charts                          # 譜面の一覧（?q=でタイトル検索）
GET    /api/v1/charts/{sha256}                 # 詳細 + play_count / clear_countの集計
GET    /api/v1/charts/{sha256}/ranking         # ランキング
GET    /api/v1/charts/{sha256}/self-scores     # 自分の履歴
GET    /api/v1/charts/by-md5/{md5}
GET    /api/v1/charts/lookup?md5=...           # MD5からSHA-256を引く
POST   /api/v1/course-scores                   # コーススコア（kind=dan / course）
GET    /api/v1/courses                         # コースの一覧（?q=）
GET    /api/v1/courses/{course_hash}           # registry + play_count
GET    /api/v1/courses/{course_hash}/ranking   # globalのみ
GET    /api/v1/courses/{course_hash}/self-scores
GET    /api/v1/players                         # プレイヤーの一覧（?q=）
GET    /api/v1/players/{id}                    # プロフィール + best scores
GET    /api/v1/daily                           # player / date / mode=allの日次成果
```

### ライバルと管理

```http
GET    /api/v1/rivals
POST   /api/v1/rivals                          # { target_player_id, action: add | remove }
DELETE /api/v1/rivals/{player_id}
GET    /api/v1/rivals/{player_id}/comparison   # score identityごとのEX / BP比較
POST   /api/v1/admin/difficulty-tables/sync    # 管理者による難易度表の即時同期
GET    /api/v1/admin/moderation/users          # 管理者: ユーザー検索（?q=&status=&limit=&offset=）
GET    /api/v1/admin/moderation/users/{id}     # 管理者: ユーザーの状態・直近の監査ログ
POST   /api/v1/admin/moderation/users/{id}/status   # 管理者: 送信停止・BAN・解除
GET    /api/v1/admin/moderation/charts         # 管理者: 譜面検索（?q=&field=&status=&limit=&offset=）
GET    /api/v1/admin/moderation/charts/{sha256}     # 管理者: 譜面の状態・同じMD5の参考情報
POST   /api/v1/admin/moderation/charts/status  # 管理者: 確認中・非公開・ブロック・解除（最大50件）
GET    /api/v1/admin/moderation/audit-logs     # 管理者: 監査ログ（?target_type=&target_id=&actor_id=&limit=&offset=）
```

`PUT /api/v1/charts/{sha256}`と、難易度表を公開・管理するAPIはありません。譜面はscore submitの中で登録します。

### 送信の制限

制限は15分ごとの枠で数え、超えた場合は429と`Retry-After`を返します。

| 対象 | user | IP |
| --- | --- | --- |
| score submit / course score / attestation | 6000 | 12000 |
| replayのupload-url / upload / verify（1件で3 request） | 900 | 1800 |
| backfillの削除（1 requestあたり最大19 score） | 6000 | 12000 |

## 認証と秘密情報

- BMZ Playerは、email + passwordで`/api/v1/auth/login`へログインし、access tokenとrefresh tokenを受け取ります。Webは`nuxt-auth-utils`のcookie sessionを使います。OAuth / OIDCのdevice flowは使いません。
- refresh tokenはrotation方式です。BMZ Player側はプロセス内のmutexでrefreshを直列化します。サーバーは直前に`rotated`になったtokenの再利用を並行retryとみなし、新しいtokenを含むsession全体の失効はしません。
- ログインとrefreshの応答は`provider_key`を返します。BMZ Playerはこの値を、credentials・device key・`ir_score_jobs.provider`・`primary_provider`の識別子に使い、URLから推測しません。ローカルのIRは`bmz-dev`、本番のIRは`bmz`を返します。サーバー側では`NUXT_IR_PROVIDER_KEY`で明示的に変更できます。

秘密情報（refresh token・access token・署名用の秘密鍵）は、プロファイルの`[ir] credential_store`で選んだ場所に保存します。

| 値 | 保存先 |
| --- | --- |
| `File`（既定） | プロファイル配下のJSONファイル（unixでは0600） |
| `Os` | OSのcredential store（Keychain / Credential Manager / Secret Service）。既存のファイルは初回のアクセス時に移します |

- OSのcredential storeでは、serviceを`bmz.ir.<provider_key>`（token）と`bmz.ir-device-key.<provider_key>`（署名鍵）、userをプロファイルディレクトリの絶対パスにします。同じマシンの複数のプロファイルが衝突しないようにするためです。
- 既定をファイルにしているのは、開発時にKeychainの許可ダイアログが出ないようにするためです。
- LinuxのビルドはSecret Service用にlibdbusが必要です。

## プロバイダーの設定

IRの設定はプロファイル単位の状態なので、本体の`config.toml`ではなく`data/profiles/<profile>/profile.toml`の`[ir]`に保存します。

```toml
[ir]
primary_provider = "bmz"
credential_store = "File"
prefetch_global_ranking_on_score_submit = true
prefetch_rival_ranking_on_score_submit = true

[[ir.providers]]
provider = "bmz"
provider_key = "bmz"
base_url = "https://bmz-player.hyrorre.workers.dev/"
enabled = true
account_display_name = "Hyrorre"
account_id = "..."
send_policy = "always"
role = "primary"

[[ir.providers]]
provider = "rian-ir"
base_url = "https://rianir.link/"
enabled = false
send_policy = "always"
role = "submit_only"

[[ir.providers]]
provider = "bms-ir"
base_url = "https://www.bms-ir.org"
enabled = false
send_policy = "always"
role = "submit_only"
```

- 先頭の3枠は、BMZ IR / rianIR / BMS-IRの固定枠です。無効にしてもプロファイルに残し、BMZ メニューでは削除・種別の変更ができません。
- 4枠目以降は、BMZ / rianIR互換のプロトコルを選ぶカスタムURL用です。プロトコルと`base_url`はログイン前に設定します。
- 保存するのは、`provider`、`provider_key`、`base_url`、`enabled`、`account_display_name`、`account_id`、`send_policy`、`role`、`last_login_at`、`last_success_at`です。token・client secret・署名用の秘密鍵は保存しません。
- `IrProviderConfig.provider`は表示名・実装の種別として残し、識別には`provider_key`を使います。

### 送信ポリシー

`send_policy`はbeatorajaの三択と同じです。既定は`always`です。

| 値 | 送る条件 |
| --- | --- |
| `always` | 共通の保存・送信条件を満たすリザルトを常に送る |
| `complete_song` | 最終ゲージが0より大きい場合だけ送る |
| `update_score` | 初回のプレイ、またはEXスコア・クリア・最大コンボ・BP・CBのどれかが自己ベストより良い場合だけ送る |

- 送信ポリシーはクライアント側の制御です。実効アシスト・autoplay・リプレイ再生・プラクティス・キーモード変換は、送信ポリシーより前に除外します。
- サーバーは受け取ったスコアを自分で比較してbestを更新します。送られたスコアを必ずbestとして採用するわけではありません。

### ランキングの事前取得

「スコア送信時に全体ランキングを取得する」（`prefetch_global_ranking_on_score_submit`）と「スコア送信時にライバルランキングを取得する」（`prefetch_rival_ranking_on_score_submit`）は、どちらも既定でONです。

| 全体 | ライバル | スコアの送信時 | タブの切り替え時 |
| --- | --- | --- | --- |
| OFF | OFF | スコアの送信だけ | 初めて開いたランキングをその都度取得 |
| ON | OFF | `global`だけを取得 | `self_and_rivals`は切り替え時に取得 |
| OFF | ON | `self_and_rivals`だけを取得 | `global`は切り替え時に取得 |
| ON | ON | `global`と`self_and_rivals`を取得 | どちらもすぐ切り替えられる |

取得済みのランキングはscopeごとに保持します。ランキングの取得に失敗しても、スコアの送信は失敗にしません。

## スコアの送信

```http
POST /api/v1/scores
POST /api/v1/scores?include=rankings&ranking_scopes=global,self_and_rivals&ranking_limit=100
```

| パラメータ | 内容 |
| --- | --- |
| `include` | `rankings`を含めると、送信と同時にランキングを返す |
| `ranking_scopes` | カンマ区切りのscope。省略時は`global` |
| `ranking_limit` | 各ランキングの最大件数。1〜200、既定100 |

### payload

```json
{
  "client": { "name": "BMZ", "version": "0.4.3", "platform": "windows" },
  "chart": {
    "sha256": "chart_sha256...",
    "md5": "chart_md5...",
    "ln_profile": {
      "has_undefined_ln": true,
      "has_defined_ln": false,
      "has_defined_cn": false,
      "has_defined_hcn": false
    },
    "title": "song title",
    "artist": "artist",
    "mode": "7K",
    "level": 12,
    "total": 350.0,
    "judge": 100,
    "bpm": { "min": 150.0, "max": 180.0 },
    "notes": { "total": 1800, "ln": 120, "cn": 0, "hcn": 0, "mine": 0 },
    "features": { "random": false, "stop": true, "ln": true, "cn": false, "hcn": false, "mine": false }
  },
  "rule": {
    "play_mode": "single",
    "key_mode": "7K",
    "gauge": "Normal",
    "ln_policy": "ForceLn",
    "effective_ln_mode": "ln",
    "judge_algorithm": "bmz_v1",
    "scoring": "bms_ex_score_v1",
    "rule_mode": "Beatoraja"
  },
  "result": {
    "clear": "Hard",
    "played_at": 1780569296,
    "duration_ms": 123456,
    "judges": {
      "fast": { "pgreat": 500, "great": 100, "good": 20, "bad": 3, "poor": 2, "empty_poor": 1 },
      "slow": { "pgreat": 480, "great": 90, "good": 15, "bad": 4, "poor": 2, "empty_poor": 1 }
    },
    "ex_score": 2140,
    "max_combo": 1234,
    "notes": 1800,
    "min_bp": 12,
    "min_cb": 10
  },
  "play_options": {
    "device_type": "keyboard",
    "option": "random",
    "arrange_1p": "random",
    "arrange_2p": "off",
    "double_option": "off",
    "applied_double_option": "off",
    "source_kind": "local",
    "seed": "123456789",
    "seed_scheme": "beatoraja_24bit_v1",
    "rule_mode": "Beatoraja"
  },
  "replay": { "hash": "replay_sha256...", "format": "bmz-replay-v1", "upload_intent": "later" },
  "evidence": {
    "schema": "bmz-score-evidence-v1",
    "canonical_hash": "sha256_of_canonical_submission",
    "client_signature": "base64url_signature",
    "public_key_id": "key_..."
  },
  "idempotency_key": "bmz-score-v2-<UUID v4>"
}
```

サーバーが必須として確認する項目は次のとおりです。

- `client` / `chart` / `rule` / `result` / `play_options` / `idempotency_key`
- `chart.sha256`（小文字の16進64桁）。`chart.md5`を送る場合は小文字の16進32桁
- `rule.ln_policy`（`AutoLn` / `AutoCn` / `AutoHcn` / `ForceLn` / `ForceCn` / `ForceHcn`）、`rule.rule_mode`（`Beatoraja` / `Lr2Oraja` / `Dx`）、`rule.effective_ln_mode`（`ln` / `cn` / `hcn`）、`rule.scoring`（`bms_ex_score_v1`）
- `result`の`ex_score` / `max_combo` / `notes` / `min_bp` / `min_cb`と、`judges.fast` / `judges.slow`の各判定数（0以上の整数）
- `play_options.device_type`（`keyboard` / `controller`）

各項目の扱いは次のとおりです。

- `played_at`は、BMZ Playerからはunix秒で送ります。ISO文字列も受け付け、サーバーで正規化して保存します。クライアントの時計に由来する値なので、順位や監査の基準時刻にはサーバーが付ける`server_received_at`を使います。
- `play_count` / `clear_count`は送りません。サーバーが投稿履歴から集計します。
- `device_type`はスコアを記録した主な入力デバイスです。判定のルールは[判定・EXスコア・クリアランプ](#判定exスコアクリアランプ)を参照してください。ランキングを分ける条件ではなく、表示・検索・検証の補助に使います。
- `rule.ln_policy`は、プロファイルの設定と譜面のLN情報からBMZが正規化した値で、ランキング・best・集計の単位に使います。`rule.effective_ln_mode`は実際に降らせたLNの種別を示す補助情報です。`chart.ln_profile`はサーバーが譜面情報を補完・検証するための参考情報で、bestを分ける単位にはしません。
- `play_options.double_option`はランキングを分ける単位で、`off` / `battle` / `battle_auto_scratch`のどれかです（`flip`は`off`として扱います）。実際に適用したDPオプションは`applied_double_option`（`off` / `flip` / `battle` / `battle_auto_scratch`）に入れ、個別のスコアの履歴に残します。
- `play_options.option`は初期のpayloadとの互換用で、`arrange_1p`と同じ値です。
- `seed` / `random_seed`は10進数の文字列で送ります。数値も過去のpayloadとの互換のために受け付けます。
- `result.ghost`には、通常のプレイでbeatoraja形式のghostを入れます。
- `gauge`は、保存値・返却値を`AssistEasy` / `Easy` / `Normal` / `Hard` / `ExHard` / `Hazard` / `Class` / `ExClass` / `ExHardClass`にそろえます。小文字や旧名称も受け付けます。

### idempotency key

- 新しいプレイのkeyは、queueへ入れる前に`bmz-score-v2-<UUID v4>`を一度だけ生成し、payloadに保存します。HTTPのretryでは保存済みのkeyを使います。ローカルのscore IDは、keyの一意性には使いません。
- 通常のリザルトの送信では、既存スコアの再送や既存jobの再enqueueはしません。
- local backfillのkeyは、`score_history`ごとに永続化したUUIDを使います。score DBのmigrationで既存の履歴にkeyを割り当て、新しい履歴は保存のtransaction内で生成します。同じ履歴を再実行したり、DBを開き直したりしてもkeyは変わりません。別のプロファイルで同じhistory IDになっても、keyは衝突しません。プロファイルのDBを複製した後に、それぞれで別の履歴を追加した場合も、新しいkeyを生成します。
- dry-runはkeyを割り当てません。既存のqueueからの再送では、保存済みのpayloadとkeyをそのまま使います。
- 同じプレイヤー・同じ`idempotency_key`で、保存済みのプレイ内容と一致する投稿には、既存の`score_id`を返し、履歴とbestは変えません。譜面のSHA-256、ルール、成績、判定の内訳、プレイオプション、リプレイ情報などが異なる場合は409 Conflictを返します。retryごとに変わり得るevidence、client version、譜面の表示用metadataは比較しません。旧形式のkeyも受け付けますが、衝突時は同じく409を返します。

### 応答

```json
{
  "accepted": true,
  "score_id": "...",
  "best_updated": true,
  "updated_fields": { "ex_score": true, "clear": false, "max_combo": true, "min_bp": false, "min_cb": false },
  "server_received_at": "2026-06-04T12:35:01Z",
  "previous_best": { "clear_type": "Normal", "ex_score": 2100, "max_combo": 1200, "min_bp": 15, "min_cb": 12 },
  "rankings": {
    "global": { "succeeded": true, "previous_rank": 25, "data": { "...": "Ranking APIと同じ形式" } },
    "self_and_rivals": { "succeeded": false, "error": "Failed to fetch rival ranking" }
  }
}
```

- `rankings`には、要求したscopeだけが入ります。scopeごとの取得に失敗した場合は`succeeded: false`と`error`を返し、スコアの送信は成功として扱います。
- `previous_rank`は、送信したスコアを除いた自己ベストを、応答を作った時点の全体ランキングへ仮に置いたときの順位です。初回のプレイでは`0`です。リザルトスキンの`NUMBER_IR_PREVRANK(182)`に使います。
- 不正な署名や不正なpayloadは400で拒否します。
- リプレイのupload URLは応答に含めません。`POST /api/v1/scores/{id}/replay/upload-url`で別に取得します。

サーバーは次の順で処理します。ランキングは必ずbestを更新した後に取得します。

1. access tokenと送信の制限を確認する
2. payloadを検証する
3. アカウントと譜面の制限を確認する（[アカウント・譜面の制限](#アカウント譜面の制限)。拒否した場合はscore・譜面を作らない）
4. `idempotency_key`を確認する（重複なら既存のscoreを使う）
5. 譜面を登録・確認する
6. scoreを保存する
7. bestを更新するか判定する
8. `include=rankings`ならランキングを取得する
9. 応答を返す

## ランキング

```http
GET /api/v1/charts/{sha256}/ranking?scope=global&limit=100&offset=0&rule_mode=Beatoraja
```

| パラメータ | 内容 |
| --- | --- |
| `scope` | `global`（既定） / `self_and_rivals` / `rivals` / `self` / `around_self` |
| `limit` | 最大件数。1〜200、既定100 |
| `offset` | ページング用。0〜100000、既定0 |
| `ln_policy` | 省略可。指定した場合だけ、その`LnScorePolicy`で絞り込む |
| `double_option` | `off`（既定） / `battle` / `battle_auto_scratch` |
| `rule_mode` | 省略可。`Beatoraja` / `Lr2Oraja` / `Dx`。省略時と`ALL`はすべてのルールを対象にする |
| `scoring` | `bms_ex_score_v1`だけに対応 |

| scope | 内容 |
| --- | --- |
| `global` | 全体のランキング |
| `self_and_rivals` | 自分 + ライバル |
| `rivals` | ライバルだけ（自分を含めない） |
| `self` | 自分だけ |
| `around_self` | 全体のランキングで、自分の前後5件ずつ。未ログインや自分のスコアがない場合は`global`と同じ |

### 順位と並び順

- 条件に合うbestをプレイヤーごとに1行へまとめます。`ln_policy`や`rule_mode`を省略して複数のbestがある場合は、項目ごとに最も良い値を合わせます。
- 並び順は、EXスコアの降順、クリアの降順、BPの昇順、CBの昇順、最大コンボの降順、プレイ日時の昇順です。
- `rank`は常に全体ランキングでの順位で、EXスコアが同じなら同じ順位です。`self_and_rivals`などでも`rank`は全体の順位を返します。
- `scope_rank`はscope内での表示順（`offset`からの通し番号）です。

例えば全体で3位の自分と7位のライバルだけがいる場合、`self_and_rivals`では次のようになります。

```txt
rank=3, scope_rank=1  自分
rank=7, scope_rank=2  ライバル
```

### 応答

```json
{
  "chart": { "sha256": "chart_sha256..." },
  "rule": { "scoring": "bms_ex_score_v1", "ln_policy": "ForceLn", "effective_ln_mode": "ln", "double_option": "off", "rule_mode": "Beatoraja" },
  "ranking": {
    "scope": "self_and_rivals",
    "sort": "ex_score_desc",
    "clear_rate": 72,
    "entries": [
      {
        "rank": 3,
        "scope_rank": 1,
        "player": { "id": "...", "display_name": "Hyrorre" },
        "score": {
          "score_id": "...",
          "clear": "Normal",
          "ex_score": 3000,
          "max_combo": 1234,
          "min_bp": 12,
          "min_cb": 10,
          "gauge": "Normal",
          "ln_policy": "ForceLn",
          "double_option": "off",
          "rule_mode": "Beatoraja",
          "device_type": "controller",
          "arrange_1p": "random",
          "played_at": "2026-06-04T12:34:56Z",
          "verification": "verified_play",
          "judges": { "fast": { "...": 0 }, "slow": { "...": 0 } },
          "source_score_ids": { "ex_score": "...", "clear": "...", "max_combo": "...", "min_bp": "...", "min_cb": "..." }
        },
        "relation": { "is_self": true, "is_rival": false }
      }
    ],
    "self": { "rank": 3, "score_id": "...", "included_in_entries": true, "entry": { "...": "entriesと同じ形式" } },
    "pagination": { "limit": 100, "offset": 0, "total": 2, "has_more": false }
  }
}
```

- `clear_rate`は、全プレイヤーのうちNoPlay / Failedを除いたプレイヤーの割合（%）です。スコアがない場合は`null`です。
- `pagination.total`はscope内の総件数です。
- `score.judges`は、EXスコアが最も高いプレイの判定の内訳です。過去の行にはありません。
- `score.source_score_ids`は、EXスコア・クリア・最大コンボ・BP・CBのそれぞれの最良値を記録したscoreです。

## 譜面

- 譜面は、score submitの中で登録・更新します。既存の譜面の正規のmetadataは、local backfillでは上書きしません。
- `GET /api/v1/charts/{sha256}`は、譜面の情報と、全体・ログイン中の本人それぞれの`play_count` / `clear_count`（`stats.global` / `stats.self`）を返します。
- `play_count`は、対象の投稿のうち受け付けた数、`clear_count`はそのうちクリアがNoPlay / Failed以外の数です。どちらもサーバーが投稿履歴から集計し、payloadの値は使いません。

## リプレイ

1. score submitのpayloadの`replay`に、hash・形式（`bmz-replay-v1`）・`upload_intent`（現在は`later`）を入れます。
2. 送信が成功してscore IDを受け取ったら、BMZ Playerは`kind=replay`のjobを作ります。scoreの成功履歴の保存とreplay jobの投入は、同じtransactionで確定します。
3. `POST /api/v1/scores/{id}/replay/upload-url`でupload先を取得します。応答は`upload_url` / `path` / `required_hash`です。submitでhashを申告していないscoreには409を返します。
4. `PUT /api/v1/scores/{id}/replay/upload`へリプレイ本体を送ります。
5. `POST /api/v1/scores/{id}/replay/verify`で、保存した実体のSHA-256と申告したhashを比べます。一致すれば`verified`、一致しなければ`rejected`になります。

- リプレイの状態は`replay_objects.status`（`metadata_only` / `pending_upload` / `uploaded` / `verified` / `rejected`）で管理します。スコアの`verification`とは別です。
- `GET /api/v1/scores/{id}/replay`は、`uploaded` / `verified`のリプレイについて、`download_url` / `expires_in`（300秒） / `hash` / `format` / `size_bytes` / `status`を返します。リプレイはランキングと同じく公開情報として扱います。
- upload / verifyに失敗した場合は、replay jobだけを再試行し、成功したscoreを未送信には戻しません。
- `bmz-player ir replay <SCORE_ID>`でダウンロードし、`bmz-player --boot-replay-file <PATH>`で再生できます。

## 改ざん検出（evidence）

- device keyは、初回のスコア送信時、または`bmz-player ir device-key`やBMZ メニューの署名鍵の操作をしたときに、provider keyごとにlazyに生成します。秘密鍵は[秘密情報](#認証と秘密情報)の保存先に、公開鍵はサーバーに登録します。
- canonical formは、payloadからトップレベルの`evidence`を除き、キーを昇順に並べて空白なしで文字列化したJSONです（RFC 8785 JCSと同じ規則）。数値はECMAScriptの`JSON.stringify()`と同じ表現になるため、`160.0`と`160`は同じbytesになります。コースでは小数を送らず、ゲージを整数の%に丸めます。
- 署名は`Ed25519(秘密鍵, SHA256(canonical form))`です。`evidence`には`schema`（単曲は`bmz-score-evidence-v1`、コースは`bmz-course-score-evidence-v1`）、`canonical_hash`、`client_signature`（base64url）、`public_key_id`を入れます。
- サーバーは、登録済みで失効していない公開鍵で検証し、hashと署名を保存します。署名が不正な場合は保存せずに400で拒否します。

スコアの`verification`は次の3つです。

| 値 | 意味 |
| --- | --- |
| `unverified` | 署名がない |
| `signed_backfill` | ローカルの`score.db`の履歴を、後からdevice keyで署名した（`play_options.submission_source = "local_backfill"`） |
| `verified_play` | 通常のプレイ結果をdevice keyで署名した |

署名のない送信済みのscoreには、`POST /api/v1/scores/{id}/attestations`（schemaは`bmz-score-attestation-v1`）で後から署名を付けられます。score本文は送り直さず、所有者のdevice keyでscore IDに署名して`verification`だけを更新します。

## 判定・EXスコア・クリアランプ

- EXスコアは`(FAST PGREAT + SLOW PGREAT) * 2 + FAST GREAT + SLOW GREAT`です。
- 判定数はBMZの`ScoreState`のJudgeCountsと同じで、FAST / SLOWそれぞれの`pgreat` / `great` / `good` / `bad` / `poor` / `empty_poor`です。BMZでは「Miss」という判定名は使わず、見逃しは`poor`、空押しは`empty_poor`です。`empty_poor`にもFAST / SLOWがあるため、丸めずに保存します。
- BPは`bad + poor + empty_poor`、CBは`bad + poor`です。`empty_poor`はコンボを切らないため、CBには含めません。IRの`min_bp` / `min_cb`は、送られたBP / CBの自己ベストの最小値です。
- `device_type`は、プレイ中の人による押下入力（`InputSource::Human`の`InputKind::Press`）を数え、コントローラーの押下数がキーボードより多ければ`controller`、それ以外は`keyboard`にします。`mixed`はありません。スクラッチだけをキーボード入力へ変換している環境でも、鍵盤がコントローラーなら`controller`になります。device情報のない古いリプレイは`keyboard`として扱います。
- ローカルのscore DBは、`score_history` / `score_best` / `replay_slots`に`bp` / `cb`を、`score_history` / `score_best`に`device_type`を保存します。IRのpayloadは`score_history`の値から作ります。BMZでは`miss_count`という名前は使いません。

クリアは`ClearType::as_str()`のPascalCaseで送り、保存・応答もこの値にそろえます。`hard_clear`などの別名も受け付けます。
サーバーでbestを比べるときの順位（`clear_rank`）は次のとおりです。

| クリア | `clear_rank` |
| --- | --- |
| `NoPlay` | 0 |
| `Failed` | 1 |
| `AssistEasy` / `LightAssistEasy` | 2 |
| `Easy` | 3 |
| `Normal` | 4 |
| `Hard` | 5 |
| `ExHard` | 6 |
| `FullCombo` | 7 |
| `Perfect` | 8 |
| `Max` | 9 |

### bestの更新

サーバーは、score identityごとに次のbestを保持します。

- ランキングの代表となるスコアは、EXスコアが高い、同点ならクリアが高い、BPが低い、CBが低い、最大コンボが高い、の順で比べます。
- クリア・最大コンボ・BP・CBは、それぞれの最良値と、その値を記録したscoreを別に保持します（beatorajaの`IR_SEND_UPDATE_SCORE`と同じく、項目ごとに更新します）。
- `best_scores.device_type`は、代表のスコアを記録した投稿の値で上書きします。

## ライバル

- `POST /api/v1/rivals`の本文は`{ "target_player_id": "...", "action": "add" | "remove" }`です。自分自身は登録できません。
- `GET /api/v1/rivals`は、登録したライバル（`rivals`）と、自分をライバルに登録しているプレイヤー（`reverse_rivals`）を返します。各行は`player_id` / `relation_type`（`rival`） / `created_at` / `profile`です。
- 比較API（`/rivals/{player_id}/comparison`）は、score identity（`chart_sha256 + ln_policy + double_option + rule_mode + scoring`）が一致する共通の譜面だけを対象に、EX SCOREとMIN BPの勝敗・差分を返します。

## コーススコア

- コースのidentityは`course_hash`です。プレイ順の譜面のSHA-256の配列と制約から`{"charts": [...], "constraints": {...}}`を作り、evidenceと同じ規則でcanonical JSONにしたもののSHA-256です。タイトルや出典URLは含めません。サーバーは受け取った定義から再計算し、一致しない場合は拒否します。
- ローカルの`courses.course_key`とは独立に計算します。BMS-IR向けには、LR2互換の長い`course_key`を`course.course_key`として一緒に送ります。BMS-IRから取得した表はサーバーが提供したkeyをそのまま使い、それ以外は標準のDan headerと譜面順のMD5から作ります。rianIR用のhashとは別です。
- score identityは`course_hash + gauge + ln_policy + rule_mode + scoring`です。段位（class系の制約）ではゲージが制約で決まりますが、通常のコースではユーザーが選ぶため、gaugeを含めます。`ln_policy`は、コース内の全譜面のLN情報とコースのLN制約から、単曲と同じ規則で正規化します。
- bestは、EXスコア、クリア、BP（小さいほうが良い）、最大コンボの順で比べます。
- 同じプレイヤー・同じ`idempotency_key`の重複投稿には、既存の`course_score_id`を返し、bestは再計算せず`best_updated=false`にします。
- `GET /api/v1/courses/{course_hash}/ranking`はglobalだけで、`gauge` / `ln_policy` / `rule_mode`（それぞれ`ALL`で全体）と`limit`（1〜200、既定100）で絞り込めます。認証付きの取得では、`limit`の範囲外でも`ranking.self`に自分の順位を返します。`ranking.pagination.total`は条件に合う全プレイヤー数です。BMZ Playerは、ログイン済みならコースランキングも認証付きで取得し、Course Resultの自分の順位と参加人数に使います。
- BMZ Playerは、コースの終了時に`CourseResultSummary`からpayloadを作り、`ir_score_jobs`へ`kind=course`のjobを入れます。`local_score_id`にはローカルのcourse score ID、`chart_sha256`にはcourse hashを入れます。
- 未登録の譜面がコースに含まれる場合は、metadataなしでSHA-256だけを登録します。
- 難易度表（tables）の専用の実装は、まだありません。

```json
{
  "client": { "name": "BMZ", "version": "0.4.3", "platform": "macos" },
  "course": {
    "course_hash": "sha256_of_course_definition",
    "title": "Example Course",
    "kind": "dan",
    "charts": ["chart1...", "chart2..."],
    "constraints": { "gauge": "Class", "ln": "AutoLn" }
  },
  "rule": { "gauge": "Class", "ln_policy": "AutoLn", "effective_ln_mode": 1, "rule_mode": "Beatoraja", "scoring": "bms_ex_score_v1" },
  "result": {
    "clear": "Normal",
    "course_clear": true,
    "course_failed": false,
    "played_entries": 2,
    "trophies": [],
    "ex_score": 8000,
    "max_ex_score": 10000,
    "total_notes": 5000,
    "max_combo": 4000,
    "bp": 50,
    "judges": { "pgreat": 3000, "great": 500, "good": 100, "bad": 20, "poor": 25, "empty_poor": 5 },
    "gauge_value": 82,
    "entries": [
      { "sha256": "chart1...", "ex_score": 4000, "max_combo": 2000, "bp": 20, "clear": "Normal", "gauge_end": 88 },
      { "sha256": "chart2...", "ex_score": 4000, "max_combo": 2000, "bp": 30, "clear": "Normal", "gauge_end": 82 }
    ],
    "played_at": 1780569296
  },
  "play_options": { "device_type": "keyboard", "option": "random", "seed": "123456789", "entry_randomizations": [] },
  "evidence": { "schema": "bmz-course-score-evidence-v1", "canonical_hash": "...", "client_signature": "...", "public_key_id": "..." },
  "idempotency_key": "..."
}
```

## 日次成果と難易度表

- 日次成果は`/api/v1/daily?player=<id>&date=YYYY-MM-DD&mode=all`で公開します。`player`を省略すると、ログイン中の本人を対象にします。
- 成果日の切り替わりは、プロフィールの`daily_boundary_minutes`（JST 00:00からの分、既定0）で設定できます。
- 難易度表は、運用側のallowlistを6時間ごとにD1へ世代単位で同期し、日次APIの返却時にMD5優先、SHA-256をfallbackとしてラベルを付けます。
- 難易度表を即時に同期する管理APIは、ログイン済みのユーザーのうち`NUXT_IR_ADMIN_USER_IDS`（カンマ区切り）に含まれるIDだけが実行できます。未設定の場合は全員を拒否します。
- Cronと管理APIが重なった場合は、D1の期限付きのロックで片方を`already_running`としてスキップします。
- 同期の失敗は、Cloudflare LogsへURLと原因を最大10件、各500文字まで記録します。全件が失敗した場合は、最初の原因を管理APIの502エラーにも含めます。

## アカウント・譜面の制限

不正なスコアの送信者や、公開に問題がある譜面を、管理者がサーバー側で制限する仕組みです。
制限はすべてサーバーで判定し、クライアントの表示や設定に頼りません。自動検出・通報・外部DBとの連携はまだありません。

### 状態

ユーザー（`user_moderation`）と譜面（`chart_moderation`）は、行が無ければ`active`として扱います。既存のテーブルには列を足していません。

| 対象 | 状態 | 新規送信 | ログイン・token | 公開の表示 |
| --- | --- | --- | --- | --- |
| ユーザー | `active` | 可 | 可 | 表示 |
| ユーザー | `submit_suspended` | 403 `ACCOUNT_SUSPENDED` | 可 | 表示（過去のスコアは残る） |
| ユーザー | `banned` | 403 `ACCOUNT_BANNED` | login / refreshは403、既存のsessionとrefresh tokenは失効 | ランキング・プレイヤー一覧・プロフィール・統計・ライバル比較から除外 |
| 譜面 | `active` | 可 | - | 表示 |
| 譜面 | `under_review` | 可 | - | 表示（管理者の確認用の印） |
| 譜面 | `hidden` | 403 `CHART_HIDDEN` | - | 譜面一覧・詳細・MD5検索・ランキング・プロフィールのbest・日次成果・ライバル比較から除外 |
| 譜面 | `blocked` | 403 `CHART_BLOCKED` | - | `hidden`と同じ |

- 送信の判定は、score submit・course score・attestation・replayのupload-url / upload / verifyで行います。ユーザーを先に、次に譜面を確認し、複数の譜面が該当すると`blocked`を優先します。
- 判定は`idempotency_key`の照合より前に行います。制限中に同じ内容を再送しても、以前に受理されたscoreの応答は返しません。拒否したrequestでは譜面もscoreも作りません。
- コーススコアは、コースに含まれる譜面のどれかが`hidden` / `blocked`なら拒否します。単曲の制限をコース経由で回避できません。非公開の譜面を含むコースは、コースの一覧・詳細・ランキングからも除外します。
- MD5だけを根拠に、別のSHA-256の譜面を自動で制限することはしません。管理画面では、同じMD5の別の譜面を参考情報として表示するだけです。タイトル・アーティストによる自動の制限もありません。
- 本人は、自分の送信履歴（`/api/v1/me/scores`、self-scores）とscoreの詳細・リプレイを、制限中も閲覧できます。他のユーザーには404を返します。
- 管理者のアカウントは制限できず、管理者は自分の状態を変更できません。
- IPアドレスでの恒久的なBANや、端末のfingerprintは使いません。既存の送信の制限（429）とCSRFの対策はそのまま有効です。

### 非公開データの扱いと復元

- 制限は読み取り時の条件（`NOT EXISTS`の相関subquery）で反映し、`scores` / `best_scores`の行は変更・削除しません。順位・件数・クリア率・ページングは、除外した後の結果で計算します。
- 解除（`active`へ戻す）すると、再計算をせずに以前のスコアとランキングがそのまま表示に戻ります。`accepted`とは別の扱いのため、元から不受理だったスコアは解除後も表示しません。
- BANで失効したsessionとrefresh tokenは復元しません。解除後に、ユーザーがログインし直します。
- 失効済みのrefresh tokenが使われた場合、全セッションを失効させる（再利用検知）のは、rotation済みのtokenが5分の猶予を過ぎて使われたときと、再利用検知で失効したtokenが使われたときだけです。ログアウト・パスワード変更・BANで失効したtokenは拒否するだけで、その後のログインで作られたセッションには影響しません。ログアウト・個別のセッション失効では対象のセッショングループ、パスワード変更・BANではそのユーザーの全セッションについて、それより前にrotation済み（または再利用検知済み）のtokenも同じ失効理由に付け替えます。そのため、古い世代のtokenが後から使われても拒否されるだけで、その後のログインで作られたセッションや、対象外の端末のセッションは失効しません。その後に発行されたtokenと、対象外のセッショングループでは、再利用検知が通常どおり働きます。失効済みのtokenを提示したログアウトでは付け替えません。
- refreshでは、旧tokenのrotationを「旧tokenが有効でユーザーがBANされていない」ことを条件にした1文で、新しいtokenの組の追加を「旧tokenがこのrotationの結果のまま残っていてBANされていない」ことを条件にした1文で行います。D1は書き込みを1文ずつ原子的に適用するため、BANがどの時点に割り込んでも、BAN後に有効なtokenは残らず、BANによる失効をrotationで上書きしません。同じtokenでの並行refreshは1つだけが成功します。新しいtokenの追加に失敗した場合はtokenを返さず、ユーザーはログインし直します。
- ログイン・登録のtokenの組も、「ユーザーがBANされていない」ことを条件にした1文で追加します。BANの確認の後にBANが確定した場合はtokenを発行せず`ACCOUNT_BANNED`を返し、追加の後にBANが確定した場合はBANがそのtokenを失効させます。
- BANによるセッションの失効と付け替えは、書き込み時点でもユーザーがBAN中の場合だけ行います。refreshがBANを読み取った後に解除・再ログインがあっても、解除後のセッションは失効しません（そのrefreshには、読み取った時点の結果として403を返します）。
- ログアウト（`POST /api/v1/auth/logout`）は、Bearerのaccess token、bodyの`refresh_token`、Webのcookie sessionのうち、有効（失効しておらず期限内）なものが属するセッショングループ全体を失効させます。グループはサーバー側でtokenから求め、他の端末のセッションには触れません。refreshと同時に実行されても、refreshが先なら新しいtokenも同じグループとして失効し、rotationと新しいtokenの追加の間にログアウトが確定すれば追加が成立しません。失効済み・期限切れのtokenだけを提示した場合は何も変更しませんが、クライアントがローカルのtokenを破棄できるよう、応答は常に成功です。
- 制限の判定とscoreの保存は、同じrequestの中で続けて行いますが、1つのtransactionではありません。判定の直後に制限した場合、そのrequestのscoreは保存されることがあります。そのscoreは制限より前に送られたものと同じ扱いになり、制限中は公開の表示から除外され、解除後に表示されます。
- 公開APIの応答には、制限の理由・メモ・管理者のIDやメールアドレスを含めません。制限中の対象は「存在しない」（404）として扱います。

### エラーコード

制限による拒否は、どれもHTTP 403で、応答bodyの`data.code`に次のコードを入れます。`statusMessage`は英語の固定文です。

| `data.code` | `statusMessage` |
| --- | --- |
| `ACCOUNT_SUSPENDED` | Score submission is suspended for this account |
| `ACCOUNT_BANNED` | This account is banned from BMZ IR |
| `CHART_HIDDEN` | Score submission is temporarily disabled for this chart |
| `CHART_BLOCKED` | Score submission is blocked for this chart |

`data.code`の無い403（権限不足など）と、401は、従来どおり認証のエラーです。

### 管理者の設定と管理画面

- 管理者は、サーバーの`NUXT_IR_ADMIN_USER_IDS`（カンマ区切りのユーザーID、runtime configの`ir.adminUserIds`）だけで決めます。未設定の場合は全員を拒否します。requestのbodyやqueryにある操作者のIDは使いません。
- 管理APIは未ログインに401、管理者以外に403を返します。`/api/v1/me`は`is_admin`と`account_status`を返し、Webのナビゲーションに管理画面を出すかどうかだけに使います。
- Webの`/admin/moderation`（noindex）には、ユーザー・譜面・監査ログのタブがあります。
  - ユーザー: ID・表示名・メールアドレスで検索し、送信停止・BAN・解除を行います。
  - 譜面: SHA-256・MD5・タイトル・アーティストで検索し、確認中・非公開・ブロック・解除を行います。未登録のSHA-256も、先に制限できます。ページをまたいで選択し、最大50件をまとめて変更できます。結果は譜面ごとに`changed` / `unchanged` / `conflict`で表示します。選択した譜面は、選択した時点の`revision`で送ります。一覧を読み込み直しても`revision`は更新せず、他の管理者が先に変更した譜面は`conflict`として選択から外し、最新の状態を確認して選択し直すまで送りません。
  - 監査ログ: 対象の種別・ID・操作者で絞り込みます。
- 変更には理由（最大1000文字）が必須で、確認ダイアログを経由します。ダイアログを開いた時点の対象と`revision`を固定し、詳細の読み込み中や送信中は操作できません。選択を切り替えると古い詳細は破棄し、最後に選んだ対象の応答だけを表示します。譜面には内部向けのメモ（最大2000文字）も付けられます。
- 各行は`revision`を持ち、画面が読んだ`expected_revision`と現在値が違う場合は409（譜面の一括変更では`conflict`）で上書きを防ぎます。同じ状態への変更は`unchanged`で、監査ログを増やしません。同じrequestの再送は安全です。

運用の手順は次のとおりです。

1. 管理者にするユーザーのIDを`NUXT_IR_ADMIN_USER_IDS`へ設定してdeployする
2. 管理者のアカウントでWebにログインし、`/admin/moderation`を開く
3. 対象を検索し、状態と理由を入力して確定する（BANはそのユーザーのsessionを即時に失効させる）
4. 解除は同じ画面で「解除（active）」を選ぶ。表示はすぐに元に戻る
5. 経緯は監査ログのタブで確認する

### 監査ログ

`moderation_audit_logs`は追記専用で、SQLiteのtriggerがUPDATE / DELETEを拒否します。

- 1件の状態変更ごとに、操作者、対象の種別とID、操作（`user_suspend` / `user_ban` / `user_restore` / `chart_review` / `chart_hide` / `chart_block` / `chart_restore`）、変更前後の状態、理由、metadata（一括変更の`batch_id`、譜面のメモなど）、時刻を記録します。
- 状態の更新、監査ログの追加、BAN時のsessionの失効は、D1の`batch`で同時に適用します。どれかが失敗した場合は、どれも適用しません。並行した変更では、`revision`の条件で片方だけが成功し、ログも1件だけ残ります。
- 監査ログのAPIは、操作者と対象の表示名を返しますが、メールアドレスは返しません。

### migration

`server/db/migrations/sqlite/0015_moderation.sql`は、`user_moderation` / `chart_moderation` / `moderation_audit_logs`の3つのテーブル、index、追記専用のtriggerを追加します。既存のテーブルとデータは変更しません。

- ローカルでは`bun run db:migrate`で適用します。
- 本番のD1への適用は、運用者が承認した上で既存のmigrationの手順で行います。適用の前に`bun run db:export:remote`などでbackupを取り、適用後に3つのテーブルが空で作られたことを確認します。全ユーザー・全譜面は`active`のまま始まります。
- migrationより先に新しいWorkerをdeployすると、制限の判定と公開APIの条件が存在しないテーブルを参照して失敗します。migrationを先に適用します。
- rollbackが必要な場合も、既存のテーブルには影響しません。新しいテーブルを削除するとすべての制限が解除されますが、監査ログも失われるため、先にexportします。

### ユーザーへの案内

制限されたユーザーには、BMZ PlayerとWebで「送信が停止されている」「BANされている」ことだけを表示し、理由は表示しません。問い合わせ先は、運用側で決めたBMZ IRの窓口を案内します。ローカルのスコアは保持されるため、解除された後に手動で再送できます。

## BMZ Playerのクライアント

### CLI

`bmz-player ir login|logout|status|ranking|sync|upload-local|download-scores|attest-submitted|cleanup-imported|cleanup-duplicate|rivals|device-key|replay`です。
「BMZ メニュー → IR」からもログインできます。

### 送信queue

- リザルトの確定時に、譜面の時間を`result.duration_ms`へ入れ、送信ポリシーを判定してから`network.db`の`ir_score_jobs`へjobを入れます。単曲は`kind=score`、コースは`kind=course`、リプレイは`kind=replay`です。
- リザルト画面ですぐに送信し、残りはアプリに常駐するworkerが30秒間隔で処理します。
- リザルト画面と常駐workerは、1バッチ20件、job間を3.1秒空けて送ります（`IR_SYNC_BATCH_LIMIT` / `IR_SYNC_JOB_SPACING_MS`）。CLIの`ir sync`とlocal backfillは、手動での確認を速くするため100件 / 200msを使います（`IR_CLI_SYNC_BATCH_LIMIT` / `IR_CLI_SYNC_JOB_SPACING_MS`）。
- バッチを取得するときに対象のjobをまとめて`sending`としてclaimするため、リザルトの送信と常駐workerが並行しても、同じjobを二重に送りません。
- CLIの`ir sync`と`ir upload-local --sync`は、中断時に未処理のjobをまとめて`sending`に残さないよう1件ずつclaimし、`[N/100]`の進捗を逐次表示します。Ctrl+Cで残る`sending`は処理中の最大1件で、5分後に再び取得します。

jobの状態は`pending` / `sending` / `succeeded` / `failed`です。

- `pending` / `failed`は、`next_attempt_at`を過ぎると送信の対象になります。
- `sending`は通常はworkerが処理中であることを示しますが、プロセスの終了などで残ることがあるため、`updated_at`から300秒（lease）を過ぎたものは再び送信の対象にします。
- 再試行の間隔は、1分 → 5分 → 30分 → 2時間 → 以降24時間です。429が`Retry-After`を返した場合は、通常のbackoffより優先してその時刻まで待ちます。
- 成功したjobはpayloadを空にし、同期の後に、30日以内または最新の500件だけを残します。未送信・失敗・送信中のjobは削除しません。
- サーバーが制限のコード（[エラーコード](#エラーコード)）で拒否したjobは、`failed`のまま`next_attempt_at`を自動再試行が届かない時刻（`IR_JOB_NO_AUTO_RETRY_AT`、9999年末）にし、`last_error`の末尾に`[CHART_BLOCKED]`などのコードを残します。payloadとローカルのスコアは消しません。送信キューの画面では再試行の時刻を表示しません。
- アカウントの停止・BANを受けたバッチでは、同じprovider / accountの残りのjobを送らずに同じ扱いにします。token refreshがBANで拒否された場合も、ログインし直しを促す認証エラーではなく、制限として扱います。5xx・通信障害・429は、従来どおりbackoffで再試行します。
- 制限が解除された後は、送信キューの再送（個別・失敗分の一括）で送り直せます。再送はbackoffを無視するため、制限中のjobも対象になります。

成功の判定とremote score IDの紐付けは、jobとログの保持期間とは別に、成功台帳（`ir_score_success_ledger`）にも保存します。

- 台帳は、upload-localの送信済みの判定、download-scoresの既存の履歴へのリンク、attestation、IR cleanupで使います。
- 成功時には、元のrequest payload、譜面のhash、LN policyも同じtransactionで台帳へ保存します。jobを削除した後でも、元の内容のまま`--resend`できます。成功した履歴ごとに、元のrequest bodyの分の容量を使います。
- 元のpayloadが残っていない古い成功履歴は、本文を作り直さず再送不可として数え、CLIは非ゼロで終了します。
- IR cleanupでは、snapshotも台帳と同じtransactionで削除します。
- migrationでは、残っている成功submissionから台帳を作ります。以前に削除済みのsubmissionにしかなかったremote IDと紐付けは復元できません。

### primary providerとランキングの取得

- `primary_provider`を明示している場合、ランキングの取得・表示はそのproviderだけを使い、無効・未設定でも別のproviderへ切り替えません。空の旧設定だけは、先頭の有効なproviderを使います。
- submit-onlyのproviderにはスコアを送りますが、送信の応答へランキングを含めるよう要求しません。
- リザルト画面でのランキングAPIの取得は、primaryの今回分の送信が終わってから始めます。secondary providerやbacklogの送信は待ちません。primaryのjobを別の同期taskがclaim済みの場合も、そのjobだけを待ちます。
- リザルト画面のオーバーレイには、送信対象なし・成功・失敗をproviderごとに表示します。

### local backfill

`bmz-player ir upload-local [--dry-run] [--limit N] [--sync] [--all] [--resend] [--include-course-stages] [--include-replay]`は、`score.db`の既存の`score_history`を、`local_backfill`のscore submitとしてqueueへ入れます。

候補の選び方は次のとおりです。

- 送信済みの履歴と、queueにある履歴は既定で飛ばします。
- `download-scores`で同じprovider / accountから取得したscoreも、IRにscoreがあるため候補から除き、`--resend`でも入れ直しません。別のprovider・別のaccountへの送信は候補になります。取得元の印はBMZ IR / BMS-IR共通の`bmz_ir`で、providerとaccountも照合します。
- コース中の単曲とautoplayは既定で除きます。コース中の単曲は`--include-course-stages`で含められます。
- 値を解釈できない履歴や、譜面情報・リプレイファイルを読めない履歴は、その行だけを飛ばし、`invalid`として件数と理由を出力します。ほかの履歴の投入は続け、1件以上あれば最後に非ゼロで終了します。`--all`では、同じ実行の中で失敗した行を読み直しません。
- local backfillは未登録の譜面を作成できますが、既存の譜面の正規のmetadataは上書きしません。

投入と同期の動作は次のとおりです。

- 投入のbatchの既定は200件です。
- `--sync`で既存の未完了のscore jobがある場合は、queueを増やさず、先に最大100件を同期します。同期で失敗したjobが残った場合や、他のプロセスの`sending` jobだけが残って進まない場合は、非ゼロで終了します（失敗したjobはqueueに残ります）。
- `--all`は`--sync`を含む完走モードで、既存のqueueの送信と次のbatchの投入を、候補がなくなるまで繰り返します。履歴は`(played_at, id)`の順に512行ずつ読み、次のbatchは前のbatchが止まった行の次から読みます。
- dry-run（GUIでの件数の見積もりを含む）は、譜面の解析情報を読まず、リプレイはhashを計算せず存在だけを確かめます。
- 1回の実行で、各jobは最大1回だけ送ります。実行の開始前から残っている`failed` jobは、backoffを無視して1回だけ再送し、実行中に失敗したjobは取り直しません。
- 送信に失敗したjobはqueueに残したまま次へ進み、全件を処理した後に失敗の件数を報告して非ゼロで終了します。失敗したjobは、常駐workerのbackoffに従う再試行、送信キューからの再送、次回の`--all`で再び送られます。
- 他のプロセスの`sending` jobだけが残って進まない場合も非ゼロで終了します。5分のleaseの後に再実行してください。
- upload-localの同期の対象と進捗の件数は、指定したprovider / accountのscore jobだけです。

次の場合は途中で打ち切り、非ゼロで終了します。

- 401 / 403、未ログイン、providerの設定の不備：以降のjobも失敗するため、すぐ止めます。
- 429：サーバーの送信制限に従い、すぐ止めます。
- それ以外の失敗（4xxの内容エラー、5xx、通信エラー）が10件続いた場合（`IR_MANUAL_CONSECUTIVE_FAILURE_LIMIT`）：サーバーの障害中に全jobのbackoffを伸ばさないためです。成功が1件あれば数え直します。

`--resend`の動作は次のとおりです。

- 失敗したjobと成功snapshotを、同じidempotency keyとrequest payloadでqueueへ入れ直します。入れ直したjobは試行回数を0に戻し、次に失敗したときのbackoffを初回（1分）から数え直します。
- 既存の`pending` jobと有効な`sending`のleaseは変更しません。`enqueued`は実際に入れ直した件数です。
- `--all --resend`は、1回の実行で各履歴を最大1回だけ処理します。
- `--dry-run --resend`は状態を書き換えず、元のpayloadが足りずに再送できない履歴も別のカウンターで報告します。

payloadと署名は次のとおりです。

- 送信の直前にdevice keyで署名し、サーバーでは`signed_backfill`として保存します。
- `source_kind`、1P / 2Pの配置、実際に適用したDPオプションも送り、ランキング用の`double_option`とは別に、個別のスコアの履歴に保持します。
- `--include-replay`を付けると、score payloadにリプレイのupload intentを付け、scoreが成功した後にreplay jobを作ります。replay jobの送信は、upload-localでは行わず、通常のworkerか`ir sync`が処理します。
- `pass_notes`は保存済みの判定数から作り、`empty_poor`は実ノーツの判定数に含めません。Failedのスコアでは、未処理のノーツの分を`min_bp` / `min_cb`に含めて送るため、download-scoresは同じ`pass_notes`からローカルの履歴を復元できます。
- 以前のbackfillで`pass_notes == notes`のまま送ったFailedのスコアは、BP / CBの増分が同じ正の数で、そこから推定した処理済みの数が判定数と一致する場合だけ復元します。一致しない履歴は取り込まず、エラーにします。
- 履歴ごとのghostは、現在の`score_history`に保持していないため送りません。

### 送信済みscoreへの署名（attestation）

`bmz-player ir attest-submitted --all`は、署名のない送信済みのscoreに、ローカルの成功台帳に残るremote score IDを使って後から署名を送ります。

- 成功台帳は、jobやログを削除した後もremote score IDを保持します。
- 成功台帳を追加する前にjobを削除してremote score IDを失った古いDBの履歴は、復元できないため対象外です。これに対応するには、サーバー側で一括してattestationするAPIが必要です。

### download scores

`bmz-player ir download-scores [--limit N] [--dry-run]`は、IRにある自分のscoreをローカルへ取り込みます。

- 自分が送信したscoreは新しい履歴として取り込まず、既存のローカルの履歴に`bmz_ir`の取得元を紐付けます。
- 照合では、成功台帳のremote score IDを優先します。引けない場合は、`GET /api/v1/me/scores`が返す投稿時の`idempotency_key`を、成功台帳・未完了のjobの`submission_key`、local backfillの`score_history_ir_identity`の順に照合します。
- 応答を失った送信、score IDのない応答、送信中のjob、台帳を追加する前に削除された履歴も、重複して取り込みません。
- keyで引いた履歴は、譜面のhashが一致する場合だけ紐付けます。keyを返さないproviderでは、remote score IDだけで照合します。
- 各行の取り込みの失敗数を集計してから出力し、1件以上あれば通常の実行・dry-runとも非ゼロで終了します。通常の実行ではほかの有効な行を保存し、再実行しても重複して登録しません。dry-runでは保存しません。
- 途中のページの取得に失敗した場合も、それまでのページで保存した件数を出力してから非ゼロで終了します。
- 無効にしたproviderからは取得しません。認証情報のaccountが設定の`account_id`と異なる場合も取得しません。取り込み元の記録と、upload-localでの送り返しの除外を、同じaccountで照合するためです。

### BMZ メニューからの操作

「BMZ メニュー → IR」の「スコアの一括操作」と「IR送信キュー」は、CLIと共通の`ir::manual`（`upload_local_batch` / `upload_local_all` / `sync_jobs` / `download_scores` / `resend_job` / `resend_failed_jobs`）で処理します。CLIは進捗の通知を出力へ変換するだけです。

- 対象はprimary providerだけです。ローカルスコアの一括送信はBMZ IRだけ（BMS-IR / rianIRは非対応）、スコアの取得はrianIR以外が対象です。未ログインやアカウントが決まっていない間は、ボタンを無効にします。
- 「ローカルスコアを一括送信」は`ir upload-local --all`と同じ完走モードで、`include_replay`と`include_course_stages`はCLIの既定値（どちらもfalse）です。
- 確認画面を開くと、dry-runと同じ方法で候補の件数を数え（送信待ちの既存のjobも加えます）、`件数 × IR_CLI_SYNC_JOB_SPACING_MS`を最短時間の目安として表示します。
- 失敗したjobの扱いと打ち切りの条件は`--all`と同じです。leaseの待ちから再開しても同じ実行開始時刻を使い、その実行で失敗したjobは再送しません。失敗が残った完了・連続の失敗・認証エラー・送信制限・アカウントの送信停止 / BANは、それぞれ別のトーストで通知します。アカウントの制限では即座に止め、譜面単位の拒否は連続の失敗に数えずに次のjobへ進みます。
- 「IRからスコアを取得」は`ir download-scores`と同じで、CLIの既定の上限（200件）を付けず、全件を対象にします。取り込み済みのscoreは除くため、中断した後の再実行は続きから進みます。
- 新しいscoreを保存した場合は、完了・中断・エラーの通知時に、選曲画面の一覧、フォルダの集計、player statsを更新します。既存の履歴へのリンクだけで新しい保存がない場合は更新しません。取得結果には取り込み・紐付け・失敗の件数を表示し、一部の失敗は通常の完了と区別します。
- 送信は`IR_CLI_SYNC_BATCH_LIMIT`（100）件ずつ、job間は`IR_CLI_SYNC_JOB_SPACING_MS`（200ms）です。中断は、job間・batch間・送信間隔の待ちの間に確認し、処理中の1件は終えてから止まります。
- 書き込みを伴う操作は、同時に1つだけ実行します。要求ごとの世代番号で、古い結果・進捗・一覧の読み込みを破棄します。専用スレッドのcurrent-thread runtimeで動き、設定ウィンドウを閉じても続きます。完了・失敗・中断はトーストで通知します。
- 実行中は常駐workerを一時停止し、終了・失敗・中断のどれでも再開します。
- 他のプロセスや、停止前にclaim済みのjobが`sending`のまま進まない場合は、15秒間隔で再試行します。進まないまま6分（lease 5分 + 余裕）続くと、止めて報告します。待っている間も中断できます。失敗したjobがあった場合は、待たずに止めます。
- 実行中はプロファイルの切り替えを始められません（UIも無効にします）。アプリの終了時は中断を要求し、処理中の1件を最大3秒待ちます。待ち切れなかったjobは`sending`のまま残り、5分のleaseの後に再び取得します。
- 送信キューの画面は、`list_ir_score_jobs` / `ir_score_job_status_counts`で読み、状態ごとの件数、状態のフィルター（既定は失敗）、1ページ50件のページングを持ちます。並びは`failed` → `sending` → `pending` → `succeeded`、次に更新日時の新しい順です。DBの読み込みは別のスレッドで行い、UIスレッドでは開きません。一覧は操作の完了後に自動で読み込み直します。
- 再送（個別・失敗分の一括）は状態を直接書き換えず、通常の送信と同じclaimを経由し、再試行の待ち時刻（backoff）を無視して送ります。`Immediate`トランザクションの中で`sending`へ更新するため、常駐workerや他のプロセスが送信中のjobは二重に送らず、「送信中のため送らなかった」件数として数えます。失敗分の一括の再送は、開始時点の`failed`の一覧を対象に固定します。
- 決定画面・プレイ画面の表示中は、IR設定と同様に操作できません。

### 重複したスコアの整理

`bmz-player ir cleanup-imported [--apply]`は、取り込みで重複した旧`source_kind=Local`の履歴を整理します。

- Beatorajaの行と、譜面のhash、プレイ日時、EX、全判定の内訳、BP、CB、最大コンボ、random seedが一致する履歴を、dry-runで表示します。
- `--apply`は、選んだprovider / accountの明示的な`local_backfill` scoreだけをサーバーで削除し、影響する`best_scores`を残りの履歴から作り直します。ローカルの重複に対応する通常のプレイや、旧形式のIRのscoreは残します。
- 次に、ローカルのIRのjob / submissionの台帳を削除してから、`score.db`の履歴、`score_best`、`player_stats`を集計し直します。
- 別のIR accountへの送信済みの候補がある場合は、ローカルの削除を止めます。
- サーバーへの削除は1 requestあたり最大19 scoreで、batch間を200ms空けます。

`bmz-player ir cleanup-duplicate <HISTORY_ID> [--apply]`は、同じsource kindの中で、譜面のhash、プレイ日時、EX、全判定の内訳、BP、CB、最大コンボ、random seedが一致する履歴がちょうど1件だけあることを確かめてから、指定した履歴を削除します。
対応する`local_backfill`のIR score、networkのjob / submission、ローカルの集計も、同じ順序で整理します。

### スキンとプレイへの反映

- リザルト・選曲スキンでは、`NUMBER_IR_RANK(179)` / `NUMBER_IR_TOTALPLAYER(180/200)` / `NUMBER_IR_CLEARRATE(181)` / `OPTION_IR_LOADING/LOADED/NOPLAYER/FAILED(601..604)`を使えます。LR2 CSVの選曲・決定スキンでは、600..608を1つの状態として排他的に評価します（[スキン仕様](skin.md)の600..608を参照してください）。BMZ IRが非公開・ブロック中の譜面への送信を拒否したリザルトでは、ランキングの取得失敗をLR2の`IRstatus`のBAN曲（605）として渡します。beatoraja系のスキンには対応するpropertyが無いため、従来どおり取得失敗（604 / 608）で表示し、組み込みのリザルトは`IR RESTRICTED`と表示します。
- 選曲スキンでは、`STRING_RIVAL(1)` / `NUMBER_RIVAL_SCORE(271)` / `NUMBER_RIVAL_MAXCOMBO(275)` / `NUMBER_RIVAL_MISSCOUNT(276)` / `OPTION_(NOT_)COMPARE_RIVAL(624/625)`を使えます。
- リザルト画面には、eguiのオーバーレイも表示します。
- `TARGET: RIVAL`（`TargetOption::Rival`）は、選曲時に選んだIRライバルのベストEXを、プレイ中のゴーストとリザルトでの差分に使います。
- `ir rivals`を実行すると、`profile.rival.entries`（`source=Ir`）へ同期します。
- IRページを開く操作（既定F11）は、primary providerの`<base_url>/charts/<sha256>`、コースでは`<base_url>/courses/<course_hash>`を開きます。

### Webの表示

- ユーザーページからライバルを登録・解除できます。
- `/rivals`では、自分をライバルに登録しているプレイヤーの一覧と、score identityごとのEX / BPの比較を表示します。
- 譜面のランキングは、全体と「自分＋ライバル」を切り替えられます。

### BMS-IR

`https://www.bms-ir.org`を3番目の固定providerとして保持します。

- 既定では無効です。BMS-IRの数値IDとLR2のgame tokenを、秘密情報の保存先に保存します。
- ログインした後はprimary providerに選べ、既存のqueueから単曲と、最後のコース / 段位のscoreを送ります。
- ランキング、登録したライバルとbestの同期、BMS-IRの難易度表、サーバーからローカルへの自己スコアの同期を、providerごとのcacheで扱います。ライバルの追加・解除はWebで行います。
- BMS / PMSはMD5、BMSONはBMS-IR互換のSHA hashで照合します。
- 送信の対象は、Beatoraja / LR2orajaのルール、実際のLN / CN / HCN、Off / Flip / Battle / Battle ASの、4 / 5 / 6 / 7 / 8 / 9 / 10 / 14 / 24 / 48Kです。BattleとBattle ASは別の区分、G-BATTLEは通常の区分を使います。4 / 6 / 8KもBMS-IRの正規のキーモードとして送ります。
- DX、full autoplay、リプレイ再生、プラクティス、実効アシスト、リプレイファイルの送受信、local backfillは対象外です。
- クリアは`Failed` / `Easy` / `Normal` / `Hard` / `ExHard` / `FullCombo` / `Perfect` / `Max`と、`gauge`が`AssistEasy`の`LightAssistEasy`を送ります。後者はASSIST EASYゲージ（GASの最終ゲージを含む）のクリアで、実効アシストのプレイはqueueに入りません。実効Assistのランプである`AssistEasy`は送りません。ランプを`Easy`などへ置き換えて送ることもしません。
- コース中の単曲のスコアには`course_stage=true`を付け、保存時と同じくNoPlay / FullCombo / Perfect / Maxへ丸めたランプだけを送ります。数値のスコアと判定は単曲の結果のままです。最後のコース / 段位のscoreは別のjobとして送ります。
- ローカルでの結合確認用のビルドだけは、`cargo build`の時に`BMZ_BMS_IR_BASE_URL`を指定すると、固定のendpointを差し替えられます。通常のビルドでは本番のURLに固定で、プロファイルの設定からgame tokenの送信先を変えることはできません。

### random seed

- 新しいプレイでは、beatorajaと同じサイド別の24 bitのseedを使います。SPはP1、DPは`P1 + P2 * 2^24`を、`play_options.seed` / `random_seed`に10進数の文字列で送ります。
- `seed_scheme=beatoraja_24bit_v1`と、BMSの`#RANDOM`の選択列（`bms_random_choices` / `bms_switch_choices`）を、別のmetadataとして送ります。
- F-RANDOM / MF-RANDOMを含むオプションを送るかどうか、外部のIDへどう対応させるかは、共通の層では拒否せず、将来のprovider側の対応状況とユーザーの設定で決めます。

### ローカルでの動作確認

```bash
bun install
bun run db:migrate
bun run build
bun run cf:types
bun run dev                                   # http://localhost:3000
bmz-player ir login --email <EMAIL> --base-url http://localhost:3000
bmz-player ir status                          # key: bmz-dev / connection: OK を確認する
# 1曲プレイすると、リザルトで送信とリプレイの自動アップロードを行う
bmz-player ir ranking <SHA256> --ln-policy ForceLn
```

## 開発環境と実装の入口

Node.js / Bunを用意し、リポジトリrootから操作します。
コマンドの定義は[package.json](../package.json)、ディレクトリ対応とruntime設定は
[nuxt.config.ts](../nuxt.config.ts)を確認します。

- Node.jsはNuxt 4.6の要件（`^22.22.3`、`^24.15.0`、`>=26.0.0`のいずれか）を満たす版を使います。
  `bun run` / `bunx` は `nuxt`・`wrangler`・`vue-tsc` などをPATH上の `node` で起動し、`bun test` はBunで実行します。
- `typescript` は ^6 を明示しています。`vue-tsc` とtypescript-eslintがTypeScript 7にまだ対応していないためです。
- 依存を更新した後は、`node_modules` を消して `bun install --frozen-lockfile` で入れ直します。
  差分インストールでは、lockにない古い入れ子のパッケージが残り、Workerのbundleに入ることがあります。

### アプリ構成

| 場所 | 責務 |
|---|---|
| [bmz-ir-web/app](../bmz-ir-web/app/) | Nuxt UIです。pages、components、layouts、composables等 |
| [bmz-ir-web/server](../bmz-ir-web/server/) | Nitro API、middleware、services、repositories等 |
| [bmz-ir-web/shared](../bmz-ir-web/shared/) | app/server共通の型・schema・定数・純粋関数です。secretやDB queryは置かない |
| [bmz-ir-web/public](../bmz-ir-web/public/) | root URLから配信する静的ファイル |
| [Rust IR client](../crates/bmz-player/src/ir/) | client、credentials、device_key、sync、secret_store |
| [Rust IR CLI](../crates/bmz-player/src/ir_cmd.rs) | `bmz-player ir`の各サブコマンド |

Result / Selectの連携は`crates/bmz-player/src/screens/result_ir.rs` / `select_ir.rs`、
skin IDの解決は`crates/bmz-render/src/skin/`を確認します。
秘密情報の保存先はprofileの`[ir] credential_store = "File" | "Os"`で選びます。
APIの一覧は[API一覧](#api一覧)、ローカルでの操作例は[ローカルでの動作確認](#ローカルでの動作確認)を参照してください。

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
