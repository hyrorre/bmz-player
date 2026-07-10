# BMZ IR (Internet Ranking) API 設計メモ

Codex に最終レビューと実装を引き継ぐための設計まとめ。
本ドキュメントでは、BMZ 新BMSプレイヤー向けの IR API、クライアント側 Provider trait、認証、ランキング取得、リザルト画面連携、replay 拡張、NuxtHub / Drizzle 実装方針を整理する。

---

## 実装状況 (2026-06-18)

以下は本メモを元に実装済みの範囲と、設計からの主な差分。
ソースの所在: クライアントは `crates/bmz-player/src/ir/`、サーバーは
`bmz-ir-web/server/`、DB schema は `bmz-ir-web/server/db/schema.ts`、
DB migration は `server/db/migrations/sqlite/`。

### 実装済み API

```http
POST   /api/v1/auth/login                      # email+password → provider_key + access/refresh token
POST   /api/v1/auth/register
POST   /api/v1/auth/logout
POST   /api/v1/auth/refresh                    # refresh token rotation + provider_key
GET    /api/v1/me
POST   /api/v1/scores                          # include=rankings&ranking_scopes=... 対応
GET    /api/v1/scores/{id}                     # スコア詳細 (公開)
GET    /api/v1/charts                          # 譜面一覧 (?q= タイトル検索)
GET    /api/v1/charts/{sha256}                 # 詳細 + play_count/clear_count 集計
GET    /api/v1/charts/{sha256}/ranking         # scope/ln_policy/rule_mode/limit/offset
GET    /api/v1/rivals
POST   /api/v1/rivals                          # { target_player_id, action: add|remove }
GET    /api/v1/players/{id}                    # プロフィール + best scores
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

未実装: `PUT /api/v1/charts/{sha256}` (chart upsert は score submit 内で実施)、
`DELETE /api/v1/rivals/{player_id}` (POST の action=remove で代替)、tables 系。

### クライアント (bmz-player)

- CLI: `bmz ir login|logout|status|ranking|sync|upload-local|rivals|device-key|replay`。
  egui プロファイル設定からもログイン可能。
- 送信: リザルト確定時に `ir_score_jobs` へ enqueue (send_policy 判定込み) →
  リザルト画面で即時送信 + アプリ常駐ワーカー (30 秒間隔) が残りを処理。
  送信は 1 バッチ 20 件、job 間 3.1 秒待ちで、backfill 時も同じペースに揃える。
  バッチ取得時に対象 job をまとめて `sending` として claim し、リザルト送信と
  常駐ワーカーが並行しても同じ job を二重送信しない。
  CLI の `ir sync` / `ir upload-local --sync` は中断時に未処理jobを一括で
  `sending` に残さないよう1件ずつclaimし、`[N/20]` の進捗を逐次表示する。
  Ctrl+C時に残る `sending` は処理中の最大1件で、5分後に再取得される。
  リトライは 1分 → 5分 → 30分 → 2時間 → 以降24時間。
  429 が `Retry-After` を返した場合は通常のバックオフより優先してその時刻まで待つ。
  score 成功履歴の保存と `kind=replay` job の投入は同じ transaction で確定する。
  replay upload / verify の失敗は replay job だけを再試行し、成功済み score を
  未送信状態へ戻さない。
  成功済み job は payload を空にし、同期後に 30 日以内または最新 500 件だけを
  `network.db` に保持する。未送信 / 失敗 / 送信中 job は剪定対象外。
- local backfill: `bmz ir upload-local [--dry-run] [--limit N] [--sync] [--all]` で
  `score.db` の既存 `score_history` を `local_backfill` source の score submit として enqueue
  する。既送信履歴と既に queue にある履歴は既定でスキップし、course stage /
  autoplay は既定で除外。local backfill は未登録 chart を作成できるが、既存 chart
  の正規メタデータは上書きしない。`--sync` で既存の未完了score jobがある場合は、
  queueを増やさず先に最大20件を同期する。
  `--all` は `--sync` を含む完走モードで、既存queueの排出と次の投入batchを候補が
  なくなるまで繰り返す。送信失敗または他プロセスの `sending` job で進捗できない場合は
  非ゼロで終了し、retry/backoff または5分のlease後に再実行する。
  送信直前にdevice keyで署名し、サーバー側では `signed_backfill` として保存する。
  既に送信済みで署名のないscoreは `bmz ir attest-submitted --all` で、ローカルの
  成功履歴に残るremote score IDへ後付けattestationを送る。attestationはscore本文を
  再送せず、所有者のdevice keyでscore IDを署名してverificationだけを更新する。
  成功履歴の保持期間外でremote score IDを失ったscoreは対象外で、将来のserver側一括
  attestation APIが必要になる。
  per-history ghost は現在の `score_history` には保持していないため送らない。
- rate limit: score submit / course score は 15 分あたり user 1500 / IP 3000。
  replay upload 系は 1 replay あたり upload-url / upload / verify の 3 request を使うため、
  15 分あたり user 900 / IP 1800。429 では `Retry-After` を返す。
- ランキング表示: Result / Select スキンの `NUMBER_IR_RANK(179)` /
  `NUMBER_IR_TOTALPLAYER(180/200)` / `NUMBER_IR_CLEARRATE(181)` /
  `OPTION_IR_LOADING/LOADED/NOPLAYER/FAILED(601..604)`、
  Select の `STRING_RIVAL(1)` / `NUMBER_RIVAL_SCORE(271)` /
  `NUMBER_RIVAL_MAXCOMBO(275)` / `NUMBER_RIVAL_MISSCOUNT(276)` /
  `OPTION_(NOT_)COMPARE_RIVAL(624/625)`。egui のリザルトオーバーレイもあり。
- ターゲット: `TARGET: RIVAL` (TargetOption::Rival) が選曲時の IR ライバル
  ベスト EX をプレイ中ゴースト / リザルト差分に使う。
- ライバル: `ir rivals` 実行時に `profile.rival.entries` (source=Ir) へ同期。
- リプレイ: 送信 payload に hash 申告 → 送信成功後に自動アップロード + 検証。
  `bmz ir replay <SCORE_ID>` でダウンロードし
  `bmz --boot-replay-file <PATH>` で再生。
- provider key: BMZ クライアントは `/api/v1/auth/login` /
  `/api/v1/auth/refresh` が返す `provider_key` を credentials / device key /
  `ir_score_jobs.provider` / `primary_provider` の識別子として使う。
  `IrProviderConfig.provider` は表示名または実装種別として残す。

### 設計からの主な差分

1. **認証**: OAuth/OIDC device flow ではなくローカル email+password
   (`/api/v1/auth/login`, `nuxt-auth-utils` session + bearer token) を採用。
   device flow は将来課題。
2. **秘密情報の保存先**: `profile.toml` の `[ir] credential_store = "File" | "Os"`。
   既定は File (プロファイル配下 0600 JSON)。開発時の Keychain 許可ダイアログを
   避けるためで、`"Os"` にすると keyring 経由で OS credential store に保存し、
   既存ファイルは初回アクセスで自動移行する。
   Linux ビルドは Secret Service 用に libdbus が必要。
3. **provider 設定**: `IrProviderConfig` に `base_url` と `provider_key` を追加。
   `provider_key` はクライアント側で URL から推測せず、BMZ IR サーバーの
   auth response から取得する。ローカル IR は `bmz-dev`、production IR は
   `bmz` を返す。サーバー側は `NUXT_IR_PROVIDER_KEY` で明示上書き可能。
4. **verification**: DB は `unverified / signed_backfill / verified_play` の 3 値。
   `replay_uploaded / verified` はスコアの verification ではなく
   `replay_objects.status` (`metadata_only / pending_upload / uploaded /
   verified / rejected`) で管理する。不正な署名はscore submitを400で拒否する。
5. **tamper evidence**: canonical form は「top-level `evidence` を除いた
   payload を RFC 8785 JSON Canonicalization Scheme (JCS) で compact JSON 化」
   したもの。署名は Ed25519(secret, SHA256(canonical))。
   JSON number は ECMAScript `JSON.stringify()` 相当の表現に正規化するため、
   `160.0` と `160` は同じ canonical bytes になる。
6. **ranking response**: `pagination.total` (scope 内総数) と
   `ranking.clear_rate` (%) を追加。`NUMBER_IR_PREVRANK(182)` は未対応 (None)。
7. **played_at**: クライアントは unix 秒で送る。サーバーが ISO へ正規化して
   timestamptz に保存する。
8. **gauge 表記**: 保存値・返却値は BMZ の `GaugeType::as_str()` と同じ
   `AssistEasy` / `Easy` / `Normal` / `Hard` / `ExHard` / `Hazard` /
   `Class` / `ExClass` / `ExHardClass`。API は lowercase や legacy alias も
   受けるが、DB と response では canonical 値へ正規化する。
9. **idempotency / retry**: score / course score の重複投稿は既存行を返す。
   course score の重複時は best を再計算せず `best_updated=false`。
   ローカル job queue は `sending` のまま 5 分以上止まった job も再送対象にする。
   refresh token は rotation 方式で、BMZ クライアント側はプロセス内 mutex で
   refresh を直列化する。サーバー側は直近の `rotated` token 再利用を並行 retry
   とみなし、新 token を巻き込んだ全 session revoke はしない。
10. **course score**: 実装済み (本節の API 一覧と §19 の DDL 参照)。
   コース終了時に `ir_score_jobs` (kind=course) へ enqueue し、
   evidence schema は `bmz-course-score-evidence-v1`。
   `around_self` は自分の前後 5 件ずつのウィンドウを返す
   (未ログイン / 自己スコアなしのときは global と同じ)。
   **tables の専用実装**は未着手。

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

- BMZ は新規 BMS プレイヤー。
- IR は beatoraja IR API 互換そのものではなく、BMZ 内部モデルを中心に設計する。
- beatoraja の `IRConnection` 的な公開境界は参考にする。
- 将来 Mocha / MinIR 互換 Provider を追加する可能性がある。
- BMZ 公式 IR サーバーも作成中だが、現時点では動作確認・reference implementation 寄り。
- 公式 IR サーバーは Nuxt + NuxtHub DB / Drizzle ORM で実装中。
- Cloudflare deploy は NuxtHub が生成する wrangler config を使い、D1 binding は
  `DB`、R2 replay blob binding は `BLOB` とする。
- リプレイ blob は `hub:blob` 経由で保存し、ローカルは `.data/blob`、Cloudflare
  build は `NUXT_HUB_BLOB_BUCKET` の R2 bucket を使う。
- 長期的に BMZ 公式 IR をサポートし続けるかは未確定。
- BMZ 本体は公式 IR API に密結合しない。
- 譜面 hash は SHA256 を主キーにする。
- MD5 は beatoraja 系互換・外部IR連携用に保持する。
- ランキングの主軸は EX score 固定。
- LN / CN / HCN は `docs/ln.md` の `LnScorePolicy` に従って区別する。
- IR の score identity は chart SHA256 単独ではなく、`chart_sha256` / `ln_policy` / `double_option` / `rule_mode` / `scoring` を含む。
  `gauge` は投稿時のプレイ設定・表示 metadata として保存するが、ランキング分離キーにはしない。
- replay は将来アップロードする可能性があるため、初期設計から hash / format / status を持たせる。

### 設計ゴール

- BMZ 内部モデルを中心にして、Provider Adapter で外部IR差分を吸収する。
- スコア送信とランキング取得がセットになりやすい BMS プレイヤーの UX に合わせる。
- リザルト画面で全体ランキング / ライバルランキングを切り替え可能にする。
- スコア送信時にランキングを同時取得するかどうかを設定可能にする。
- ランキング取得失敗でスコア送信成功まで失敗扱いにしない。
- replay upload / verification に将来拡張できる。
- tamper evidence として canonical hash / client signature を入れる。
- 完全なチート防止ではなく、「提出内容があとから改変されていない」ことを示す仕組みを作る。

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

- BMZ 内部では独自の `IrProvider` trait を定義する。
- 公式IRも外部IRも trait 実装として扱う。
- スコア送信は有効な全 IR に送る。
- ランキング取得、ライバル取得、表取得、URL open は Primary IR のみで行う。
- Secondary IR は原則スコア送信用。

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

- ブラウザ認証 + token 保存。
- デスクトップアプリでは OAuth/OIDC の device authorization flow または loopback redirect を使う。
- BMZ の config には秘密情報を置かない。
- refresh token は OS credential store に保存する。

### OS credential store

候補:

- macOS: Keychain
- Windows: Credential Manager
- Linux: Secret Service / libsecret
- Rust: `keyring` crate 系を候補にする

### Config に置くもの

IR 関連設定はユーザー / profile 単位の状態なので、BMZ 本体の global
`config.toml` ではなく `data/profiles/{profile}/profile.toml` に保存する。

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
provider = "bmz"
provider_key = "bmz-dev"
base_url = "http://localhost:3000"
enabled = true
account_display_name = "ExampleUser"
send_policy = "complete_song"
role = "submit_only"
```

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

Login flow は OS / UI / ブラウザ起動が絡むため、Provider に閉じ込めすぎない。

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

beatoraja の三択を踏襲する。

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
  リザルト確定時に常に送る

CompleteSong:
  最終ゲージが 0 より大きい場合だけ送る

UpdateScore:
  EX score、clear、combo、minbp のいずれかが改善した場合だけ送る
```

### 注意

- 送信ポリシーはクライアント側 UX の制御。
- サーバー側はサーバー側で best 更新判定を行う。
- クライアントが送ってきたからといって必ず best として採用しない。

---

## 5. リザルト画面用ランキング取得設定

本体設定にチェックボックスを2つ用意する。

```txt
[ ] スコア送信時に全体ランキングを取得する
[ ] スコア送信時にライバルランキングを取得する
```

### 内部設定名

ユーザー向け文言は上記でよいが、内部名は `prefetch` として扱う。

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
| ON | OFF | global だけ取得 | self_and_rivals は切替時に取得 |
| OFF | ON | self_and_rivals だけ取得 | global は切替時に取得 |
| ON | ON | global と self_and_rivals を取得 | 両方即切替可能 |

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
| `include` | string | `rankings` を指定するとランキングを同時取得する |
| `ranking_scopes` | comma-separated string | `global`, `self_and_rivals` など |
| `ranking_limit` | integer | 各ランキングの最大件数。初期値は 100 など |

### Request payload

`play_count` / `clear_count` は Score Submission API では送らない。
これらはクライアントが自己申告する値ではなく、IR Server が `scores` 投稿履歴から集計する値として扱う。
Local BMZ がローカル表示用に `play_count` / `clear_count` を保持していても、IR 送信 payload には含めない。

`device_type` は BMS IR の慣習に合わせ、スコアを記録した主入力デバイスとして必ず送る。
値は `keyboard` / `controller` の2種類だけにし、`mixed` は作らない。
BMZ はプレイ中の human press input を集計し、controller 入力数が keyboard 入力数より多ければ `controller`、
それ以外は `keyboard` と判定する。
controller のスクラッチだけ keyboard 入力へ変換している環境でも、鍵盤側が controller なら `controller` になる。
`device_type` はランキング分離キーではなく、表示・検索・検証補助用の score metadata として扱う。

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
    "seed": 123456789,
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

BMZ 本体は `docs/ln.md` の `LnScorePolicy` を local score DB の保存キーに使う。
IR でも同じ値を score identity の一部として扱う。

送信 payload の `rule.ln_policy` は BMZ が profile policy と chart LN profile から正規化した結果で、保存値は次のいずれか。

```txt
AutoLn
AutoCn
AutoHcn
ForceLn
ForceCn
ForceHcn
```

`rule.effective_ln_mode` は実際に降らせた LN 種別の補助情報で、初期値は `ln` / `cn` / `hcn` のいずれかにする。
ランキング、best score、play_count / clear_count の集計キーは `effective_ln_mode` ではなく `ln_policy` を使う。

`chart.ln_profile` はサーバーが譜面 metadata を補完・検証するための参考情報であり、best score の分離単位にはしない。
ただし chart registry には保存してよい。

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

`verification` と replay hash は server DB に保存される。リプレイの upload URL
は score submit response には含めず、
`POST /api/v1/scores/{id}/replay/upload-url` で別途取得する。

同じ player / `idempotency_key` の重複投稿は既存の `score_id` を返す。
履歴の二重 insert は行わないが、現在の実装では best 更新判定は通常の投稿と
同じ経路を通る。

### Response: ランキングあり

`rankings` は request された scope だけ含める。

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

スコア送信自体は成功扱いにする。

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

ランキング同時取得時は、必ず best 更新後にランキングを取得する。

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
| `offset` | integer | pagination 用 |
| `ln_policy` | string | 任意。指定した場合だけ `AutoLn` / `AutoCn` / `AutoHcn` / `ForceLn` / `ForceCn` / `ForceHcn` で絞り込む |
| `rule_mode` | string | 必須。`Beatoraja` / `Lr2Oraja` / `Dx` |
| `scoring` | string | 初期は `bms_ex_score_v1` |

### Ranking scope

| scope | 内容 |
|---|---|
| `global` | 全体ランキング |
| `self_and_rivals` | 自分 + ライバル |
| `rivals` | ライバルのみ。自分は含めない |
| `self` | 自分だけ |
| `around_self` | 全体ランキング上で自分の周辺 |

ユーザーが希望している「ライバルと自分のみ」は `self_and_rivals` を使う。

### `rival_only` alias

正式には `scope` を推奨する。
互換・分かりやすさ用に alias を受けてもよい。

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
          "seed": 123456789,
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
          "seed": 987654321,
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

- `rank` は常に全体ランキング上の順位。
- `scope_rank` は scope 内での表示順位。
- `self_and_rivals` でも `rank` は全体順位を返す。

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

Ranking API は必要なら entry ごとに `stats.play_count` / `stats.clear_count` を返す。
これらは投稿 payload 由来ではなく、IR Server が `scores` 投稿履歴から集計する。

```txt
play_count  = 対象 player + chart + rule の accepted score 投稿数
clear_count = そのうち clear_type が Failed / NoPlay 以外の投稿数
```

集計単位は ranking query と同じ `chart_sha256` / `ln_policy` / `double_option` / `rule_mode` / `scoring` を基本にする。
`play_count` / `clear_count` が不要な画面では response から省略してよい。

### Device type

Ranking API / Chart detail API は score 表示に `device_type` を含める。
`device_type` は `keyboard` / `controller` の2値で、`mixed` は返さない。

集計・順位の key には含めない。
同じ player / chart / ln_policy / double_option / rule_mode / scoring の best score が更新された場合、`best_scores.device_type` は
その best score を記録した投稿の値で上書きする。

### EX score 同点順位

BMS IR らしさを考えると、順位は EX score のみで同点同順位にする。

```sql
RANK() OVER (
    ORDER BY bs.ex_score DESC
) AS rank
```

表示順の安定化には追加条件を使う。

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

`PUT` にしておくと、スコア送信前に chart metadata を upsert できる。

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

`GET /api/v1/charts/{sha256}` は必要なら chart 全体、またはログイン中 player の
`play_count` / `clear_count` を返してよい。これも Score Submission payload の値ではなく、
IR Server が `scores` 投稿履歴から集計する。

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

Chart detail に stats が不要な画面では省略してよい。

---

## 9. Replay 設計

### 初期方針

- replay 本体アップロードは score submit とは分離し、送信成功後に専用 API で
  upload URL を取得して実行する。
- Score submission payload には replay hash / format / upload_intent を入れる。
- サーバーは replay hash と storage object の hash を照合し、
  `replay_objects.status` で状態を管理する。

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
| `none` | replay なし |
| `later` | hash だけ送って、あとでアップロード予定 |
| `now` | score submit 後に続けてアップロード予定 |

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

- 完全なチート防止ではない。
- 改造クライアントなら偽スコアに署名できる。
- 目的は「提出内容があとから改変されていない」ことを示すこと。
- 将来 replay upload / replay verification と組み合わせる。

### 署名対象

Canonical JSON または MessagePack などで正規化する。
JSON の素朴な文字列化は避ける。

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

`played_at` はクライアント時計由来なので、署名対象に入れる場合でも信用時刻として扱わない。
ランキングや監査の基準時刻には、サーバー側で付与する `server_received_at` / `created_at` を別に保存する。
クライアント時計が大きくずれている場合は、表示用の参考値として保持しつつ warning / suspicious flag を付けられるようにする。

含めないもの:

```txt
access token
refresh token
サーバー側で付与するID
サーバー受信時刻
ランキング順位
```

### Device key

初回スコア送信時、または `bmz ir device-key` / egui の署名鍵操作時に
provider key ごとの device key を lazy 生成する。

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

リプレイの検証状態は `replay_objects.status` として別管理する。

---

## 11. EX Score / Judge model

### EX Score

beatoraja メモに合わせる。

```txt
(epg + lpg) * 2 + egr + lgr
```

BMZ 内部関数案:

```rust
pub fn ex_score(j: &JudgeCounts) -> u32 {
    (j.fast_pgreat + j.slow_pgreat) * 2
        + j.fast_great
        + j.slow_great
}
```

### Judge counts

IR payload は現行 BMZ の `ScoreState::JudgeCounts` に合わせる。
BMZ では `Miss` という判定名は使わず、見逃しは `Poor`、空押しは `EmptyPoor` として扱う。
`EmptyPoor` にも FAST/SLOW があるため、IR でも丸めずに保存する。

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

将来 FAST/SLOW に分類できない独立イベントが必要になった場合は、v1 payload へ `neutral` を足すのではなく、
用途を明確にした別 field として追加する。

### BP / CB

BMZ 本体では `miss_count` 名を廃止し、BMS 文脈の BP / CB をそのまま保存・表示名に使う。

```txt
bp = bad + poor + empty_poor
cb = bad + poor
```

`empty_poor` は combo を切らないため、combo break 集計の `cb` からは除外する。
IR v1 では最小値として次を扱う。

```txt
min_bp = best play の最小 bp
min_cb = best play の最小 cb
```

BMZ の local score DB は `score_history` / `score_best` / `replay_slots` に `bp` / `cb` を直接保存する。
本体側の対応方針:

- `ScoreState::bp()` は `bad + poor + empty_poor`、`ScoreState::cb()` は `bad + poor` を返す。
- `score_history` / `score_best` / `replay_slots` に `bp` / `cb` を保存し、表示・リプレイ slot metrics でも `miss_count` 名を使わない。
- local best 更新条件は `ex_score`、`clear_type`、`bp`、`cb`、`max_combo` の順に比較する。
- IR 側の `min_bp` / `min_cb` は、送信された `bp` / `cb` の自己ベスト最小値として扱う。

### Device type

BMZ の local score DB は `score_history` / `score_best` に `device_type` を保存する。
`replay_slots` は score history の snapshot 表示に必要なら `device_type` を持ってよいが、IR payload の元データは
`score_history.device_type` を正とする。

判定ルール:

```txt
human controller press count > human keyboard press count => controller
otherwise                                             => keyboard
```

- `mixed` は保存しない。
- `InputKind::Press` だけを数え、release による二重カウントを避ける。
- `InputSource::Human` だけを数え、autoplay / replay 再生入力はローカル実プレイの device 判定に含めない。
- 旧 replay や device 情報のない入力は `keyboard` fallback でよい。

---

## 12. Clear Lamp

サーバー・クライアントで順序を固定する。IR v1 は BMZ の `ClearType::as_str()` に準拠する。
文字列表現は local DB と同じ PascalCase を送る。

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

API の human-friendly alias として `hard_clear` などを受ける場合でも、保存時・response 時は
`ClearType::as_str()` の値へ正規化する。
BMS系ではアシスト、EASY、HARD、EXHARD、FULLCOMBO、MAX の扱いがズレやすいので、早めに固定する。

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

投稿履歴をすべて保存する。`play_count` / `clear_count` は submission payload から受け取らず、
この `scores` 履歴から IR Server 側で集計する。
`device_type` は投稿時点の主入力デバイスとして `scores.device_type` に保存する。

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

`clear_rank > Failed` は `NoPlay` / `Failed` を除外する意味。
Chart detail の global stats のように player 単位でない集計が必要な場合は、`player_id` 条件を外す。
負荷が問題になるまでは `scores` から都度集計でよく、必要になったら materialized view や summary table を検討する。

`device_type` は集計条件に含めない。
controller / keyboard 別ランキングを作る場合は v1 の score identity とは別の filter / view として追加する。

### best_scores

ランキング用に正規化する。

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

`scores` 側にも同じ `device_type text NOT NULL` を持たせる。
`best_scores.device_type` は best 更新に採用された `scores.device_type` をコピーする。

`effective_ln_mode` は表示・互換・将来の検証用に保存するが、best の unique key には使わない。
BMZ local DB と同じく、score 分離の正規化キーは `ln_policy` とする。

### scores と best_scores を分ける

```txt
scores:
  投稿履歴全部

best_scores:
  chart_id + player_id + rule ごとの現在ベスト
```

### best 更新条件

EX score ranking 固定のため、基本は EX score を主軸にする。

```txt
1. EX score が高い
2. 同点なら clear が高い
3. 同点なら minbp が低い
4. 同点なら mincb が低い
5. 同点なら max combo が高い
6. 同点なら played_at が古い/新しい、どちらか仕様で固定
```

ただし beatoraja 的な `IR_SEND_UPDATE_SCORE` は次のいずれかが改善したら送信する。

```txt
EX score
clear
combo
minbp
mincb
```

そのため、将来は項目別 best を分けてもよい。

```txt
best_score_by_ex_score
best_clear
best_combo
best_minbp
best_mincb
```

初期は `best_scores` にランキング用 best を保存するだけでよい。
BMZ 本体の local DB は `bp` / `cb` を保存するため、IR payload でも `min_bp` / `min_cb` を v1 から必須項目として扱う。

---

## 14. Ranking SQL 案

### Global ranking

順位は EX score のみで同点同順位。

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

全体順位を維持したまま表示対象だけ絞る。

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

`self_and_rivals` から `UNION SELECT $4` を抜く。

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
```

Response:

```json
{
  "rivals": [
    {
      "id": "pl_01H...",
      "display_name": "RivalPlayer",
      "relationship": "rival"
    }
  ]
}
```

---

## 16. Client local DB 設計案

### ir_accounts

現行実装では `provider` は IR サーバーが返す `provider_key` を保存する。
同じ provider 実装名でも、production `bmz` と local `bmz-dev` は別 provider
として扱う。

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

`provider` は送信先の `provider_key`。pending job はこの値で
`IrProviderConfig.provider_key` を引き、該当する `base_url` へ送信する。

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
    UNIQUE(provider, account_id, kind, local_score_id)
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

`pending` / `failed` は `next_attempt_at <= now` のとき送信対象。
`sending` は通常はワーカーが処理中の印だが、プロセス終了などで残る可能性が
あるため、`updated_at` から 300 秒以上経過したものは再送対象にする。

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

### Local score device type

IR 実装前準備として、BMZ 本体の通常スコア保存にも `device_type` を持たせる。

```sql
ALTER TABLE score_history ADD COLUMN device_type TEXT NOT NULL DEFAULT 'keyboard';
ALTER TABLE score_best ADD COLUMN device_type TEXT NOT NULL DEFAULT 'keyboard';
```

`device_type` は `keyboard` / `controller` の2値に固定する。
local best が更新された場合、`score_best.device_type` は更新元の `score_history.device_type` で上書きする。
IR job 作成時は `score_history.device_type` から payload の `play_options.device_type` を生成する。

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

- スコア送信は job queue 化する。
- オフライン時や一時失敗時は retry する。
- `idempotency_key` を使い二重投稿を防ぐ。

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

初期は自動送信 + retry でよい。

---

## 19. Course Score API

コーススコア投稿・ランキングは実装済み。

### Server DB 設計 (2026-06-11)

`scores` / `best_scores` の規約 (clear_rank、verification、idempotency、
served timestamps) をコースにもそのまま流用する。BMZ local の
metadata は `library.db` の `courses`、プレイ結果は profile-local
`score.db` の `course_scores` / `course_score_charts` と対応させる。

#### Course identity

- `course_hash` を主キー相当の identity にする。
  ローカル `course_key` ではなくサーバー間で再現可能な値として、
  「譜面 sha256 の順序付きリスト + constraint 群」を canonical JSON 化した
  ものの SHA256 とする (tamper evidence と同じ正規化規則)。

```txt
course_hash = SHA256(canonical_json({
  "charts": ["sha256-1", "sha256-2", ...],   # プレイ順
  "constraints": {
    "class": ..., "speed": ..., "judge": ..., "gauge": ..., "ln": ...
  }
}))
```

- タイトルや出典 URL は identity に含めない (表示 metadata)。
- score identity (best / ranking の分離キー) は
  `course_hash + gauge + ln_policy_setting + rule_mode + scoring`。
  - `gauge`: 段位 (class 系 constraint) では constraint で固定されるが、
    通常コースではユーザー選択なので key に含める。
  - `ln_policy_setting`: コースは譜面ごとに LN 解決が変わるため、
    解決後の policy ではなく設定値 (`AutoLn` / `ForceCn` など) を使う。
  - `rule_mode`: 単曲スコアと同じく `Beatoraja` / `Lr2Oraja` / `Dx` を
    別ランキングとして扱う。

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

submit 時に `scores` の chart upsert と同様に upsert する。
個々の譜面は `charts` registry に既存の仕組みで upsert 済みである前提
(コース送信時に未登録 chart があれば metadata なしで sha256 だけ登録)。

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
  ln_policy text not null,                      -- LnPolicySetting 値
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

`entries` を jsonb にするのはランキング集計に内訳を使わないため。
譜面単位の分析が必要になったら正規化テーブルへ移す。

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

best 更新条件は単曲と同じ
`ex_score > clear_rank > bp(小) > max_combo` の順。
段位らしさを優先するなら `clear_rank` (合格段位) を第一キーにする案も
あるが、IR 全体の「EX score 主軸」を崩さない方を既定にする。
不正な署名は保存せず拒否する。
同じ player / `idempotency_key` の重複投稿は既存の `course_score_id` を返し、
best は再計算せず `best_updated=false` とする。

#### API

- `scores` / `best_scores` と同じ方針:
  select は公開、書き込みは server route の認証済み API のみ。
- API は本節冒頭の `POST /api/v1/course-scores` に加えて:

```http
GET /api/v1/courses/{course_hash}                # registry + 集計
GET /api/v1/courses/{course_hash}/ranking        # scope/gauge/ln_policy/rule_mode
```

- evidence は `bmz-course-score-evidence-v1` schema で、単曲と同じ
  Ed25519 device key / canonical JSON を使う。

#### クライアント側の対応 (実装済み)

- `CourseResultSummary` (course_session.rs) から payload を構築。
  `course_hash` は course 定義 (charts + constraints) から計算し、
  ローカル `courses.course_key` とは独立に持つ。
- `ir_score_jobs.kind = 'course'` で単曲 job と同じ queue を使う。
  `local_score_id` は local course score id、`chart_sha256` は course hash を
  入れる。

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

beatoraja の `getSongURL`, `getCourseURL`, `getPlayerURL` 相当。

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

この節は初期計画の履歴。現在の実装済み API は冒頭の「実装状況」を正とする。

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

- `nuxt-auth-utils` cookie session と BMZ クライアント向け bearer access token を検証する。
- `users` と `profiles` を紐付ける。
- BMZ クライアントは email/password login で access/refresh token を取得する。

### NuxtHub DB

- Schema は `bmz-ir-web/server/db/schema.ts` の Drizzle schema を正とする。
- Migration は NuxtHub CLI / drizzle-kit の既定に合わせて
  `server/db/migrations/sqlite/` に生成する。
- server route は `hub:db` の Drizzle client 経由で DB を読む。

### Transaction

Score submission では次を transaction 的に扱う。

```txt
score insert
best_scores upsert
ranking select
```

Nuxt server route だけでつらくなったら RPC 化する。

---

## 23. Provider Adapter 方針

BMZ 内部では情報をリッチに持つ。

```txt
BMZ Score
  - avg_judge_ms
  - replay_hash
  - device_type
  - rule
  - skin
  - signature
```

外部IRが受け取れない項目は Adapter で明示的に落とす。

```rust
impl MochaAdapter {
    fn to_mocha_score(&self, score: &IrScoreSubmission) -> MochaScorePayload {
        // BMZ固有項目は捨てる or meta/commentに詰める
    }
}
```

情報を落とす箇所はログ・コメントで明示する。

---

## 24. Codex 実装タスク案

### Phase 1: BMZ client model / trait

- `IrProvider` trait を追加。
- `IrSubmitOptions` / `IrSubmitResponse` / `IrRankingQuery` / `IrRankingScope` を追加。
- `IrChartIdentity` / `IrScoreSubmission` / `IrRanking` を定義。
- リザルト画面用の `ResultRankingState` を追加。
- IR 設定項目を `profile.toml` 側に追加。

### Phase 2: Local DB / Job Queue

- BMZ 本体の score 保存に `bp` / `cb` 集計を追加する。
- `score_history` / `score_best` / replay slot metrics の `bp` / `cb` を IR payload / best update に接続する。
- BMZ 本体の入力 pipeline に `keyboard` / `controller` の device kind を残し、human press input の多数決で `device_type` を決める。
- `score_history` / `score_best` に `device_type` を保存し、IR payload の `play_options.device_type` へ接続する。
- ローカル表示用の `play_count` / `clear_count` を local score DB から集計または保存する。ただし IR 送信値には使わない。
- `ir_accounts`
- `ir_score_jobs`
- `ir_score_submissions`
- retry / idempotency 管理

### Phase 3: BMZ Official Provider

- OAuth/OIDC login flow 連携。
- token refresh。
- Credential Store 保存。
- `POST /api/v1/scores` 実装。
- `GET /api/v1/charts/{sha256}/ranking` 実装。

### Phase 4: NuxtHub / Drizzle IR server

- `players`, `charts`, `scores`, `best_scores`, `rival_relationships`, `device_keys`, `replay_objects` を作成。
- Score submission route 実装。
- `ln_policy` を score / best / ranking / stats の分離キーとして実装する。
- `play_count` / `clear_count` は submission payload から受け取らず、`scores` 投稿履歴から集計する。
- Ranking route 実装。
- Ranking / Chart detail response で必要に応じて `play_count` / `clear_count` を返す。
- Rival route 実装。
- `include=rankings&ranking_scopes=...` に対応。

### Phase 5: Replay / Evidence

- canonical payload hash 実装。
- Ed25519 device key 署名。
- server-side signature verification。
- replay hash 保存。
- replay upload / download / verify API 実装。
- device key 登録 / 一覧 / 失効 API 実装。

---

## 25. 未決事項 / 要レビュー

Codex にレビューしてほしい点。

1. `rankings` response の wrapper 形式
   - `rankings.global.succeeded/data` の形でよいか。
   - それとも `rankings.global = null | IrRanking` にして error は別 field にするか。

2. `best_scores` の採用条件
   - ランキング主軸は EX score 固定。
   - 同点時に clear / minbp / mincb / combo を best 更新判定に含めるか。
   - ランキング順位は EX score のみで同点同順位にする。
   - LN/CN/HCN の分離は `docs/ln.md` の `LnScorePolicy` 準拠で固定し、`best_scores` の unique key に含める。

3. `scope_rank` の必要性
   - 初期から返すか。
   - UI側で計算するか。

4. replay upload API
   - 実装済み。score submit response には upload URL を含めず、
     `POST /api/v1/scores/{id}/replay/upload-url` で取得する。
   - replay object の hash 検証は
     `POST /api/v1/scores/{id}/replay/verify` で行う。

5. tamper evidence
   - canonical JSON を採用するか。
   - MessagePack 等の binary canonical format を採用するか。
   - Ed25519 key の生成・保存・rotate・revoke をどう実装するか。
   - クライアント時計由来の `played_at` とサーバー受信時刻のズレをどう表示・警告するか。

6. NuxtHub / Drizzle route 分担
   - submit + best update + ranking fetch を route/service 内で書く。
   - D1 / Turso 移行を見据え、DB 固有 RPC には寄せない。

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

## note

`bun dev` 等でエラーが起きる場合は `export TMPDIR=/tmp` を実行してみる。

### Setup Local Environment

Install NodeJS / bun

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
