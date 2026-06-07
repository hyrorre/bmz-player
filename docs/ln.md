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

## beatoraja score DB migration

beatoraja のスコアには BMZ の profile policy や score policy に相当する情報が無い。
そのため、beatoraja から migrate する score / best / replay slot は `ForceLn` として取り込む。

理由:

- LN が無い譜面は BMZ でも `ForceLn` に正規化する。
- undefined LN のみの譜面は旧来 BMS の主流で、beatoraja 由来のスコアは LN 扱いとして解釈するのが最も自然。
- beatoraja ではユーザーが任意の LNMODE でプレイしたという情報を保存していないため、`ForceCn` / `ForceHcn` / `Auto*` として復元できない。

BMZ の score DB migration でも、既存行は `ForceLn` として移行する。

## Implementation Pointers

- policy calculation: `crates/bmz-player/src/ln_policy.rs`
- profile config: `crates/bmz-player/src/config/profile_config.rs`
- chart LN profile persistence: `crates/bmz-player/src/storage/library_db.rs`
- score DB persistence: `crates/bmz-player/src/storage/score_db.rs`
- play start / result wiring: `crates/bmz-player/src/screens/play_start.rs`, `crates/bmz-player/src/screens/play_finish.rs`
- select screen lookup: `crates/bmz-player/src/screens/select_model.rs`
