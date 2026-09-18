# rianIR Compatibility Provider Plan

BMZ Player から beatoraja 向け IR の rianIR へ接続するための設計・実装計画。
BMZ 公式 IR の設計は `docs/ir.md`、LN 正規化は `docs/ln.md`、判定・ゲージの
rule は `docs/rule.md`、アシストを含む保存・送信条件は
`docs/score-persistence.md` を正とし、本ドキュメントでは rianIR との互換境界だけを扱う。

## Goal

- rianIR の既存アカウントでログインできる。
- BMZ の単曲スコアを rianIR 互換 payload に変換して送信できる。
- 選曲・リザルト表示に必要な単曲ランキングを取得できる。
- BMZ のコース完走・途中 FAILED の終端結果を rianIR へ送信できる。
- rianIR が提供する難易度表、POPULAR、レビュー、Rivals RECENT、コースを選曲から利用できる。
- クライアント実装と判定・ゲージ仕様を別フィールドとして扱い、同じ rule の
  beatoraja / LR2oraja / BMZ スコアを同じランキング条件で比較できる。

rianIR は BMZ 公式 IR とは別 provider とする。credential、送信 queue、エラー状態、
ランキング cache は provider key で分離し、片方の障害やログアウトをもう片方へ
波及させない。

## Initial Scope

初期リリースでは次の6経路を実装対象とする。

| 機能 | rianIR endpoint | BMZ側の責務 |
| --- | --- | --- |
| login | `POST /api/auth/login.php` | ID/password を送り、`player_name` と `api_token` を provider credential として保存する |
| 単曲送信 | `POST /api/score/score.php` | BMZ score/result/chart を rianIR payload へ変換し、署名して送る |
| 単曲ランキング取得 | `GET /api/score/get_score.php` | chart SHA-256 と `body` を条件に取得し、BMZ の共通ランキング型へ変換する |
| course送信 | `POST /api/score/course_score.php` | 完走または途中 FAILED の course attempt を変換し、署名して送る |
| courseランキング取得 | `GET /api/score/get_course_score.php` | `rian_course_hash_v1` と `body` で取得する |
| テーブル取得 | `GET /api/common/get_tables.php?id=...` | rianIR の動的folder/courseを既存の難易度表・course modelへ変換してキャッシュする |

初期スコープ外:

- rianIR 上でのアカウント登録
- 過去スコアの一括 backfill
- replay upload/download
- ライバル管理
- rianIR のユーザー・譜面管理画面
- rianIR から BMZ のローカル best への score import
- Battle / Battle AS 専用ランキング
- `client_hash` による正式なリリース検証

実装順は login、単曲送信、単曲ランキング取得、course送信、courseランキング取得
とする。単曲送信の変換・署名・送信可否判定を共通部品にし、course送信から再利用する。

## Current BMZ Implementation

`codex/rian-ir` の初期実装は次を含む。

- `rian-ir` providerのlogin、credential保存、local logout
- 単曲・course payload変換とHMAC-SHA256署名
- 送信request logからの `api_token` / `signature` 除去
- 既存IR queueを使った単曲・course送信
- 選曲・リザルト・CLIのglobal単曲ランキング取得
- 選曲・リザルトのglobal courseランキング取得
- BMZ内部hashと分離した `rian_course_hash_v1` によるcourse送信・ランキング取得
- 元譜面のLN構成と実際に適用したLN種別を分離した `canonical-v1` 送信
- 正規化後のAuto/Force LN/CN/HCNすべてに対応するprovider別eligibility
- rianIR向けlocal score backfillの新規投入・既存queue送信の無効化
- Battle / Battle ASの非送信
- rianIR成功後にBMZ公式IR用replay/evidence jobを作らないcapability分離
- `ir rivals` によるrianIRライバル一覧のプロファイル同期
- 選択中ライバル1人分だけの全曲ベスト取得、`network.db`キャッシュ、ETag再検証
- 選曲中の7キー切替と、ライバルスコアのプレイ時ターゲット自動適用
- `NONE` / `RIVALCHART` / `RIVALOPTION` の譜面再現モード

初回loginは次を使う。rianIRでは `--id` がログインIDであり、表示名やメールアドレス
ではない。`--email` は既存CLIとの後方互換aliasとして残す。

```bash
cargo run -p bmz-player -- ir login --provider rian-ir --id <LOGIN_ID>
```

`rian-ir` のbase URLを省略した場合は `https://rianir.link/api/` を使う。

## Provider and Authentication

base URL は設定可能にし、既定値を rianIR の production URL とする。endpoint の
組み立てでは base URL の末尾 slash を正規化し、HTTP timeout を設ける。

login request:

```json
{
  "id": "player id",
  "pass": "password"
}
```

成功 response の `data.attributes.player_name` と `data.meta.api_token` を保存する。
password は保存しない。game client からの register は rianIR 側で禁止されているため、
未登録ユーザーは Web サイトへ案内する。

単曲署名は `api_token` を鍵とする HMAC-SHA256 とし、compact JSON の次の配列を
署名対象にする。

```text
[player_name, sha256, exscore, maxcombo, minbp, date]
```

course署名は次の配列を署名対象にする。

```text
[player_name, course_sha256, exscore, maxcombo, date]
```

署名生成は payload の文字列表現を含めて golden test で固定する。表示用に整形した
JSON やフィールド順が異なる object を署名対象にしてはならない。

## Client and Rule Contract

`client` はゲームクライアントの実装、`client_version` はそのバージョンを表す。
判定窓・ゲージ・コンボなどのスコア規則は既存フィールド `body` で表し、両者を
混同しない。rianIR側に新しいrule fieldは追加しない。

client の canonical value:

| client | 意味 |
| --- | --- |
| `beatoraja` | upstream beatoraja |
| `LR2oraja` | LR2oraja |
| `LR2oraja-EndressDream` | LR2oraja EndlessDream |
| `bmz-player` | BMZ Player |

BMZ は常に次を送る。

```text
client = bmz-player
client_version = <BMZの製品バージョン>
```

`client_version` は `0.8.8` のような製品名を含まない値とする。rianIR 側では旧値の
`LR2orajaED` を `LR2oraja-EndressDream` の alias として受理する。特定 client の
version prefix 補正を、`bmz-player` を含む他 client へ適用してはならない。

BMZ の `RuleMode` と rianIR の canonical rule value は次のように対応させる。

| BMZ `RuleMode` | rianIR rule value |
| --- | --- |
| `Beatoraja` | `beatoraja` |
| `Lr2Oraja` | `LR2oraja` |
| `Dx` | `DX MODE` |

rianIRの既存契約どおり、この値を `body` に格納する。BMZは `rule_mode` を
追加送信せず、送信とランキング取得の双方で同じ `body` 値を使う。

```text
body = LR2oraja
client = bmz-player
```

`body=bmz` は送らない。BMZはclientの名前であり、判定・ゲージ仕様ではない。
ランキング条件もclientではなく `body` で分離する。

## Score Eligibility

provider 固有の送信可否は result 保存後、rianIR job を enqueue する直前に判定する。
Battle / Battle AS のような provider 固有の非送信 result は、ローカル score と replay
には通常どおり保存する。一方、実効アシスト、autoplay、replay、practice、key mode
変換は共通保存経路で先に除外され、rianIR の判定まで到達しない。非送信理由をUIへ
表示する機能は後続課題とする。

初期実装で送信可能なのは次をすべて満たすプレイだけとする。

- 通常の手動プレイである。
- chart の SHA-256 を取得できる。
- 元譜面のLN profileと、正規化後の `LnScorePolicy` を取得できる。
- `DoubleOption` が `Off` または `Flip` である。
- rianIR が受理する key mode である。
- score/result の必須値が rianIR の範囲に収まる。

次は送信しない。

- autoplay
- replay再生
- practice
- 実効 `LightAssist` / `Assist`
- `DoubleOption::Battle`
- `DoubleOption::BattleAutoScratch`（画面表記 `BATTLE AS`）
- 中断され、rianIR の score contract を満たさない result

Battle / Battle AS はレーン・ノーツ数とランキング条件を変える一方、rianIR の
通常スコア区分で安全に表現できないためである。`Off` と `Flip` は rianIR の通常
DP契約で扱う。

## LN Contract

BMZは単曲score requestへ `ln_mode_format=canonical-v1` を必ず付ける。
FORCE変換後のchartから元譜面の構成を逆算せず、import直後に取得したLN profileを
プレイ終了まで保持する。

`song_ln_mode` は譜面に含まれるLN種別を表し、混在時は HCN > CN > LN の順で
上位種を採用する。

| 元譜面 | `song_ln_mode` |
| --- | ---: |
| LN系ノートなし | `0` |
| 定義LNのみ、未定義LNのみ、またはその混在 | `1` |
| CNを含み、HCNを含まない | `2` |
| HCNを含む | `3` |

`ln_mode` は実際に適用された譜面のLN種別を表す。LN系ノートがなければ設定に
かかわらず `0`、FORCE LN/CN/HCNならそれぞれ `1` / `2` / `3` とする。AUTOでは
定義済みLNの種別を維持し、未定義LNに適用するfallbackを加えた結果から
HCN > CN > LN の上位種を採用する。したがって、定義CN譜面をFORCE LNで
プレイした場合は `song_ln_mode=2, ln_mode=1`、LN+CN混在譜面をAUTO LNで
プレイした場合は `song_ln_mode=2, ln_mode=2` となる。

過去スコアpayloadの組み立て自体も同じ規則で実効LN種別を算出する。ただし、
rianIRへのlocal score backfillは運用上無効化する。`ir upload-local` でrianIRを
指定した場合はqueueへ投入せずエラーにし、以前に作成済みの
`submission_source=local_backfill` jobも送信直前に拒否する。

rianIR のランキング取得 API が `ln_mode` を filter/group key に含めるまでは、
異なる FORCE mode のスコアが混ざる可能性がある。初期版はこの制約を明示した上で
既存APIを利用し、LN別filter/groupingはrianIR側の後続改善とする。courseランキング
にも同じ制約がある。

## Key Mode Contract

beatoraja 由来の mode 名に合わせ、BMZ の key mode を次の `play_mode` へ変換する。

| BMZ key mode | rianIR `play_mode` |
| --- | --- |
| 4K | `beat-4k` |
| 5K | `beat-5k` |
| 6K | `beat-6k` |
| 7K | `beat-7k` |
| 8K | `beat-8k` |
| 9K | `popn-9k` |
| 10K | `beat-10k` |
| 14K | `beat-14k` |

4K / 6K / 8K は SP として扱う。rianIR 側では保存だけでなく、ランキング、検索、
ユーザー統計、譜面表示の mode classifier へ追加する。同じ SHA-256 の
`songs.play_mode` を投稿のたびに別値へ上書きしないよう、canonical mode の
upsert規則も rianIR 側で固定する。

BMZは初期版から `beat-4k` / `beat-6k` / `beat-8k` を送る。rianIR側では予定されて
いる表示・検索・統計対応を追加するが、score API schemaの変更は不要である。

## Arrange and Seed Contract

rianIR の legacy `play_option` は次の decimal packing を使う。

```text
option_1p + option_2p * 10 + double_option * 100
```

ただし F-RANDOM / MF-RANDOM は legacy 数値だけでは表せないため、structured field を
正とする。

| BMZ arrange | structured value |
| --- | --- |
| `FRandom` | `f-random` |
| `MFRandom` | `mf-random` |

SP は `arrange_1p`、DP は `arrange_1p` と `arrange_2p` を送る。
`double_option` は `off` または `flip` を送る。rianIR は structured field があれば
それを優先し、許可値の検証と lower-case canonicalization を行う。F/MF の場合も
`play_option` は互換用の安全な fallback 値を送るが、ランキング・表示の判定には
使わない。

random seed は beatoraja 互換の side 別 24 bit seed を使う。

## Rival Target and Cache Contract

rianIRの `GET /api/score/get_rivals.php?id=...` はライバル関係の一覧同期に使う。
BMZから追加・解除は行わず、`ir rivals` 実行時にprimary rianIRの一覧で
`profile.rival.entries` の同一provider分だけを更新する。

選択ライバルのスコアは、追加した軽量endpoint
`GET|POST /api/score/get_rival_scores.php` から取得する。`rival_id` と `body` は
クエリまたはJSONボディで渡せる。
全ライバルを起動時に取得せず、7キーで現在選ばれた1人だけを取得して
profileの `network.db` に保存する。レスポンスは次のフィールドだけを返す。

| field | 用途 |
| --- | --- |
| `sha256`, `ln_mode` | 譜面・実効LNモード単位のキー |
| `ex_score` | 自動ターゲット |
| `clear_type`, `max_combo`, `min_bp` | 選曲スキンのライバル値 |
| `play_option` | 旧クライアント用配置fallback |
| `arrange_1p`, `arrange_2p`, `double_option` | 配置のcanonical値 |
| `play_seed` | `RIVALCHART`の24bit side別seed |

ghost、判定内訳、履歴、曲メタデータ、`min_cb`は取得しない。サーバーは
譜面/LNモードごとに最高EXの行（同点なら最新id）の配置とseedを返し、
clear / max combo / min BPだけを全履歴から集約する。ETagが一致した場合は304を返す。

選択ライバルが対象譜面をプレイ済みなら、そのEXスコアと表示名をターゲットにする。
未プレイならライバルターゲットを作らず、beatorajaと同じく通常のTARGET設定を使う。
譜面再現モードは次の3値で、既定値は `RIVALCHART`。

| mode | 動作 |
| --- | --- |
| `NONE` | ターゲットだけ適用し、現在の配置を維持 |
| `RIVALOPTION` | ライバルの配置種別だけ適用し、seedは新規抽選 |
| `RIVALCHART` | ライバルの配置種別と`play_seed`を適用 |

structured配置をlegacy `play_option`より優先するため、BMZから送信された
F-RANDOM / MF-RANDOMも再現できる。ghostによる進行ターゲットは実装対象外。

```text
SP: play_seed = p1_seed
DP: play_seed = p1_seed + p2_seed * 2^24
0 <= p1_seed, p2_seed < 2^24
```

seed は実際にプレイへ適用され、replayへ保存された値から作る。設定画面の一時値や
再生成した乱数を送ってはならない。BMS `#RANDOM` の分岐選択列は arrange seed と
別概念であり、`play_seed` へ混ぜない。

## Score Field Mapping Notes

変換処理は rianIR provider 内の純粋関数として実装し、BMZ の score model や DB に
rianIR の数値 enum を持ち込まない。

特に判定名は次のように変換する。

| BMZ | rianIR |
| --- | --- |
| `PGreat` | `pgreat` |
| `Great` | `great` |
| `Good` | `good` |
| `Bad` | `bad` |
| `Poor` | `poor` |
| `EmptyPoor` | `miss` |

rianIR/beatoraja の `miss` は見逃しではなく Empty Poor に相当する。BMZ の `Poor` と
`EmptyPoor` を合算してはならない。FAST/SLOW は各判定の合計値へ畳み込む。

`date` はunix秒で送る。曲長はプレイ時の変換後譜面から再計算せず、スキャン時に
`library.db.charts.length_ms` へ保存した値を使う。legacy互換の `length` は秒、
`length_ms` はミリ秒で同じ曲長を送る。実プレイ時間はアプリの描画時刻ではなく
audio output callbackのフレーム数から、譜面時刻0（READY用の負の開始マージンは除外）
から最初の `Finished` / `Failed` までを計測し、`play_duration` は秒、
`play_duration_ms` はミリ秒で送る。

BMS `#RANDOM` / `#SETRANDOM` を含む譜面は `has_random=true` を送り、曲長を0には
しない。rianIR側はこのフラグがtrueのときだけ曲長と実プレイ時間の速度検査を
スキップする。`Failed` も途中落ちを正常に記録できるよう速度検査の対象外とする。
非RANDOMのclear scoreはserverと同じ許容幅（短い側 `0.85 * length - 5秒`、長い側
`1.15 * length + 15秒`）をBMZでもqueue投入前に検査し、恒久的に400になるjobを
作らない。

duration-aware対応前に保存済みのqueue jobには `chart.length_ms` が無いため、再送時は
`length=0` の旧wire形式を維持し、当時の譜面終了時刻を実プレイ時間として
`play_duration_ms` へ読み替えない。

曲名などの文字列 metadata は `B64:<base64>` 形式を使用し、UTF-8 と JSON escaping の
差で署名や request が壊れないようにする。

## Course Submission

course送信にも単曲と同じ client/rule/LN/arrange の送信可否判定を適用する。

- course が完走または途中 FAILED で終端 Result に到達した attempt を送る。
- course全体を単一の Force LN mode で表現できる場合だけ送る。
- Battle / Battle AS を使用した attempt は送らない。
- 初期版は誤った曲 metadata を登録しないため `tracks=[]` とする。BMZの既存course
  queueはstage SHA-256を保持するが、曲名・artist・levelまでは保持しないためである。
  正確なstage metadata送信はqueue拡張後の後続課題とする。
- rianIRへ送る `course_sha256` とランキング取得には次の
  `rian_course_hash_v1` を使う。

```text
rian_course_hash_v1 =
    SHA256(UTF-8(decoded_course_title + ordered_stage_sha256_hex_strings))
```

BMZ内部・BMZ公式IR用のcanonical course hashは変更しない。course identityには両方を
保持し、provider境界で使い分ける。
- course constraint、gauge、total notes、判定合計が server の期待値と一致することを
  送信前に検証する。

course endpoint の `client_hash` 検証方針は単曲 endpoint と統一する。

## `client_hash` Manifest

release buildはbeatorajaのJAR hashと同じ考え方で、実行ファイルそのものの
SHA-256（小文字64桁hex）を `client_hash` として送る。archive、installer、app bundle
全体のhashではない。

Windows/macOS/Flatpak/Linux tarの各release buildは最終実行ファイルから内部用manifestを生成する。
最終release jobは全targetのmanifestを検証・集約し、GitHub ReleaseにはrianIR importer
互換manifestを `client-manifest-bmz-player-vX.Y.Z.json` として1ファイルだけ含める。

```json
{
  "schema": "bmz-rianir-client-manifest-v1",
  "client": "bmz-player",
  "version": "0.1.11",
  "git_commit": "<40 hex>",
  "builds": [
    {
      "platform": "windows",
      "arch": "x86_64",
      "package_kind": "portable-installer",
      "client_hash": "<64 hex>"
    },
    {
      "platform": "macos",
      "arch": "aarch64",
      "package_kind": "app",
      "client_hash": "<64 hex>"
    },
    {
      "platform": "macos",
      "arch": "x86_64",
      "package_kind": "app",
      "client_hash": "<64 hex>"
    },
    {
      "platform": "linux",
      "arch": "x86_64",
      "package_kind": "flatpak",
      "client_hash": "<64 hex>"
    },
    {
      "platform": "linux",
      "arch": "x86_64",
      "package_kind": "tar",
      "client_hash": "<64 hex>"
    }
  ]
}
```

集約時はclient、version、commitの一致、targetの重複・欠落、hash形式を検証し、
targetをrianIRのplatform、arch、package_kindへ変換する。rianIR管理者はmanifestの
version、commit、各buildを確認してから `builds[].client_hash`を `allowed_clients`
へ登録する。失効時は同テーブルから削除する。

Linux tarは `linux-x64-tar` を `platform=linux, arch=x86_64, package_kind=tar` に変換する。
ハッシュは `patchelf` 後のアーカイブ内 `bin/bmz-player` の検証済み値を使う。
初回公開前にrianIR管理者と `package_kind=tar` の受入を確認し、当該hashを登録する。
Releaseへのmanifest添付だけではallowlist登録は行われない。

ローカル開発でdebug/release profileを繰り返しbuildするときは、コンパイル時の
`BMZ_RIANIR_DEV_CLIENT_HASH`に個人用の小文字64桁hexを設定できる。設定値は
`option_env!`で実行ファイルへ埋め込み、build profileにかかわらず優先する。実行時の
環境変数ではないため、配布済みの公式実行ファイルを後から差し替える用途には使えない。
公式release workflowはこの環境変数が設定されている場合、package build前に失敗する。

PowerShellではhashを設定したshellから通常のrelease profileを起動する。

```powershell
$env:BMZ_RIANIR_DEV_CLIENT_HASH = "<個人用の小文字64桁hex>"
cargo run -r -p bmz-player
```

macOS / Linuxでは同じbuild invocationへ環境変数を渡す。

```bash
BMZ_RIANIR_DEV_CLIENT_HASH="<個人用の小文字64桁hex>" cargo run -r -p bmz-player
```

開発用固定値が未設定のdebug build、固定値の形式が不正なbuild、実行ファイルhashの
取得に失敗したrelease buildだけ、暫定互換値 `UNKNOWN` を送る。固定値が未設定の
release buildは従来どおり実行ファイルのSHA-256を送る。

Flatpakは配布 `.flatpak` 全体ではなく、`flatpak-builder` が最終配置した
`build/files/bin/bmz-player`（実行時の `/app/bin/bmz-player`）をhash対象にする。

## Error and Queue Policy

- 初期版は401/403/429を含め、既存queueのbackoffを使う。
- 401/403をcredentialまたはpayloadのterminal errorとして分類し、自動retryを止める
  改善は後続課題とする。
- 429 の `Retry-After` 対応も後続課題とする。
- timeout、接続失敗、5xx は retry可能とする。
- payload validation error と unsupported play は retryしない。
- 単曲送信に成功してランキング取得だけ失敗した場合、score job を失敗へ戻さない。
- 同じローカル score history を複数回送らないよう、provider単位の submission ledger
  または idempotency key を持つ。
- rianIR が player/chart/date 単位で重複判定する可能性があるため、date の再生成や
  秒単位への丸め直しを retry ごとに行わない。

## Known Constraints

- `body` は既存の判定・ゲージ仕様区分として維持し、BMZ識別には使わない。
- rianIR の現在の通常スコア送信はallowlist済みの実行ファイル `client_hash` を要求する。
  開発用固定値が未設定のdebug buildは暫定的に `UNKNOWN` を使うため、正式な
  クライアント検証にはならない。
- LN mode を含めないランキング query/grouping では Force LN/CN/HCN が混在する。
- 4K / 6K / 8K は保存できても、rianIR の統計や画面から漏れる可能性がある。
- F-RANDOM / MF-RANDOM は structured arrange field 対応版の rianIR が必要。
- 同じ chart SHA-256 の metadata を別 client が異なる mode/LN 判定で投稿した場合の
  canonical metadata ownership を rianIR 側で決める必要がある。
- rianIR の API error は JSON:API形式と旧形式が混在する可能性があるため、
  HTTP statusを主、response bodyを補助情報として扱う。
- rianIR の client version 補正は BMZ 1.x を誤変換しないよう client別に限定する必要がある。
- 現行beatoraja/rianIR connectorはcourse送信時に `score.sha256`（stage SHA-256の
  単純連結）を優先する一方、取得時は `rian_course_hash_v1` を使う。connector側で
  送信・取得・URL生成を同じhelperへ統一するまで、beatorajaからの新規course scoreと
  BMZの横断ランキングは成立しない。
- `rian_course_hash_v1` はconstraintsを含まないため、同じタイトル・同じ曲順で
  constraintsだけが異なるcourseは現行rianIRと同様に同じランキングへ混在する。
- courseの `ln_mode` は現行connectorで複合値になり得る。BMZ初期版はForce LN/CN/HCN
  の実効値1/2/3を送るが、既存beatoraja scoreとの分類互換は後続課題とする。
- 初期実装では server-side replay verification が無く、送信内容の正当性を完全には
  証明できない。

## Minimal rianIR-side Work

初期接続のためのDB schema変更や新endpoint追加は行わない。BMZは現行のlogin、
score、get_score、course_score、get_course_scoreをそのまま利用する。

rianIR側の初期作業は次に限定する。

- `beat-4k` / `beat-6k` / `beat-8k` を検索・表示・統計のmode classifierへ追加する。
- Java connectorに `rian_course_hash_v1` helperを1つ追加し、
  `sendCoursePlayData` / `getCoursePlayData` / `getCourseURL`から共通利用する。
  `sendCoursePlayData`での `score.sha256` 優先は廃止する。
- 現行API testのcourse成功response期待値を実Controllerの
  `{"status":"success"}` に合わせる。

次は初期接続を止めず、後続課題とする。

- get_score / get_course_scoreのLN別filter・集約
- constraints込みのcourse hash v2とcourse `ln_mode` の統一
- `client=bmz-player` が1.xになったときのversion prefix補正除外
- manifestへのrelease署名とrianIRへのallowlist登録自動化

## Verification Plan

### BMZ unit tests

- `RuleMode` 3種と `client=bmz-player` の mapping。
- clear type、gauge、assist、legacy option の全 enum mapping。
- `Poor -> poor`、`EmptyPoor -> miss` と FAST/SLOW 集計。
- `ln_mode_format=canonical-v1` が常に付くこと。
- LNなし、未定義LN、定義LN/CN/HCN、混在譜面について、Auto/Forceの
  `song_ln_mode` / `ln_mode` matrixが規則どおりになること。
- Auto 3種を送信でき、LNなし譜面は設定にかかわらず `0 / 0` になること。
- rianIR向けlocal score backfillが新規投入時と既存queue送信時の双方で拒否されること。
- 4K/6K/8Kを含む全対応 key mode の `play_mode` mapping。
- 1P/2P の F-RANDOM / MF-RANDOM structured field。
- Off/Flip は送信でき、Battle/Battle AS は送信できないこと。
- SP seed、DP packed seed、24 bit境界値。
- unix秒、曲長、play duration の単位変換。
- UTF-8タイトルとstage順を含む `rian_course_hash_v1` のRust/Java共通golden fixture。
- login/score/course request と HMAC の golden fixture。
- retry時に date、signature、seed が変わらないこと。
- primary provider が rianIR の場合の難易度表・POPULAR・レビュー・Rivals RECENT・
  course取得。既存の難易度表folderとcourse表示を再利用する。

### rianIR table cache lifecycle

- cache scopeは `provider_key + base_url + account_id` とし、scopeをSHA-256 digest化した
  内部source URLでアカウント間を分離する。ログインIDをDBのsource URLへ直接含めない。
- アプリ起動後の初回描画直後に取得する。前回cacheがあれば取得完了を待たずに表示する。
- 起動中は30分間隔で再取得する。rianIR側のcommon table cacheは10分なので、それより
  短いpollingは行わない。
- rianIR由来table内、またはrootでrianIR tableを選択中のF5は、そのアカウントの
  table一式を即時更新する。5秒以内の連打と同時実行は抑止する。
- 通信失敗時は既存cacheを残す（stale-while-revalidate）。
- primary provider、base URL、account IDの変更とlogoutでは旧scopeのtable/course
  cacheを削除して選曲から即時非表示にする。変更前に開始した通信結果はgenerationで
  破棄し、旧アカウントのcacheを復活させない。
- rianIR側の変更は不要で、既存 `get_tables.php` responseをそのまま利用する。

### rianIR integration tests

- `client=bmz-player`、version、3種の ruleを保存・取得できる。
- `body` が既存のrule bucketとして保存・取得に使われる。
- `LR2orajaED` alias と `LR2oraja-EndressDream` canonical value。
- BMZ 1.x version が別client用prefixへ書き換えられない。
- 現行APIではForce LN/CN/HCNが混在することをfixtureで固定し、LN filter追加後に
  分離されること。
- `beat-4k` / `beat-6k` / `beat-8k` がランキング・検索・統計へ現れる。
- F-RANDOM / MF-RANDOM が1P/2P別に保存・表示される。
- `UNKNOWN` client hash の暫定許可と未検証表示。
- 不正 token、signature、client hash、enum、範囲外 score の拒否。
- rate limit response と `Retry-After`。
- 単曲とcourseで client hash・rule・LN validation が一致する。

### Manual end-to-end

1. rianIR のテストアカウントで BMZ から login する。
2. 5K/7K/14K と 4K/6K/8K を各1曲プレイし、rule・mode・判定内訳をDBと画面で確認する。
3. 同じ譜面を Force LN/CN/HCN でプレイし、現行APIで混在する制約を確認する。
4. F-RANDOM / MF-RANDOM の SP/DP scoreで arrange、seed、replay再現結果を照合する。
5. Battle / Battle AS はローカル保存のみ、実効アシスト（SPIRAL / H-RANDOM /
   ALL-SCR / RANDOM-EX / S-RANDOM-EXを含む）は assist lamp と統計だけを更新し、
   数値score・履歴・replay・IRを保存しないことを確認する。autoplayも送信されないことを確認する。
6. courseの完走と途中 FAILED を各1回行い、course scoreを確認する。初期版では
   `tracks=[]` であることも確認する。
7. 通信切断、401、403、429、5xxの各ケースで queue とUI表示を確認する。

## Implementation Pointers

BMZ側:

- IR provider境界: `crates/bmz-player/src/ir/`
- 共通 score payload: `crates/bmz-player/src/ir/payload.rs`
- course payload: `crates/bmz-player/src/ir/course_payload.rs`
- queue/sync: `crates/bmz-player/src/ir/sync.rs`
- LN正規化: `crates/bmz-player/src/ln_policy.rs`
- arrange/double option: `crates/bmz-player/src/select_options.rs`
- result組み立て: `crates/bmz-player/src/screens/play_finish.rs`

参照実装:

- rianIR Java connector: `.local/rianIR/src/main/java/bms/player/beatoraja/ir/rianIR.java`
- rianIR score service: `.local/rianIR/src/Service/ScoreService.php`
- rianIR signature: `.local/rianIR/src/Service/SignatureService.php`
- rianIR ranking query: `.local/rianIR/src/Core/Score/ScoreQueryService.php`
- beatoraja score DTO: `.local/beatoraja/src/bms/player/beatoraja/ir/IRScoreData.java`
