# BMZ IRの設計記録

`docs/ir.md`にあった、BMZ公式IRの設計案・当初の方針・実装タスク案・未決事項を移した記録です。
設計は2026-06-04に作成し、2026-07-13に実装状況との差分を追記しています。
2026-10-18に`docs/ir.md`を現在の仕様だけにまとめた際、この記録へ移しました。

この記録の内容は当時の案であり、現在の実装と異なる箇所があります。
現在の仕様は[IR](../../docs/ir.md)、rianIR互換は[rianIR互換プロバイダー](../../docs/rian-ir.md)を参照してください。

主な相違は次のとおりです（2026-10-18にコードと照合）。

- 認証はOAuth/OIDC device flowではなく、email + passwordのログインとbearer tokenです。
- Server DBはPostgreSQLではなく、NuxtHub DB（SQLite / D1）とDrizzle ORMです。schemaは`bmz-ir-web/server/db/schema.ts`が正です。
- `best_scores`は、EX・クリア・最大コンボ・BP・CBの項目ごとに最良値とその元のscoreを保持します。
- `PUT /api/v1/charts/{sha256}`は実装していません。譜面はscore submit内で登録します。
- Ranking APIの`rule_mode`は省略でき、`rival_only`の別名は実装していません。
- keyringの保存先は`bmz.ir.<provider_key>` / `bmz.ir-device-key.<provider_key>`のserviceと、プロファイルディレクトリの絶対パスのuserです。
- 未決事項のうち、`rankings`のwrapper形式（`succeeded` / `data` / `error`）、`scope_rank`（常に返す）、replay upload API、canonical JSON（キー昇順のcompact JSON + Ed25519）は実装で決まりました。

## 設計からの主な差分（2026-07-13時点）

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

## 前提と設計ゴール

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

## 全体アーキテクチャ

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

## 認証とtoken保存の当初案

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

## Provider traitの案

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

## リザルト画面の状態管理の案

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

## Chart APIとchart identityの案

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

## Server DBの設計案（PostgreSQL想定）

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

## Ranking SQLの案

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

## Rival tableの案

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

## Client local DBの設計案

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

## Client submit flowとtrait型の案

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

## Retryと通信設定の案

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

## Course Score APIのserver DB設計（2026-06-11）

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

## URL Resolverの案

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

## API一覧と初期実装の優先度

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

## Nuxt + NuxtHub / Drizzleの実装方針

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

## Provider Adapterの方針

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

## 実装タスク案

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

## 未決事項

レビューで判断が必要な点です。

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

## 最終結論（設計時点）

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
