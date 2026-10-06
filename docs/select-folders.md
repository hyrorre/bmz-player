# 仮想フォルダ

BMZ の選曲ルートには、beatoraja の標準フォルダに相当する仮想フォルダを表示します。
組み込み定義は `crates/bmz-player/resources/select-folders.toml` にあり、次を含みます。

- LAMP UPDATE / SCORE UPDATE（直近30日）
- MY BEST
- CLEAR TYPE / SCORE RANK
- DENSITY（AVERAGE / PEAK / END）
- FEATURE（皿率 / LN率）
- LEVEL（5K / 7K / 9K / 10K / 14K）
- NEW（当日から7日前、および直近7日）

FAVORITE は既存のコレクション機能を使います。INVISIBLE は実装しません。
同一曲フォルダは選択中の曲に対する既存の same-folder 操作を使います。

曲root内の ZIP / RAR / 7z もスキャン対象です。書庫内の BMS / BME / BML / PMS / BMSON を
通常の曲と同様に選択でき、手動展開は不要です。書庫内のフォルダは `書庫名.zip!/曲フォルダ`
として扱い、再生・プレビュー・曲画像が必要になった時点でcacheへ展開します。
rootの再帰設定は元の書庫ファイルの位置に適用し、書庫内部の階層は含めて列挙します。
保存する譜面識別子、重複コピー、cacheと再スキャンの契約は [曲の取得元とスキャン](song-sources.md) を参照してください。

## 空フォルダと一覧更新

通常フォルダ・仮想フォルダ・検索履歴・同一曲フォルダは、表示項目がなければ開かず、
現在の一覧とカーソルを維持して通知します。空のフォルダ自体は親の一覧に残ります。
判定は既存のキーモード自動切り替えとフィルター適用後に行い、子フォルダ、未所持の
難易度表譜面、コース作成等の操作項目も表示項目として扱います。

スキャンやお気に入り変更等の一覧更新で現在のフォルダが空になった場合は、表示項目が
ある最寄りの親へ戻ります。親の一覧では元のフォルダを選択し、それが消えていた場合は
保存したカーソル位置を一覧の範囲内に補正します。読み込み失敗は空と区別し、現在の
表示を維持してエラーを通知します。

空の一覧で移動・決定を繰り返しても再読み込みは行いません。一覧はフォルダ移動、
更新操作、スキャン完了などの契機で読み直します。

## プロファイル別定義

`data/profiles/<profile>/select-folders.toml` を置くと、組み込み定義を上書きできます。
同じトップレベル `id` は置換され、`enabled = false` なら非表示になります。
新しい `id` は組み込みフォルダの後ろに追加されます。

最小の1行条件は次の形式です。

```toml
version = 1

[[folders]]
id = "level-12"
name = "LEVEL 12"
query = "mode == '7K' && level == 12"
```

階層は `items` で定義します。

```toml
version = 1

[[folders]]
id = "practice"
name = "PRACTICE"
items = [
  { id = "unplayed", name = "UNPLAYED", query = "play_count == 0" },
  { id = "failed", name = "FAILED", query = "clear == 1" },
]
```

抽出後に順位を付けて件数を制限する場合、`query` をテーブルにします。

```toml
[[folders]]
id = "most-played"
name = "MOST PLAYED"

[folders.query]
filter = "play_count > 0"
order_by = "play_count desc"
limit = 20
```

連番フォルダは生成できます。`{value}`、`{ordinal}`（value + 1）、
`{days_ago}`（0ならTODAY、それ以外は「N DAYS AGO」）を置換します。

```toml
[[folders]]
id = "recent"
name = "RECENT"

[folders.generate]
values = "0..=6"
id = "day-{value}"
name = "{days_ago}"
query = "added_at in local_day({value})"
```

`items` と併用する場合は `insert_at = 0` のように生成行の挿入位置も指定できます。
省略時は既存 `items` の後ろへ追加します。

数値範囲を多数作る場合は `buckets` を使えます。各区間は下限を含み、上限を含みません。

```toml
[[folders]]
id = "density"
name = "DENSITY"

[folders.buckets]
field = "density"
prefix = "DENSITY"
cuts = [3, 5, 7, 9, 10]
```

## 1行クエリ

クエリはSQLではなく、選曲メタデータだけを参照できる型付きDSLです。
`&&`、`||`、`!`、括弧、`==`、`!=`、`<`、`<=`、`>`、`>=`、
`in [値, ...]` を使えます。

| フィールド | 値 |
|---|---|
| `mode` | `"5K"`、`"7K"`、`"9K"`、`"10K"`、`"14K"` など |
| `level` | BMSのプレイレベル |
| `density` | 平均密度 |
| `peak_density` | 最大密度 |
| `end_density` | 終盤密度 |
| `scratch_rate` | 全ノーツに対する皿ノーツの比率（0.0〜1.0） |
| `long_note_rate` | 全ノーツに対するLNの比率（0.0〜1.0） |
| `clear` | 0=未プレイ、1=FAILED、2/3=ASSIST、4=EASY、5=NORMAL、6=HARD、7=EX HARD、8以上=FULL COMBO以上 |
| `score_rate` | EXスコア率（0〜100） |
| `play_count` | プレイ回数 |
| `added_at` | ライブラリへの初回登録日時 |
| `lamp_updated_at` | ランプを更新したローカルプレイ日時 |
| `score_updated_at` | EXスコアを更新したローカルプレイ日時 |

日時にはOSのローカル日付を使います。

```text
added_at in local_day(0)          # 今日
lamp_updated_at in local_day(3)   # 4日前
added_at in local_days(7)         # 今日を含む直近7暦日
```

未知のフィールド、壊れた式、重複した兄弟 `id` は定義ロード時にエラーになります。
