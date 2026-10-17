# rianIR Compatibility Provider Plan

BMZ Playerからbeatoraja向けIRのrianIRへ接続するための設計・実装計画です。
BMZ公式IRの設計は`docs/ir.md`、LN正規化は`docs/ln.md`、判定・ゲージの
ruleは`docs/rule.md`、アシストを含む保存・送信条件は
`docs/score-persistence.md`を正とし、本ドキュメントではrianIRとの互換境界だけを扱います。

## Goal

- rianIRの既存アカウントでログインできます。
- BMZの単曲スコアをrianIR互換payloadに変換して送信できます。
- 選曲・リザルト表示に必要な単曲ランキングを取得できます。
- BMZのコース完走・途中FAILEDの終端結果をrianIRへ送信できます。
- rianIRが提供する難易度表、POPULAR、レビュー、Rivals RECENT、コースを選曲から利用できます。
- クライアント実装と判定・ゲージ仕様を別フィールドとして扱い、同じruleの
  beatoraja / LR2oraja / BMZスコアを同じランキング条件で比較できます。

rianIRはBMZ公式IRとは別providerとします。credential、送信queue、エラー状態、
ランキングcacheはprovider keyで分離し、片方の障害やログアウトをもう片方へ
波及させません。

## Initial Scope

初期リリースでは次の6経路を実装対象とします。

| 機能 | rianIR endpoint | BMZ側の責務 |
| --- | --- | --- |
| login | `POST /api/auth/login.php` | ID/passwordを送り、`player_name`と`api_token`をprovider credentialとして保存する |
| 単曲送信 | `POST /api/score/score.php` | BMZ score/result/chartをrianIR payloadへ変換し、署名して送る |
| 単曲ランキング取得 | `GET /api/score/get_score.php` | chart SHA-256と`body`を条件に取得し、BMZの共通ランキング型へ変換する |
| course送信 | `POST /api/score/course_score.php` | 完走または途中FAILEDのcourse attemptを変換し、署名して送る |
| courseランキング取得 | `GET /api/score/get_course_score.php` | `rian_course_hash_v1`と`body`で取得する |
| テーブル取得 | `GET /api/common/get_tables.php?id=...` | rianIRの動的folder/courseを既存の難易度表・course modelへ変換してキャッシュする |

初期スコープ外:

- rianIR上でのアカウント登録
- 過去スコアの一括backfill
- replay upload/download
- ライバル管理
- rianIRのユーザー・譜面管理画面
- rianIRからBMZのローカルbestへのscore import
- Battle / Battle AS専用ランキング
- `client_hash`による正式なリリース検証

実装順はlogin、単曲送信、単曲ランキング取得、course送信、courseランキング取得
とします。単曲送信の変換・署名・送信可否判定を共通部品にし、course送信から再利用します。

## Current BMZ Implementation

`codex/rian-ir`の初期実装は次を含みます。

- `rian-ir` providerのlogin、credential保存、local logout
- 単曲・course payload変換とHMAC-SHA256署名
- 送信request logからの`api_token` / `signature`除去
- 既存IR queueを使った単曲・course送信
- 選曲・リザルト・CLIのglobal単曲ランキング取得
- 選曲・リザルトのglobal courseランキング取得
- BMZ内部hashと分離した`rian_course_hash_v1`によるcourse送信・ランキング取得
- 元譜面のLN構成と実際に適用したLN種別を分離した`canonical-v1`送信
- 正規化後のAuto/Force LN/CN/HCNすべてに対応するprovider別eligibility
- rianIR向けlocal score backfillの新規投入・既存queue送信の無効化
- Battle / Battle ASの非送信
- rianIR成功後にBMZ公式IR用replay/evidence jobを作らないcapability分離
- `ir rivals`によるrianIRライバル一覧のプロファイル同期
- 選択中ライバル1人分だけの全曲ベスト取得、`network.db`キャッシュ、ETag再検証
- 選曲中の7キー切替と、ライバルスコアのプレイ時ターゲット自動適用
- `NONE` / `RIVALCHART` / `RIVALOPTION`の譜面再現モード

初回loginは次を使います。rianIRでは`--id`がログインIDであり、表示名やメールアドレス
ではありません。`--email`は既存CLIとの後方互換aliasとして残します。

```bash
cargo run -p bmz-player -- ir login --provider rian-ir --id <LOGIN_ID>
```

`rian-ir`のbase URLを省略した場合は`https://rianir.link/api/`を使います。

## Provider and Authentication

base URLは設定可能にし、既定値をrianIRのproduction URLとします。endpointの
組み立てではbase URLの末尾slashを正規化し、HTTP timeoutを設けます。

login request:

```json
{
  "id": "player id",
  "pass": "password"
}
```

成功responseの`data.attributes.player_name`と`data.meta.api_token`を保存します。
passwordは保存しません。game clientからのregisterはrianIR側で禁止されているため、
未登録ユーザーはWebサイトへ案内します。

単曲署名は`api_token`を鍵とするHMAC-SHA256とし、compact JSONの次の配列を
署名対象にします。

```text
[player_name, sha256, exscore, maxcombo, minbp, date]
```

course署名は次の配列を署名対象にします。

```text
[player_name, course_sha256, exscore, maxcombo, date]
```

署名生成はpayloadの文字列表現を含めてgolden testで固定します。表示用に整形した
JSONやフィールド順が異なるobjectを署名対象にしてはなりません。

## Client and Rule Contract

`client`はゲームクライアントの実装、`client_version`はそのバージョンを表します。
判定窓・ゲージ・コンボなどのスコア規則は既存フィールド`body`で表し、両者を
混同しません。rianIR側に新しいrule fieldは追加しません。

clientのcanonical value:

| client | 意味 |
| --- | --- |
| `beatoraja` | upstream beatoraja |
| `LR2oraja` | LR2oraja |
| `LR2oraja-EndressDream` | LR2oraja EndlessDream |
| `bmz-player` | BMZ Player |

BMZは常に次を送ります。

```text
client = bmz-player
client_version = <BMZの製品バージョン>
```

`client_version`は`0.8.8`のような製品名を含まない値とします。rianIR側では旧値の
`LR2orajaED`を`LR2oraja-EndressDream`のaliasとして受理します。特定clientの
version prefix補正を、`bmz-player`を含む他clientへ適用してはなりません。

BMZの`RuleMode`とrianIRのcanonical rule valueは次のように対応させます。

| BMZ `RuleMode` | rianIR rule value |
| --- | --- |
| `Beatoraja` | `beatoraja` |
| `Lr2Oraja` | `LR2oraja` |
| `Dx` | `DX MODE` |

rianIRの既存契約どおり、この値を`body`に格納します。BMZは`rule_mode`を
追加送信せず、送信とランキング取得の双方で同じ`body`値を使います。

```text
body = LR2oraja
client = bmz-player
```

`body=bmz`は送りません。BMZはclientの名前であり、判定・ゲージ仕様ではありません。
ランキング条件もclientではなく`body`で分離します。

## Score Eligibility

provider固有の送信可否はresult保存後、rianIR jobをenqueueする直前に判定します。
Battle / Battle ASのようなprovider固有の非送信resultは、ローカルscoreとreplay
には通常どおり保存します。一方、実効アシスト、autoplay、replay、practice、key mode
変換は共通保存経路で先に除外され、rianIRの判定まで到達しません。非送信理由をUIへ
表示する機能は後続課題とします。

初期実装で送信可能なのは次をすべて満たすプレイだけとします。

- 通常の手動プレイです。
- chartのSHA-256を取得できます。
- 元譜面のLN profileと、正規化後の`LnScorePolicy`を取得できます。
- `DoubleOption`が`Off`または`Flip`です。
- rianIRが受理するkey modeです。
- score/resultの必須値がrianIRの範囲に収まります。

次は送信しません。

- autoplay
- replay再生
- practice
- 実効`LightAssist` / `Assist`
- `DoubleOption::Battle`
- `DoubleOption::BattleAutoScratch`（画面表記`BATTLE AS`）
- 中断され、rianIRのscore contractを満たさないresult

Battle / Battle ASはレーン・ノーツ数とランキング条件を変える一方、rianIRの
通常スコア区分で安全に表現できないためです。`Off`と`Flip`はrianIRの通常
DP契約で扱います。

## LN Contract

BMZは単曲score requestへ`ln_mode_format=canonical-v1`を必ず付けます。
FORCE変換後のchartから元譜面の構成を逆算せず、import直後に取得したLN profileを
プレイ終了まで保持します。

`song_ln_mode`は譜面に含まれるLN種別を表し、混在時はHCN > CN > LNの順で
上位種を採用します。

| 元譜面 | `song_ln_mode` |
| --- | ---: |
| LN系ノートなし | `0` |
| 定義LNのみ、未定義LNのみ、またはその混在 | `1` |
| CNを含み、HCNを含まない | `2` |
| HCNを含む | `3` |

`ln_mode`は実際に適用された譜面のLN種別を表します。LN系ノートがなければ設定に
かかわらず`0`、FORCE LN/CN/HCNならそれぞれ`1` / `2` / `3`とします。AUTOでは
定義済みLNの種別を維持し、未定義LNに適用するfallbackを加えた結果から
HCN > CN > LNの上位種を採用します。したがって、定義CN譜面をFORCE LNで
プレイした場合は`song_ln_mode=2, ln_mode=1`、LN+CN混在譜面をAUTO LNで
プレイした場合は`song_ln_mode=2, ln_mode=2`となります。

過去スコアpayloadの組み立て自体も同じ規則で実効LN種別を算出します。ただし、
rianIRへのlocal score backfillは運用上無効化します。`ir upload-local`でrianIRを
指定した場合はqueueへ投入せずエラーにし、以前に作成済みの
`submission_source=local_backfill` jobも送信直前に拒否します。

rianIRのランキング取得APIが`ln_mode`をfilter/group keyに含めるまでは、
異なるFORCE modeのスコアが混ざる可能性があります。初期版はこの制約を明示した上で
既存APIを利用し、LN別filter/groupingはrianIR側の後続改善とします。courseランキング
にも同じ制約があります。

## Key Mode Contract

beatoraja由来のmode名に合わせ、BMZのkey modeを次の`play_mode`へ変換します。

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

4K / 6K / 8KはSPとして扱います。rianIR側では保存だけでなく、ランキング、検索、
ユーザー統計、譜面表示のmode classifierへ追加します。同じSHA-256の
`songs.play_mode`を投稿のたびに別値へ上書きしないよう、canonical modeの
upsert規則もrianIR側で固定します。

BMZは初期版から`beat-4k` / `beat-6k` / `beat-8k`を送ります。rianIR側では予定されて
いる表示・検索・統計対応を追加するが、score API schemaの変更は不要です。

## Arrange and Seed Contract

rianIRのlegacy `play_option`は次のdecimal packingを使います。

```text
option_1p + option_2p * 10 + double_option * 100
```

ただしF-RANDOM / MF-RANDOMはlegacy数値だけでは表せないため、structured fieldを
正とします。

| BMZ arrange | structured value |
| --- | --- |
| `FRandom` | `f-random` |
| `MFRandom` | `mf-random` |

SPは`arrange_1p`、DPは`arrange_1p`と`arrange_2p`を送ります。
`double_option`は`off`または`flip`を送ります。rianIRはstructured fieldがあれば
それを優先し、許可値の検証とlower-case canonicalizationを行います。F/MFの場合も
`play_option`は互換用の安全なfallback値を送るが、ランキング・表示の判定には
使いません。

random seedはbeatoraja互換のside別24 bit seedを使います。

## Rival Target and Cache Contract

rianIRの`GET /api/score/get_rivals.php?id=...`はライバル関係の一覧同期に使います。
BMZから追加・解除は行わず、`ir rivals`実行時にprimary rianIRの一覧で
`profile.rival.entries`の同一provider分だけを更新します。

選択ライバルのスコアは、追加した軽量endpoint
`GET|POST /api/score/get_rival_scores.php`から取得します。`rival_id`と`body`は
クエリまたはJSONボディで渡せます。
全ライバルを起動時に取得せず、7キーで現在選ばれた1人だけを取得して
profileの`network.db`に保存します。レスポンスは次のフィールドだけを返します。

| field | 用途 |
| --- | --- |
| `sha256`, `ln_mode` | 譜面・実効LNモード単位のキー |
| `ex_score` | 自動ターゲット |
| `clear_type`, `max_combo`, `min_bp` | 選曲スキンのライバル値 |
| `play_option` | 旧クライアント用配置fallback |
| `arrange_1p`, `arrange_2p`, `double_option` | 配置のcanonical値 |
| `play_seed` | `RIVALCHART`の24bit side別seed |

ghost、判定内訳、履歴、曲メタデータ、`min_cb`は取得しません。サーバーは
譜面/LNモードごとに最高EXの行（同点なら最新id）の配置とseedを返し、
clear / max combo / min BPだけを全履歴から集約します。ETagが一致した場合は304を返します。

選択ライバルが対象譜面をプレイ済みなら、そのEXスコアと表示名をターゲットにします。
未プレイならライバルターゲットを作らず、beatorajaと同じく通常のTARGET設定を使います。
譜面再現モードは次の3値で、既定値は`RIVALCHART`です。

| mode | 動作 |
| --- | --- |
| `NONE` | ターゲットだけ適用し、現在の配置を維持 |
| `RIVALOPTION` | ライバルの配置種別だけ適用し、seedは新規抽選 |
| `RIVALCHART` | ライバルの配置種別と`play_seed`を適用 |

structured配置をlegacy `play_option`より優先するため、BMZから送信された
F-RANDOM / MF-RANDOMも再現できます。ghostによる進行ターゲットは実装対象外です。

```text
SP: play_seed = p1_seed
DP: play_seed = p1_seed + p2_seed * 2^24
0 <= p1_seed, p2_seed < 2^24
```

seedは実際にプレイへ適用され、replayへ保存された値から作ります。設定画面の一時値や
再生成した乱数を送ってはなりません。BMS `#RANDOM`の分岐選択列はarrange seedと
別概念であり、`play_seed`へ混ぜません。

## Score Field Mapping Notes

変換処理はrianIR provider内の純粋関数として実装し、BMZのscore modelやDBに
rianIRの数値enumを持ち込みません。

特に判定名は次のように変換します。

| BMZ | rianIR |
| --- | --- |
| `PGreat` | `pgreat` |
| `Great` | `great` |
| `Good` | `good` |
| `Bad` | `bad` |
| `Poor` | `poor` |
| `EmptyPoor` | `miss` |

rianIR/beatorajaの`miss`は見逃しではなくEmpty Poorに相当します。BMZの`Poor`と
`EmptyPoor`を合算してはなりません。FAST/SLOWは各判定の合計値へ畳み込みます。

`date`はunix秒で送ります。曲長はプレイ時の変換後譜面から再計算せず、スキャン時に
`library.db.charts.length_ms`へ保存した値を使います。legacy互換の`length`は秒、
`length_ms`はミリ秒で同じ曲長を送ります。実プレイ時間はアプリの描画時刻ではなく
audio output callbackのフレーム数から、譜面時刻0（READY用の負の開始マージンは除外）
から最初の`Finished` / `Failed`までを計測し、`play_duration`は秒、
`play_duration_ms`はミリ秒で送ります。

BMS `#RANDOM` / `#SETRANDOM`を含む譜面は`has_random=true`を送り、曲長を0には
しません。rianIR側はこのフラグがtrueのときだけ曲長と実プレイ時間の速度検査を
スキップします。`Failed`も途中落ちを正常に記録できるよう速度検査の対象外とします。
非RANDOMのclear scoreはserverと同じ許容幅（短い側`0.85 * length - 5秒`、長い側
`1.15 * length + 15秒`）をBMZでもqueue投入前に検査し、恒久的に400になるjobを
作りません。

duration-aware対応前に保存済みのqueue jobには`chart.length_ms`が無いため、再送時は
`length=0`の旧wire形式を維持し、当時の譜面終了時刻を実プレイ時間として
`play_duration_ms`へ読み替えません。

曲名などの文字列metadataは`B64:<base64>`形式を使用し、UTF-8とJSON escapingの
差で署名やrequestが壊れないようにします。

## Course Submission

course送信にも単曲と同じclient/rule/LN/arrangeの送信可否判定を適用します。

- courseが完走または途中FAILEDで終端Resultに到達したattemptを送ります。
- course全体を単一のForce LN modeで表現できる場合だけ送ります。
- Battle / Battle ASを使用したattemptは送りません。
- 初期版は誤った曲metadataを登録しないため`tracks=[]`とします。BMZの既存course
  queueはstage SHA-256を保持するが、曲名・artist・levelまでは保持しないためです。
  正確なstage metadata送信はqueue拡張後の後続課題とします。
- rianIRへ送る`course_sha256`とランキング取得には次の
  `rian_course_hash_v1`を使います。

```text
rian_course_hash_v1 =
    SHA256(UTF-8(decoded_course_title + ordered_stage_sha256_hex_strings))
```

BMZ内部・BMZ公式IR用のcanonical course hashは変更しません。course identityには両方を
保持し、provider境界で使い分けます。
- course constraint、gauge、total notes、判定合計がserverの期待値と一致することを
  送信前に検証します。

course endpointの`client_hash`検証方針は単曲endpointと統一します。

## `client_hash` Manifest

release buildはbeatorajaのJAR hashと同じ考え方で、実行ファイルそのものの
SHA-256（小文字64桁hex）を`client_hash`として送ります。archive、installer、app bundle
全体のhashではありません。

Windows/macOS/Flatpak/Linux tarの各release buildは最終実行ファイルから内部用manifestを生成します。
最終release jobは全targetのmanifestを検証・集約し、GitHub Releaseには統合した
`release.json`を含めます。従来の`client-manifest-bmz-player-vX.Y.Z.json`は添付しません。
アプリ自身はmanifestを読まず、実行ファイルから直接`client_hash`を計算します。
内部用のtarget別manifestはビルド間の受け渡しに引き続き利用します。
以下は統合JSONのrianIR用フィールドの抜粋で、全体には配布物一覧と署名も含みます。
完全な形式は[Releaseメタデータ](release-metadata.md)を参照してください。

```json
{
  "schema": "bmz-release-manifest-v1",
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
targetをrianIRのplatform、arch、package_kindへ変換します。rianIR管理者はmanifestの
version、commit、各buildを確認してから`builds[].client_hash`を`allowed_clients`
へ登録します。失効時は同テーブルから削除します。

確認したrianIRの`ClientManifestImportService`はトップレベルの`client` / `version` /
`builds`を読み、追加フィールドとschema識別子を検査しないため、統合JSONを取り込めます。
署名検証は同インポーターでは行いません。署名を検証して登録する運用にはrianIR側の対応が必要です。

Linux tarは`linux-x64-tar`を`platform=linux, arch=x86_64, package_kind=tar`に変換します。
ハッシュは`patchelf`後のアーカイブ内`bin/bmz-player`の検証済み値を使います。
初回公開前にrianIR管理者と`package_kind=tar`の受入を確認し、当該hashを登録します。
Releaseへのmanifest添付だけではallowlist登録は行われません。

ローカル開発でdebug/release profileを繰り返しbuildするときは、コンパイル時の
`BMZ_RIANIR_DEV_CLIENT_HASH`に個人用の小文字64桁hexを設定できます。設定値は
`option_env!`で実行ファイルへ埋め込み、build profileにかかわらず優先します。実行時の
環境変数ではないため、配布済みの公式実行ファイルを後から差し替える用途には使えません。
公式release workflowはこの環境変数が設定されている場合、package build前に失敗します。

PowerShellではhashを設定したshellから通常のrelease profileを起動します。

```powershell
$env:BMZ_RIANIR_DEV_CLIENT_HASH = "<個人用の小文字64桁hex>"
cargo run -r -p bmz-player
```

macOS / Linuxでは同じbuild invocationへ環境変数を渡します。

```bash
BMZ_RIANIR_DEV_CLIENT_HASH="<個人用の小文字64桁hex>" cargo run -r -p bmz-player
```

開発用固定値が未設定のdebug build、固定値の形式が不正なbuild、実行ファイルhashの
取得に失敗したrelease buildだけ、暫定互換値`UNKNOWN`を送ります。固定値が未設定の
release buildは実行ファイルのSHA-256を送ります。

Flatpakは配布`.flatpak`全体ではなく、`flatpak-builder`が最終配置した
`build/files/bin/bmz-player`（実行時の`/app/bin/bmz-player`）をhash対象にします。

## Error and Queue Policy

- 初期版は401/403/429を含め、既存queueのbackoffを使います。
- 401/403をcredentialまたはpayloadのterminal errorとして分類し、自動retryを止める
  改善は後続課題とします。
- 429の`Retry-After`対応も後続課題とします。
- timeout、接続失敗、5xxはretry可能とします。
- payload validation errorとunsupported playはretryしません。
- 単曲送信に成功してランキング取得だけ失敗した場合、score jobを失敗へ戻しません。
- 同じローカルscore historyを複数回送らないよう、provider単位のsubmission ledger
  またはidempotency keyを持ちます。
- rianIRがplayer/chart/date単位で重複判定する可能性があるため、dateの再生成や
  秒単位への丸め直しをretryごとに行いません。

## Known Constraints

- `body`は既存の判定・ゲージ仕様区分として維持し、BMZ識別には使いません。
- rianIRの現在の通常スコア送信はallowlist済みの実行ファイル`client_hash`を要求します。
  開発用固定値が未設定のdebug buildは暫定的に`UNKNOWN`を使うため、正式な
  クライアント検証にはなりません。
- LN modeを含めないランキングquery/groupingではForce LN/CN/HCNが混在します。
- 4K / 6K / 8Kは保存できても、rianIRの統計や画面から漏れる可能性があります。
- F-RANDOM / MF-RANDOMはstructured arrange field対応版のrianIRが必要です。
- 同じchart SHA-256のmetadataを別clientが異なるmode/LN判定で投稿した場合の
  canonical metadata ownershipをrianIR側で決める必要があります。
- rianIRのAPI errorはJSON:API形式と旧形式が混在する可能性があるため、
  HTTP statusを主、response bodyを補助情報として扱います。
- rianIRのclient version補正はBMZ 1.xを誤変換しないようclient別に限定する必要があります。
- 現行beatoraja/rianIR connectorはcourse送信時に`score.sha256`（stage SHA-256の
  単純連結）を優先する一方、取得時は`rian_course_hash_v1`を使います。connector側で
  送信・取得・URL生成を同じhelperへ統一するまで、beatorajaからの新規course scoreと
  BMZの横断ランキングは成立しません。
- `rian_course_hash_v1`はconstraintsを含まないため、同じタイトル・同じ曲順で
  constraintsだけが異なるcourseは現行rianIRと同様に同じランキングへ混在します。
- courseの`ln_mode`は現行connectorで複合値になり得ます。BMZ初期版はForce LN/CN/HCN
  の実効値1/2/3を送るが、既存beatoraja scoreとの分類互換は後続課題とします。
- 初期実装ではserver-side replay verificationが無く、送信内容の正当性を完全には
  証明できません。

## Minimal rianIR-side Work

初期接続のためのDB schema変更や新endpoint追加は行いません。BMZは現行のlogin、
score、get_score、course_score、get_course_scoreをそのまま利用します。

rianIR側の初期作業は次に限定します。

- `beat-4k` / `beat-6k` / `beat-8k`を検索・表示・統計のmode classifierへ追加します。
- Java connectorに`rian_course_hash_v1` helperを1つ追加し、
  `sendCoursePlayData` / `getCoursePlayData` / `getCourseURL`から共通利用します。
  `sendCoursePlayData`での`score.sha256`優先は廃止します。
- 現行API testのcourse成功response期待値を実Controllerの
  `{"status":"success"}`に合わせます。

次は初期接続を止めず、後続課題とします。

- get_score / get_course_scoreのLN別filter・集約
- constraints込みのcourse hash v2とcourse `ln_mode`の統一
- `client=bmz-player`が1.xになったときのversion prefix補正除外
- manifestへのrelease署名とrianIRへのallowlist登録自動化

## Verification Plan

### BMZ unit tests

- `RuleMode` 3種と`client=bmz-player`のmappingです。
- clear type、gauge、assist、legacy optionの全enum mappingです。
- `Poor -> poor`、`EmptyPoor -> miss`とFAST/SLOW集計です。
- `ln_mode_format=canonical-v1`が常に付きます。
- LNなし、未定義LN、定義LN/CN/HCN、混在譜面について、Auto/Forceの
  `song_ln_mode` / `ln_mode` matrixが規則どおりになります。
- Auto 3種を送信でき、LNなし譜面は設定にかかわらず`0 / 0`になります。
- rianIR向けlocal score backfillが新規投入時と既存queue送信時の双方で拒否されます。
- 4K/6K/8Kを含む全対応key modeの`play_mode` mappingです。
- 1P/2PのF-RANDOM / MF-RANDOM structured fieldです。
- Off/Flipは送信でき、Battle/Battle ASは送信できません。
- SP seed、DP packed seed、24 bit境界値です。
- unix秒、曲長、play durationの単位変換です。
- UTF-8タイトルとstage順を含む`rian_course_hash_v1`のRust/Java共通golden fixtureです。
- login/score/course requestとHMACのgolden fixtureです。
- retry時にdate、signature、seedが変わりません。
- primary providerがrianIRの場合の難易度表・POPULAR・レビュー・Rivals RECENT・
  course取得です。既存の難易度表folderとcourse表示を再利用します。

### rianIR table cache lifecycle

- cache scopeは`provider_key + base_url + account_id`とし、scopeをSHA-256 digest化した
  内部source URLでアカウント間を分離します。ログインIDをDBのsource URLへ直接含めません。
- アプリ起動後の初回描画直後に取得します。前回cacheがあれば取得完了を待たずに表示します。
- 起動中は30分間隔で再取得します。rianIR側のcommon table cacheは10分なので、それより
  短いpollingは行いません。
- rianIR由来table内、またはrootでrianIR tableを選択中のF5は、そのアカウントの
  table一式を即時更新します。5秒以内の連打と同時実行は抑止します。
- 通信失敗時は既存cacheを残します（stale-while-revalidate）。
- primary provider、base URL、account IDの変更とlogoutでは旧scopeのtable/course
  cacheを削除して選曲から即時非表示にします。変更前に開始した通信結果はgenerationで
  破棄し、旧アカウントのcacheを復活させません。
- rianIR側の変更は不要で、既存`get_tables.php` responseをそのまま利用します。

### rianIR integration tests

- `client=bmz-player`、version、3種のruleを保存・取得できます。
- `body`が既存のrule bucketとして保存・取得に使われます。
- `LR2orajaED` aliasと`LR2oraja-EndressDream` canonical valueです。
- BMZ 1.x versionが別client用prefixへ書き換えられません。
- 現行APIではForce LN/CN/HCNが混在することをfixtureで固定し、LN filter追加後に
  分離されます。
- `beat-4k` / `beat-6k` / `beat-8k`がランキング・検索・統計へ現れます。
- F-RANDOM / MF-RANDOMが1P/2P別に保存・表示されます。
- `UNKNOWN` client hashの暫定許可と未検証表示です。
- 不正token、signature、client hash、enum、範囲外scoreの拒否です。
- rate limit responseと`Retry-After`です。
- 単曲とcourseでclient hash・rule・LN validationが一致します。

### Manual end-to-end

1. rianIRのテストアカウントでBMZからloginします。
2. 5K/7K/14Kと4K/6K/8Kを各1曲プレイし、rule・mode・判定内訳をDBと画面で確認します。
3. 同じ譜面をForce LN/CN/HCNでプレイし、現行APIで混在する制約を確認します。
4. F-RANDOM / MF-RANDOMのSP/DP scoreでarrange、seed、replay再現結果を照合します。
5. Battle / Battle ASはローカル保存のみ、実効アシスト（SPIRAL / H-RANDOM /
   ALL-SCR / RANDOM-EX / S-RANDOM-EXを含む）はassist lampと統計だけを更新し、
   数値score・履歴・replay・IRを保存しないことを確認します。autoplayも送信されないことを確認します。
6. courseの完走と途中FAILEDを各1回行い、course scoreを確認します。初期版では
   `tracks=[]`であることも確認します。
7. 通信切断、401、403、429、5xxの各ケースでqueueとUI表示を確認します。

## Implementation Pointers

BMZ側:

- IR provider境界: `crates/bmz-player/src/ir/`
- 共通score payload: `crates/bmz-player/src/ir/payload.rs`
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
