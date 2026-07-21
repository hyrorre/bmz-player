# LN / CN / HCN policy

BMZ は beatoraja 完全互換ではなく、譜面が宣言した LNMODE とユーザーの希望する LNMODE を分けて扱う。
ユーザーは profile の設定で、譜面の LN 宣言を尊重するか、特定の LN 種別へ強制するかを選べる。

## Terms

- undefined LN: BMS 側に `#LNMODE` などの明示的な LN 種別宣言が無い long note。現在の BMS で主流の形。
- defined LN/CN/HCN: BMS / BMSON 側が明示した `LN`, `CN`, `HCN` の long note。
- profile policy: ユーザーが `profile.toml` に設定する希望。
- score policy: score DB の保存キーに使う、実プレイ結果の区別単位。
- effective LN mode: 実際に降らせる LN 種別。`LN`, `CN`, `HCN` のいずれか。

## Profile Setting

`profile.toml` の `[play]` に `ln_mode_policy` を保存する。

設定値:

- `auto_ln`
- `auto_cn`
- `auto_hcn`
- `force_ln`
- `force_cn`
- `force_hcn`

`auto_*` は譜面の明示 LN 種別を尊重し、未定義 LN だけをユーザー設定の種別として解釈する。
`force_*` は譜面の明示 LN 種別に関係なく、指定した種別としてプレイする。

## Library DB

BMS フォルダスキャン時に、各譜面の long note 構成を `library.db` に保存する。

`charts` に保存する flags:

- `has_undefined_ln`
- `has_defined_ln`
- `has_defined_cn`
- `has_defined_hcn`

BMZ 内部では `ChartLnProfile` として扱う。

`#LNMODE` が明示されていない BMS の long note は undefined LN とする。
`#LNMODE` が明示されている BMS の long note は、そのモードの defined LN/CN/HCN とする。

## Score Policy

score DB には `LnScorePolicy` を保存する。

保存値:

- `AutoLn`
- `AutoCn`
- `AutoHcn`
- `ForceLn`
- `ForceCn`
- `ForceHcn`

score policy は profile policy と chart LN profile から決める。

| Chart LN profile | `auto_ln` | `auto_cn` | `auto_hcn` | `force_ln` | `force_cn` | `force_hcn` |
| --- | --- | --- | --- | --- | --- | --- |
| LN なし | `ForceLn` | `ForceLn` | `ForceLn` | `ForceLn` | `ForceLn` | `ForceLn` |
| undefined LN のみ | `ForceLn` | `ForceCn` | `ForceHcn` | `ForceLn` | `ForceCn` | `ForceHcn` |
| defined LN のみ | `ForceLn` | `ForceLn` | `ForceLn` | `ForceLn` | `ForceCn` | `ForceHcn` |
| defined CN のみ | `ForceCn` | `ForceCn` | `ForceCn` | `ForceLn` | `ForceCn` | `ForceHcn` |
| defined HCN のみ | `ForceHcn` | `ForceHcn` | `ForceHcn` | `ForceLn` | `ForceCn` | `ForceHcn` |
| defined LN/CN/HCN 混在 | `AutoLn` | `AutoLn` | `AutoLn` | `ForceLn` | `ForceCn` | `ForceHcn` |
| undefined + defined 混在 | `AutoLn` | `AutoCn` | `AutoHcn` | `ForceLn` | `ForceCn` | `ForceHcn` |

補足:

- LN が無い譜面は LN 種別でスコアを分ける意味が無いため、常に `ForceLn` に正規化する。
- undefined LN のみの譜面では、実際にはユーザー設定の種別しか降らないため `auto_*` も `force_*` に寄せて保存する。
- defined 単一種別のみの譜面では、`auto_*` は譜面定義に寄せて `Force*` として保存する。
- defined 種別が混在する譜面では、beatoraja 互換の固定挙動に寄せず、譜面の混在を表す `AutoLn` として保存する。
- undefined と defined が混在する譜面では、undefined 部分の解釈が profile に依存するため `AutoLn` / `AutoCn` / `AutoHcn` を区別して保存する。

## Effective LN Mode

score policy から undefined LN の fallback 種別を決める。

- `AutoLn` / `ForceLn` -> `LN`
- `AutoCn` / `ForceCn` -> `CN`
- `AutoHcn` / `ForceHcn` -> `HCN`

defined 種別が混在する譜面では score policy は `AutoLn` になる。
実プレイでは note ごとの defined 種別を尊重し、undefined 部分だけ fallback 種別で解釈する。

`force_*` の場合は defined / undefined に関係なく全 long note を指定種別として扱う。

## Runtime Judgement

始点 / 終点の扱いは beatoraja の `JudgeManager` を基準にする。ここでいう LN / CN / HCN は、
profile policy を適用した後の effective LN mode を指す。

- LN は始点押下時の判定を保持し、終点まで維持できた時点で 1 ノーツ分として確定する。
  始点と終点を別々のスコア対象にはしない。
- CN / HCN は始点と終点を別々のスコア対象として扱う。始点は押下時、終点は離した時に
  それぞれ判定する。
- CN / HCN の始点を見逃した場合、beatoraja と同じく始点と対応する終点の両方を
  その時点で POOR にする。終点時刻まで追加 POOR を遅延させない。
- CN / HCN の終点を離して判定する場合は通常ノート窓ではなく long-note end 窓を使う。
- CN / HCN を離さず終点を通過した場合の見逃し確定時刻は、long-note end の
  BAD 終端ではなく通常ノートの late BAD 終端を使う。
- 9K の beatoraja PMS と DX POP には 200ms の release margin がある。BAD / POOR 相当の
  早離しは直ちに確定せず、この猶予内に押し直すと取り消す。猶予を過ぎると保留判定を確定する。
  それ以外の通常レーンと long scratch の margin は 0。
- HCN の `passing` 参照自体は現在時刻が始点を通過すると設定されるが、始点 state が未判定の
  `0` の間は active/damage タイマーとゲージ増減を動かさない。始点が判定済みになった後、
  HCN 区間中は 200ms ごとに、押下中なら回復、非押下なら減少を適用する。

参照箇所:

- beatoraja: `.local/beatoraja/src/bms/player/beatoraja/play/JudgeManager.java`
- beatoraja 判定窓: `.local/beatoraja/src/bms/player/beatoraja/play/JudgeProperty.java`
- LR2oraja Endless Dream DX: `.local/lr2oraja-endlessdream/core/src/bms/player/beatoraja/play/JudgeManager.java`
- DX 判定窓: `.local/lr2oraja-endlessdream/core/src/bms/player/beatoraja/play/JudgeProperty.java`

## Score DB

score DB は profile ごとに分かれている。
LN policy は profile 内のスコア・リプレイをさらに分けるキーとして扱う。

保存先:

- `score_history.ln_policy`
- `score_best.ln_policy`
- `replay_slots.ln_policy`

主キー:

- `score_best`: `(chart_sha256, ln_policy)`
- `replay_slots`: `(chart_sha256, ln_policy, slot)`

通常リプレイファイルは `score_history` が個別 path を持つため、ファイル名衝突は起きにくい。
replay slot は同じ chart SHA256 / slot 番号を複数 policy で使うため、新規保存時の slot replay ファイル名に `ln_policy` を含める。

## External score DB import (LR2 / beatoraja / LR2oraja)

スコアインポートは egui 本体設定の「スコアインポート」から行う。

### 経路

| 種別 | ソース DB | 照合キー | 保存 `rule_mode` | `ln_policy` |
| --- | --- | --- | --- | --- |
| LR2 | LR2 `score` | MD5 | `Lr2Oraja` | 常に `ForceLn` |
| beatoraja | `score` / `scoredatalog` | SHA256 | `Beatoraja` | `score.mode` + 正規化 |
| LR2oraja | beatoraja 形式 | SHA256 | `Lr2Oraja` | 同上 |
| LR2oraja (DX) | beatoraja 形式 | SHA256 | `Dx` | 同上 |

### beatoraja / LR2oraja の `score.mode`

beatoraja 系の `mode` は key mode ではなく LNMODE である。

- `0` = LN, `1` = CN, `2` = HCN
- undefined LN がある譜面だけ `mode` が分かれる。無い譜面は常に `0`

インポート時は `mode` を `AutoLn` / `AutoCn` / `AutoHcn` にマップし、library の `ChartLnProfile` と合わせて `score_ln_policy` で正規化する。

### ノーツ数チェック

外部 DB のノーツ数と、**正規化後の `ln_policy` における期待スコア対象ノーツ数**を比較する。

- beatoraja 系は `score.notes`、純 LR2 は `score.totalnotes` を外部 DB のノーツ数として使う
- 期待値 = `Tap + LongStart`（library / chart `total_notes`）+ effective CN/HCN の long pair 数
- library `total_notes` 自体は ln_policy 非考慮のため、生値だけでは比較しない
- 不一致かつ policy が `ForceLn` 以外なら `ForceLn` で期待値を再計算して再比較（ビルド差フォールバック）
- それでも不一致ならその行は `failed` としてスキップする
- あわせて `EX score <= 外部ノーツ数 * 2`、`max combo <= 外部ノーツ数` を検証する

判定合計はノーツ数チェックに使わない。beatoraja 系は途中 FAILED で未処理ノーツが判定に含まれず、非消滅判定では同じノーツに複数判定が付く場合がある。純 LR2 の `poor` には Empty Poor も含まれるため、いずれも判定合計とノーツ数の一致は保証されない。

純 LR2 は CN/HCN が無いため常に `ForceLn` とし、`score.totalnotes` と期待ノーツ数を照合する。

BMZ の score DB schema migration で既存行へ付けた `ForceLn` 既定値は、上記インポートとは別件である。

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
