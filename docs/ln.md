# LN / CN / HCN policy

BMZはbeatoraja完全互換ではなく、譜面が宣言したLNMODEとユーザーの希望するLNMODEを分けて扱います。
ユーザーはprofileの設定で、譜面のLN宣言を尊重するか、特定のLN種別へ強制するかを選べます。

## Terms

- undefined LN: BMS側に`#LNMODE`などの明示的なLN種別宣言が無いlong noteです。現在のBMSで主流の形です。
- defined LN/CN/HCN: BMS / BMSON側が明示した`LN`, `CN`, `HCN`のlong noteです。
- profile policy: ユーザーが`profile.toml`に設定する希望です。
- score policy: score DBの保存キーに使う、実プレイ結果の区別単位です。
- effective LN mode: 実際に降らせるLN種別です。`LN`, `CN`, `HCN`のいずれかです。

## Profile Setting

`profile.toml`の`[play]`に`ln_mode_policy`を保存します。

設定値:

- `auto_ln`
- `auto_cn`
- `auto_hcn`
- `force_ln`
- `force_cn`
- `force_hcn`

`auto_*`は譜面の明示LN種別を尊重し、未定義LNだけをユーザー設定の種別として解釈します。
`force_*`は譜面の明示LN種別に関係なく、指定した種別としてプレイします。

## Library DB

BMSフォルダスキャン時に、各譜面のlong note構成を`library.db`に保存します。

`charts`に保存するflags:

- `has_undefined_ln`
- `has_defined_ln`
- `has_defined_cn`
- `has_defined_hcn`

BMZ内部では`ChartLnProfile`として扱います。

`#LNMODE`が明示されていないBMSのlong noteはundefined LNとします。
`#LNMODE`が明示されているBMSのlong noteは、そのモードのdefined LN/CN/HCNとします。

## Score Policy

score DBには`LnScorePolicy`を保存します。

保存値:

- `AutoLn`
- `AutoCn`
- `AutoHcn`
- `ForceLn`
- `ForceCn`
- `ForceHcn`

score policyはprofile policyとchart LN profileから決めます。

| Chart LN profile | `auto_ln` | `auto_cn` | `auto_hcn` | `force_ln` | `force_cn` | `force_hcn` |
| --- | --- | --- | --- | --- | --- | --- |
| LNなし | `ForceLn` | `ForceLn` | `ForceLn` | `ForceLn` | `ForceLn` | `ForceLn` |
| undefined LNのみ | `ForceLn` | `ForceCn` | `ForceHcn` | `ForceLn` | `ForceCn` | `ForceHcn` |
| defined LNのみ | `ForceLn` | `ForceLn` | `ForceLn` | `ForceLn` | `ForceCn` | `ForceHcn` |
| defined CNのみ | `ForceCn` | `ForceCn` | `ForceCn` | `ForceLn` | `ForceCn` | `ForceHcn` |
| defined HCNのみ | `ForceHcn` | `ForceHcn` | `ForceHcn` | `ForceLn` | `ForceCn` | `ForceHcn` |
| defined LN/CN/HCN混在 | `AutoLn` | `AutoLn` | `AutoLn` | `ForceLn` | `ForceCn` | `ForceHcn` |
| undefined + defined混在 | `AutoLn` | `AutoCn` | `AutoHcn` | `ForceLn` | `ForceCn` | `ForceHcn` |

補足:

- LNが無い譜面はLN種別でスコアを分ける意味が無いため、常に`ForceLn`に正規化します。
- undefined LNのみの譜面では、実際にはユーザー設定の種別しか降らないため`auto_*`も`force_*`に寄せて保存します。
- defined単一種別のみの譜面では、`auto_*`は譜面定義に寄せて`Force*`として保存します。
- defined種別が混在する譜面では、beatoraja互換の固定挙動に寄せず、譜面の混在を表す`AutoLn`として保存します。
- undefinedとdefinedが混在する譜面では、undefined部分の解釈がprofileに依存するため`AutoLn` / `AutoCn` / `AutoHcn`を区別して保存します。

## Effective LN Mode

score policyからundefined LNのfallback種別を決めます。

- `AutoLn` / `ForceLn` -> `LN`
- `AutoCn` / `ForceCn` -> `CN`
- `AutoHcn` / `ForceHcn` -> `HCN`

defined種別が混在する譜面ではscore policyは`AutoLn`になります。
実プレイではnoteごとのdefined種別を尊重し、undefined部分だけfallback種別で解釈します。

`force_*`の場合はdefined / undefinedに関係なく全long noteを指定種別として扱います。

## Runtime Judgement

始点 / 終点の扱いはbeatorajaの`JudgeManager`を基準にします。ここでいうLN / CN / HCNは、
profile policyを適用した後のeffective LN modeを指します。

- LNは始点押下時の判定を保持し、終点まで維持できた時点で1ノーツ分として確定します。
  始点と終点を別々のスコア対象にはしません。
- CN / HCNは始点と終点を別々のスコア対象として扱います。始点は押下時、終点は離した時に
  それぞれ判定します。
- CN / HCNの始点を見逃した場合、beatorajaと同じく始点と対応する終点の両方を
  その時点でPOORにします。終点時刻まで追加POORを遅延させません。
- CN / HCNの終点を離して判定する場合は通常ノート窓ではなくlong-note end窓を使います。
- 方向付きスクラッチCN / HCN (BSS)は、始点と同じ方向のReleaseがlong-note end窓内なら
  そのReleaseで終点を判定します。beatorajaはこのReleaseを無視して逆方向Pressを待つが、
  BMZはスクラッチを止める操作だけでも終点を取れるよう、ここを意図的に変更しています。
- 上記Releaseの直後30ms以内（30msちょうどを含む）に来た最初の逆方向Pressは1回だけ
  消費し、通常ノート、Mine、フォールバックキー音へ流しません。別方向のPressまたは30ms超過後の
  Pressは通常どおり処理します。Releaseを伴わず逆方向Pressで終点を取る場合は、終点だけを判定し、
  同じ入力を直後の通常ノートへ流しません。
- CN / HCNを離さず終点を通過した場合の見逃し確定時刻は、long-note endの
  BAD終端ではなく通常ノートのlate BAD終端を使います。
- 9Kのbeatoraja PMSとDX POPには200msのrelease marginがあります。BAD / POOR相当の
  早離しは直ちに確定せず、この猶予内に押し直すと取り消します。猶予を過ぎると保留判定を確定します。
  それ以外の通常レーンとlong scratchのmarginは0です。
- HCNの`passing`参照自体は現在時刻が始点を通過すると設定されるが、始点stateが未判定の
  `0`の間はactive/damageタイマーとゲージ増減を動かしません。始点が判定済みになった後、
  HCN区間中は200msごとに、押下中なら回復、非押下なら減少を適用します。

参照箇所:

- beatoraja: `.local/beatoraja/src/bms/player/beatoraja/play/JudgeManager.java`
- beatoraja判定窓: `.local/beatoraja/src/bms/player/beatoraja/play/JudgeProperty.java`
- LR2oraja Endless Dream DX: `.local/lr2oraja-endlessdream/core/src/bms/player/beatoraja/play/JudgeManager.java`
- DX判定窓: `.local/lr2oraja-endlessdream/core/src/bms/player/beatoraja/play/JudgeProperty.java`

## Score DB

score DBはprofileごとに分かれています。
LN policyはprofile内のスコア・リプレイをさらに分けるキーとして扱います。

保存先:

- `score_history.ln_policy`
- `score_best.ln_policy`
- `replay_slots.ln_policy`
- `course_scores.ln_policy`
- `course_replay_slots.ln_policy`

主キー:

- `score_best`: `(chart_sha256, ln_policy)`
- `replay_slots`: `(chart_sha256, ln_policy, slot)`
- `course_scores`の検索キー: `(course_hash, ln_policy, rule_mode)`
- `course_replay_slots`: `(course_hash, ln_policy, rule_mode, slot)`

単曲の`ln_policy`は譜面ごとに正規化した`LnScorePolicy`を保存します。コースも
全譜面の`ChartLnProfile` flagsを論理和で集約し、コースLN constraintによる
undefined LN fallbackを反映してから、同じ`score_ln_policy`規則で正規化した
`LnScorePolicy`を保存します。したがって、全譜面にLNが無いコースは設定にかかわらず
`ForceLn`、単一のdefined種別だけを含むコースのAUTOはその`Force*`、defined
種別が混在するコースのAUTOは`AutoLn`になります。undefinedとdefinedが混在する
場合だけfallbackに応じた`AutoLn` / `AutoCn` / `AutoHcn`を区別します。
既存のコーススコアは設定値を一意に復元できないため、migrationでは
`ForceLn` bucketへ移します。

通常リプレイファイルは`score_history`が個別pathを持つため、ファイル名衝突は起きにくいです。
replay slotは同じchart SHA256 / slot番号を複数policyで使うため、新規保存時のslot replayファイル名に`ln_policy`を含めます。

## External score DB import (LR2 / beatoraja / LR2oraja)

スコアインポートは「BMZ メニュー → データ移行」の「スコアインポート」から行います。

### 経路

| 種別 | ソースDB | 照合キー | 保存`rule_mode` | `ln_policy` |
| --- | --- | --- | --- | --- |
| LR2 | LR2 `score` | MD5 | `Lr2Oraja` | 常に`ForceLn` |
| beatoraja | `score` / `scoredatalog` | SHA256 | `Beatoraja` | `score.mode` + 正規化 |
| LR2oraja | beatoraja形式 | SHA256 | `Lr2Oraja` | 同上 |
| LR2oraja (DX) | beatoraja形式 | SHA256 | `Dx` | 同上 |

### beatoraja / LR2oraja の `score.mode`

beatoraja系の`mode`はkey modeではなくLNMODEです。

- `0` = LN, `1` = CN, `2` = HCN
- undefined LNがある譜面だけ`mode`が分かれます。無い譜面は常に`0`

インポート時は`mode`を`AutoLn` / `AutoCn` / `AutoHcn`にマップし、libraryの`ChartLnProfile`と合わせて`score_ln_policy`で正規化します。

### ノーツ数チェック

外部DBのノーツ数と、**正規化後の`ln_policy`における期待スコア対象ノーツ数**を比較します。

- beatoraja系は`score.notes`、純LR2は`score.totalnotes`を外部DBのノーツ数として使う
- 期待値 = `Tap + LongStart`（library / chart `total_notes`）+ effective CN/HCNのlong pair数
- library `total_notes`自体はln_policy非考慮のため、生値だけでは比較しない
- 現行の解析バージョンで保存した`ChartLnCounts`と`scored_total_notes()`を使い、件数確認のためにBMSを再読込しません。譜面ファイルが移動・消失していても検証できる
- 同一ハッシュの古い解析情報より新しい情報を優先します。最新の情報が無い場合は再スキャンが必要な理由を表示する
- 不一致かつpolicyが`ForceLn`以外なら`ForceLn`で期待値を再計算して再比較（ビルド差フォールバック）
- それでも不一致ならその行は`failed`としてスキップする
- あわせて`EX score <= 外部ノーツ数 * 2`、`max combo <= 外部ノーツ数`を検証する

判定合計はノーツ数チェックに使いません。beatoraja系は途中FAILEDで未処理ノーツが判定に含まれず、非消滅判定では同じノーツに複数判定が付く場合があります。純LR2の`poor`にはEmpty Poorも含まれるため、いずれも判定合計とノーツ数の一致は保証されません。

結果には未所持、重複、ノーツ数不一致、読み込み失敗、譜面情報の更新が必要、不正なスコア、未対応を分けて件数と代表理由を表示します。
ノーツ数不一致では元件数・選ばれたLN policyの期待値・各FORCE設定の期待値を表示します。
未定義LN譜面で`mode=0`にCN相当の`notes`が保存されていても、CNとして取り込む補正はしません。
再スキャンはlibraryの解析情報を更新する操作で、読み取り専用で開く外部score DBの`mode` / `notes`は修正しません。

純LR2はCN/HCNが無いため常に`ForceLn`とし、`score.totalnotes`と期待ノーツ数を照合します。

BMZのscore DB schema migrationで既存行へ付けた`ForceLn`既定値は、上記インポートとは別件です。

## Implementation Pointers

- policy calculation / expected scored notes: `crates/bmz-player/src/ln_policy.rs`
- external score import: `crates/bmz-player/src/storage/score_import.rs`
- runtime judgement: `crates/bmz-gameplay/src/judge/engine.rs`
- HCN passing / gauge: `crates/bmz-gameplay/src/session.rs`
- profile config: `crates/bmz-player/src/config/profile_config.rs`
- chart LN profile persistence: `crates/bmz-player/src/storage/library_db.rs`
- score DB persistence: `crates/bmz-player/src/storage/score_db.rs`
- play start / result wiring: `crates/bmz-player/src/screens/play_start.rs`, `crates/bmz-player/src/screens/play_finish.rs`
- select screen lookup: `crates/bmz-player/src/screens/select_model.rs`
