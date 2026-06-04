# AGENTS.md

このメモは、Codex でこのリポジトリを継続開発するときの作業入口です。実装前に構成と約束事を確認し、変更後はここにある検証コマンドを実行してください。

## Project Goal

BMZ Player は LunaticRave2 / beatoraja の後継を目指す BMS プレイヤーです。
技術方針は Rust + wgpu、アプリ内 UI は egui、音声/動画は cpal + ffmpeg-next、BMS パースは `bms-rs` を利用。
対象OSは Windows / macOS / Linux です。

最初のゲームモードは 5K / 7K / 10K / 14K 通常プレイとオートプレイです。

beatoraja の参照ソースは `.local/beatoraja/`、beatoraja 対応スキン例は `data/skins/` に置かれます。外部スキンは gitignore 管理で、コミット対象にしません。

## Repository Layout

- `crates/bmz-core`
  - 共通プリミティブです。
  - `Lane`, `Judge`, `TimingSide`, `TimeUs`, `ChartTick`, replay/input/clear type など。

- `crates/bmz-chart`
  - BMS import/normalize pipeline です。
  - hash は BMS ファイル全体の MD5 と SHA256 を扱います。
  - `import/intermediate.rs`, `import/normalize.rs`, `timing.rs`, `model.rs` を中心に確認します。

- `crates/bmz-gameplay`
  - 判定、スコア、ゲージ、セッション、オートプレイ、入力変換です。
  - beatoraja 準拠のスコア/ゲージ仕様を目指します。

- `crates/bmz-audio`
  - cpal backend、mixer、sample loader、audio clock です。
  - 音声ファイル decode は `ffmpeg-next` を使い、ffmpeg 初期化は `bmz-ffmpeg` に寄せます。

- `crates/bmz-ffmpeg`
  - `ffmpeg-next` のプロセス単位初期化を共通化する薄い crate です。
  - `bmz-audio` と `bmz-video` が共有します。
  - ffmpeg 型や decode ロジックは利用側 crate が直接持ち、本 crate は `ensure_init()` とログレベル調整だけを担当します。

- `crates/bmz-font`
  - OS フォントのパス解決を `font-kit` 経由で共通化する薄い crate です。
  - `bmz-render`（デフォルト描画フォント）と `bmz-app`（egui CJK fallback）が共有します。
  - 固定 OS パス列は使わず、`SystemSource` で CSS Fonts Level 3 準拠のマッチングを行います。
  - macOS Core Text は `Handle::Memory` を返すことがあるため、path と memory bytes の両方を扱います。
  - TTC は `font_index` 付きで解決し、ab_glyph への読込は利用側 crate が担当します。
  - スキン内フォント (`font.path`) のワイルドカード解決は `bmz-app/src/skin_loader.rs` の責務で、本 crate では扱いません。
  - 将来 `data/fonts/` 同梱フォントを `FsSource` + `Multi` で先に見る拡張点を `src/system.rs` に残しています。

- `crates/bmz-video`
  - 動画 BGA decode crate です。
  - `ffmpeg-next` で動画フレームを RGBA に変換し、play 側から時刻に応じて poll します。
  - BMS / gameplay / renderer の責務を持ち込まず、動画 decode とフレーム供給に留めます。

- `crates/bmz-render`
  - wgpu renderer、描画 plan、scene snapshot、skin document の描画評価です。
  - `SkinContext`、`SkinRenderItem`、texture id との紐付け、`draw` / `op` / `timer` 評価を持ちます。
  - egui は `crates/bmz-render/src/ui.rs` の `EguiFrame` / `EguiPainter` で wgpu surface に重ねます。
  - `egui::Context` や winit event state は持たず、app から渡された paint primitives を描画するだけにします。

- `crates/bmz-skin`
  - beatoraja JSON skin / Lua skin の document decode crate です。
  - JSON skin loader、Lua sandbox、`skin_config`、Lua table to JSON、`main_state` function 推論を扱います。
  - v1 はロード時だけ Lua を実行し、描画中の毎フレーム Lua 実行はしません。

- `crates/bmz-skin-convert`
  - Lua skin を JSON に変換する薄い CLI です。
  - 本体ロジックは `bmz-skin` を呼び、アプリ内 Lua load と同じ decode 経路を使います。

- `crates/bmz-app`
  - winit app、画面遷移、CLI、config、SQLite storage、skin loader、egui UI layer です。
  - `screens/play_*` がプレイ画面の app 側状態管理です。
  - `src/ui.rs` が egui の状態管理、イベント処理、本体設定 / スキン設定 / デバッグ表示の構築を担当します。
  - difficulty table と songs 管理の CLI は `src/cli.rs`, `src/table_cmd.rs`, `src/songs_cmd.rs` を確認します。

- `data/skins/default`
  - デフォルトスキン画像と `skin.toml`。
  - `note-blue.png`, `note-red.png` は key2/4/6 と scratch 系テクスチャに使います。

- `data/songs/sample-playable`
  - 手動確認用のサンプルBMSです。

- `data`
  - runtime data。原則コミットしません。
  - `config.toml`, `library.db`, `profiles/default/profile.toml`, `profiles/default/score.db` など。

## Worktree Setup

worktree で動作確認する場合、main worktree の runtime data / 外部アセットを作業用 snapshot としてコピーします。
`data/skins/default` と `data/songs/sample-playable` は git 管理の同梱データなので、worktree 側の checkout を使い、コピー対象から外します。

main worktree が `/Users/hyrorre/private/bmz-player` の場合:

```bash
mkdir -p data/skins data/songs
cp -a /Users/hyrorre/private/bmz-player/data/profiles data/profiles
cp -a /Users/hyrorre/private/bmz-player/data/config.toml data/config.toml
cp -a /Users/hyrorre/private/bmz-player/data/library.db data/library.db
find /Users/hyrorre/private/bmz-player/data/skins -mindepth 1 -maxdepth 1 ! -name default -exec cp -a {} data/skins/ \;
find /Users/hyrorre/private/bmz-player/data/songs -mindepth 1 -maxdepth 1 ! -name sample-playable -exec cp -a {} data/songs/ \;
```

注意:

- コピー前に worktree 側の `data/profiles`, `data/config.toml`, `data/library.db` が上書きされてよいか確認します。
- コピー後は `git status --short` で差分を確認し、今回の作業に関係ない `data/` 差分はコミットに含めません。
- 外部スキンや追加曲など gitignore 管理のファイルは、コピーされてもコミット対象にしません。
- DB migration / storage / scan 周りを検証すると `library.db` や `score.db` が更新されるため、main worktree の DB とは別物として扱います。

## Main Commands

```bash
cargo check
cargo test
cargo test -p bmz-render
cargo test -p bmz-skin
cargo test -p bmz-skin-convert
cargo test -p bmz-video
cargo test -p bmz-ffmpeg
cargo test -p bmz-font
cargo test -p bmz-app
cargo test -p bmz-app skin_loader
cargo fmt
cargo fmt --check
cargo clippy
```

アプリ起動:

```bash
cargo run -p bmz-app
cargo run -p bmz-app -- --boot-play-sample
cargo run -p bmz-app -- --boot-play-sample --smoke-exit-after-frames 3
cargo run -p bmz-app -- --boot-play-sample --autoplay-on-start --smoke-exit-on-result
cargo run -p bmz-app -- --boot-play-sample --boot-replay 1 --smoke-exit-on-result
cargo run -p bmz-app -- table list
cargo run -p bmz-app -- songs list
```

対応済みCLI引数 / サブコマンド:

起動時プレイ:

- `[PATH]` — 譜面 PATH を通常プレイで起動（ファイル不存在 / 未登録なら通常起動）
- `-a` / `--autoplay-on-start` — 起動譜面をオートプレイ
- `-r1` / `-r2` / `-r3` / `-r4` / `--boot-replay <1..4>` / `--boot-replay=<1..4>` — リプレイスロット指定
- `--boot-play-sample` — 同梱サンプル譜面で起動
- `--boot-course <COURSE_ID>` / `--boot-course=<COURSE_ID>` — 指定コースを fresh で起動
- `--boot-course-replay <COURSE_ID>` / `--boot-course-replay=<COURSE_ID>` — 指定コースの最新 attempt を replay 再生

その他:

- `--smoke-exit-after-frames <N>` / `--smoke-exit-after-frames=<N>`
- `--smoke-exit-on-result`
- `--renderer <backend>` (`vulkan`, `metal`, `dx12`, `gl`, `auto`)
- `-h` / `--help`

`table`:

- `table add <URL>`
- `table list`
- `table fetch` / `table fetch <URL>`

`songs`:

- `songs add <PATH> [--no-recursive] [--disabled]`
- `songs list`
- `songs load [PATH|NAME]`
- `songs reload [PATH|NAME]`

`course`:

- `course import <PATH>`
- `course list`
- `course history <COURSE_ID> [--limit N]`
- `course attempt <SCORE_ID>`

新しいデバッグフラグを追加するときも `crates/bmz-app/src/cli.rs` に集約してください。

## Manual Check Keys

アプリウィンドウにフォーカスがある状態:

- `F1`: egui メニューを開閉
- `F5`: 選曲画面で文脈依存 reload（曲フォルダは `songs reload`、難易度表は `table fetch <URL>`）
- `Escape` (Select 画面長押し 2 秒): アプリ終了

通常操作:

- `Enter` / `Space`: 選択中チャートを開始
- `Left` / `Right`: ハイスピードを 0.25 刻みで調整
- ハイスピード範囲は `0.5..=10.0`

デフォルトプレイキー:

- `LShift`: Scratch
- `Z S X D C F V`: Key1..Key7

## Coding Rules and Conventions

- コミットメッセージは Conventional Commits にします。
  - 既存履歴の Claude / Cursor 由来のメッセージに合わせ、subject は短い英語で「何を可能にしたか / 何を直したか」を具体的に書きます。
  - スコープに修正対象の crate 名を入れます。例: `fix(bmz-audio): ...`、`feat(bmz-skin): load json skin fonts`。
  - 複数 crate にまたがる変更は、主対象の crate をスコープにするか、`feat(bmz-render,bmz-skin): ...` のように主要 crate をカンマ区切りにします。全体的な作業だけスコープを省きます。
  - subject は命令形の小文字動詞で始め、末尾にピリオドを付けません。例: `add`, `cover`, `infer`, `map`, `render`, `support`, `fix`, `skip`, `update`。
  - `feat` はユーザー可視の機能追加、`fix` は不具合修正、`test` はテスト追加/修正、`chore` は docs/metadata/formatter など挙動に影響しない作業に使います。
  - 良い例: `feat(bmz-app): add play9 skin catalog and UI slot`、`fix(bmz-chart): saturate extreme timing values`、`test(bmz-skin): add Rm-skin load baseline and category normalization`、`chore: update agents.md`。
  - 1 行目に subject、2 行目を空行、3 行目以降に変更内容の詳細を書きます。本文は日本語で構いません。
  - 本文には「なぜ必要か」「何を変えたか」「どのテストで担保したか」を、変更規模に応じて短い段落または箇条書きで書きます。
  - Footer には対応した AI model / agent を `Co-Authored-By:` で書きます。モデル名を `GPT-5.5` のように詳しく特定できる場合は、その詳細名まで含めます。例: `Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>`、`Co-authored-by: Cursor <cursoragent@cursor.com>`、Codex の場合は `Co-Authored-By: Codex GPT-5.5 <noreply@openai.com>`。
  - テンプレート:
    ```text
    feat(bmz-app): add example behavior

    変更が必要だった背景を書く。

    - 主要な変更点を書く。
    - 検証したテストやリグレッションガードを書く。

    Co-Authored-By: Codex GPT-5.5 <noreply@openai.com>
    ```
- Windows / PowerShell でコミットする場合、メッセージ本文に `@` が混入することがあります。
  - 原因は `git commit -m @'...'@` の here-string で、native command への引数渡しで先頭に `@` が紛れ込むためです。
  - 対策: メッセージを一時ファイルへ書き、`git commit -F <file>` で渡します。1 行メッセージなら `-m "..."` でも構いません。
  - コミット後は `git log -1 --format=%s` で先頭に `@` が無いか確認します。
- 適切な粒度でコミットします。
- unrelated な差分は混ぜません。
- ユーザーや別ツールの変更を勝手に戻しません。
- `killall` を使ったデバッグは避けます。
- ファイル探索はまず `rg` / `rg --files` を使います。
- 手編集は `apply_patch` を使います。
- `cargo fmt` が広範囲を整形した場合、今回の作業と無関係な差分はコミットに混ぜないでください。
- 既存 warning は、今回の作業に関係しないなら原則触りません。
- Windowsで作業する時は、以下の点に注意。
  - Codex の sandbox 下では `pwsh` ではなく Windows 標準の `powershell.exe` 5.1 を使います。
  - Python は Codex 付属のものを使います。


## Gameplay Notes

判定名:

- 見逃しは `Poor`
- 空押しは `EmptyPoor`
- `Miss` は不要

コンボ:

- `PGreat`, `Great`, `Good` はコンボ継続/加算
- `Bad`, `Poor` はコンボを切る
- `EmptyPoor` は LR2 / beatoraja に近く、コンボを継続する

Empty Poor:

- `EmptyPoor` にも FAST/SLOW があります。
- FAST 側と SLOW 側は別の判定幅を持ちます。
- 入力時、PGREAT から BAD に相当する対象ノートが無い場合に `EMPTY POOR(SLOW)` 対象を探し、それも無ければ `EMPTY POOR(FAST)` 対象を探します。
- それも無ければ、その入力は判定に属しません。
- 例: PGREAT タイミングで2連打した場合に `EMPTY POOR(SLOW)` が起きます。
- `bad_us` 直後に見逃し `Poor` を出す仕様で問題ありません。

スコア/ゲージ:

- beatoraja 準拠を目指します。
- `crates/bmz-gameplay/src/score.rs` と `crates/bmz-gameplay/src/gauge.rs` を確認します。
- enum は保存時に string 化、空値は `""` 方針です。

入力:

- 将来、キーボードやゲームパッド入力を低遅延APIへ差し替える前提です。
- polling から event driven になっても直しやすいよう、backend と gameplay 入力変換の境界を保ってください。
- 関連: `crates/bmz-gameplay/src/input`, `crates/bmz-app/src/input`.

Mine (地雷):

- BMS の D系列 (P1) / E系列 (P2) チャネルに置かれたノーツを `NoteKind::Mine` として扱います。`NoteEvent.damage: Option<u16>` がチャネル値そのまま (= ダメージ量)。
- 描画: `RenderSnapshot.visible_mines` に振り分け、専用テクスチャ `data/skins/default/note-mine.png` (`DEFAULT_MINE_NOTE_TEXTURE = TextureId(12)`) で描きます。
- 判定: `JudgeWindow.mine_hit_us` (デフォルト 16ms) 以内の Press でヒット。`JudgeOutcome.mine_hits` に積み、`GameSession.pending_mine_hits` → `SessionFrame.mine_hits` → `FrameOutput.mine_hits` の経路で app 層まで運ばれます。
- 副作用: コンボ/スコア無影響、ゲージのみ `gauge.apply_mine(damage)` で減算 (guts 補正なし)。
- SE: app 側で `SoundType::Landmine` (`landmine.wav`) を再生。同一フレームの複数ヒットは 1 回にまとめて重ね鳴らししません。
- Autoplay は Mine を踏みません (`autoplay.rs` で skip)。

SCROLL / SPEED:

- BMS の SCROLL チャネル / SPEED チャネルを `PlayableChart.scroll_events` / `speed_events` に保持します。
- `ScrollContext::scroll_delta` で tick 区間積分し見かけ距離を計算。SCROLL factor は階段関数として畳み込み、SPEED factor は note 位置時点の値を倍率として最終 delta に掛けます。
- SCROLL factor < 0 は逆スクロール扱いで `note_y` が `None` (描画対象外) になります。
- SPEED は beatoraja では線形補間ですが、現状は階段関数で実装。

BMS パーサ:

- `bms-rs` 1.x を主パーサとして使用 (`crates/bmz-chart/src/import/bms_rs_adapter.rs`)。RANDOM/IF, LNOBJ, Mine, Base62, BMSON 等は bms-rs 側で吸収します。
- `random_seed` は `PlaySessionOptions.arrange_seed` (リプレイにも保存) から流すため、同じリプレイで RANDOM が必ず同じ分岐へ落ちます。
- bms-rs の `BmsWarning` は `map_bms_warning` で `ImportWarning::ParserDiagnostic { code, message }` に分類。`code` 名はそのまま `chart_import_warnings.code` に保存され、UI で識別できます。

## Storage and Config

config:

- serde + TOML 方針です。
- UI / theme / volume / play 表示設定は profile 側に寄せます。
- プレイスキンは key_mode 別に分かれ、profile の `[skin]` セクションで `play5` / `play7` / `play10` / `play14` の 4 フィールド (+ それぞれ `play{N}_options` / `play{N}_files`) を持ちます。決定画面でチャートの `key_mode` に応じて該当する 1 本だけを decode して install します。
- 曲 root は app config の `[songs]`、difficulty table source は app config の `[tables]` です。

database:

- `library.db` はライブラリ/チャート情報です。
- difficulty table 情報も `library.db` に保存します。
- `score.db` は profile ごとに分けます。
- 現在は `data/profiles/default/score.db` のような構成です。
- 別ツール連携時にファイルサイズを軽くするため、score DB は profile 単位を維持してください。
- 現在のアプリ利用は開発者1人のため、破壊的なスキーマ変更の提案を行っても問題ありません。

実装場所:

- config: `crates/bmz-app/src/config`
- paths: `crates/bmz-app/src/paths.rs`
- storage: `crates/bmz-app/src/storage`
- migrations: `crates/bmz-app/src/storage/migration.rs`
- difficulty table CLI/fetch: `crates/bmz-app/src/table_cmd.rs`, `crates/bmz-app/src/difficulty_table.rs`
- songs CLI/scan: `crates/bmz-app/src/songs_cmd.rs`, `crates/bmz-app/src/storage/scan.rs`

## Skin System Notes

方針:

- beatoraja JSON skin と Lua skin 互換を進めます。
- `bmz-skin` は decode 専用 crate とし、GPU texture upload や renderer 操作は持たせません。
- `bmz-app` は `.json` / `.luaskin` / `.lua` を profile の `[skin]` の `select` / `play5` / `play7` / `play10` / `play14` / `result` (および decide) から同じように受け付けます。プレイスキンは決定画面でチャートの `key_mode` から該当 1 本を選び、起動時には decode しません。
- Lua skin はロード時のみ sandbox 実行し、返された table を `SkinDocument` 相当へ変換します。
- 描画中の Lua function 評価は v1 では行いません。`value` / `draw` function は推論できるものだけ `ref` / `expr` / `draw` 条件へ変換し、未対応 function は warning として drop します。
- LR2 csvskin は将来検討です。
- Skin ID は読み込み時に `String` 化し、`100` と `"100"` は同一扱いにします。

### 外部スキンと仕様の基準

- **正しい仕様は beatoraja の実行結果**とする。`.local/beatoraja/` のソース（例: `SkinGauge.java`）を読み、同じスキン・同じプレイ条件で beatoraja と BMZ を突き合わせ、差分はエンジン側で埋める。
- `data/skins/` 配下の **第三者製スキン**（例: Starseeker）は手動確認用の参照コピー。**gitignore 管理・コミット禁止**。
- **再配布禁止などライセンス上、編集や再配布ができないスキンはファイルを変更しない。** 互換のための修正は `bmz-skin` / `bmz-render` / `bmz-app` のみに書く。
- スキン Lua / JSON を BMZ 向けに書き換えない（`skin.gauge` への `type` 追記なども含む）。beatoraja で問題なく動く未改変スキンの見え方を、BMZ がそのまま再現することを目標とする。

参照:

- beatoraja source: `.local/beatoraja/`
- skin examples: `data/skins/`
- bundled beatoraja default: `.local/beatoraja/skin/default/play7.json`

現在対応済みの主なJSON skin要素:

- numeric/string SkinId の正規化
- include / property option 展開の一部
- `source`, `image`, `imageset`
- `value`, `text`
- `note`, `gauge`, `judge`
- `slider` の play progress 系
- `hiddenCover`
- destination `timer`, `op`, safe `draw` condition の一部
- destination keyframe interpolation
- image `divx/divy/cycle` によるUV frame animation
- text `align`, `overflow`, `wrapping`, `shadow`, `outline`
- TTF/OTF/TTC font loading
- `.fnt` bitmap font loading
- destination `acc` easing
- static image destination `stretch`
- `graph` (type 101/102/110-115/140-147)
- `SkinDrawState`: BPM (now/min/max), lane_cover, total_duration_ms, judge_timing_ms, best/target ex_score
- `skin_state_number`: ref 14/90/91/107/121/150/160/163/164/310-312/407/420/425-427/525
- `skin_state_text`: ref 10-16 (title/subtitle/genre/artist/subartist 系)

現在対応済みの主なLua skin要素:

- `.luaskin` / `.lua` の load と `return skin` table の decode
- `skin_config.option`: `property.def` を優先し、なければ先頭 item を既定値にします。
- `skin_config.get_path()`: profile のスキン設定で選んだファイル (`skin.*_files` の filepath 定義名 → 相対パス) を最優先で返します。選択が無い / 該当ファイルが存在しない場合は wildcard の最初の実在候補へフォールバックします (Lua 側は現状 `filepath.def` は参照しません)。JSON skin 側の `source` / `font` ワイルドカード解決は同じ優先で、フォールバック順は ユーザ選択 → `filepath.def` → 先頭候補 です。
- sandbox: `os` / `io` / `debug` / `package.loadlib` を無効化します。
- `require` / `dofile` / `loadfile`: skin root 配下だけ許可します。
- Lua hook による命令数上限、table 深さ・配列長・総 entry 数の上限を持ちます。
- `main_state.number(...)` / `main_state.option(...)` / `main_state.timer(...)` / `main_state.gauge_type()` の一部 function 推論。
- `draw` function: 単一 ref 比較に加え、複数 ref の `or` / 2 ref 比較+`and`、定数 tail (`number(N)==0` 等) をロード時に draw 条件文字列へ変換。
- `graph.value` function: 加算式の除算 (`value_expr`) と graph type `148`/`149` (fast/slow 比率、12 ref 合計の `fastall/fsall` パターンをロード時推論) をサポート。
- Lua `draw` function: 複数 ref の `> 0` / `== 0` / `< 0` の OR、`number` と `skin_config.option` 定数の AND (`number(N) == 0` へ畳み込み) をサポート。
- `timer_util.timer_observe_boolean`: `dynamicTimer` + ID `9000+` に変換し、描画時は `DynamicTimerRuntime` で observe 条件のエッジから経過 ms を供給。
- 未対応 function は `lua skin load warning` としてログに出し、ロード自体は継続します。

未対応/今後の候補:

- Lua `draw` / `value` のさらに複雑な式 (3 ref 以上の任意 boolean、実行時に変わる `skin_config.option` 参照)
- Lua function warning の object id / source context 付き診断
- destination `center`, `offset`, `offsets`, `filter`
- destination `stretch` for non-static image objects
- `graph` / score graph 系 (type 101/102/110-115/140-147 は実装済み)
- より正確な text outline/shadow。現在の outline は周囲8方向描画の近似です。
- SDF/距離場フォント化
- beatoraja JSON skin の Mine 専用 sprite。現状は `DEFAULT_MINE_NOTE_TEXTURE` をフォールバックで使用。
- SPEED チャネルの線形補間 (現状は階段関数で代用)
- `SkinGauge` の `type=0` (RANDOM) 向け `animation` 更新、`prepare` 時の `parts` 再計算（モード差し替え時のボーダー割り切り）、Lua 側の `P1_grooveflash` 等の固定座標フラッシュが beatoraja と完全一致していない可能性（要ソース照合・エンジン側実装）
- bmz TOML スキン (`data/skins/default/skin.toml`) はまだ mode 共通の `[play.*]` セクションだけを持ち、mode 別 note 配置の差分は持てません。`play5` / `play7` / `play10` / `play14` の切り替えで別ディレクトリを指定することで対応します。

実装の入口:

- Skin decode API: `crates/bmz-skin/src/lib.rs`
- Lua sandbox / conversion / function inference: `crates/bmz-skin/src/lua.rs`
- JSON schema / render context types: `crates/bmz-render/src/skin.rs`
- draw plan: `crates/bmz-render/src/plan.rs`
- GPU/text renderer: `crates/bmz-render/src/renderer.rs`
- egui paint glue: `crates/bmz-render/src/ui.rs`
- app side skin decode/install: `crates/bmz-app/src/skin_loader.rs`
- app side egui UI: `crates/bmz-app/src/ui.rs`
- Lua to JSON CLI: `crates/bmz-skin-convert/src/main.rs`

## Rendering Notes

- 描画は実テクスチャ描画へ移行中です。
- Text は `DrawCommand::Text` から毎フレーム atlas を作って描きます。
- JSON skin の text は `TextStyle.font_id` でフォントを選び、未登録ならデフォルトフォントへ fallback します。
- OS デフォルトフォント（スキン未登録 `font_id` / egui CJK fallback）は `bmz-font` が `font-kit` で解決します。描画は `bmz-render/src/renderer.rs` の `load_default_font` / `load_japanese_font_bytes` から ab_glyph へ渡します。スキン同梱フォントの path 解決は `bmz-app/src/skin_loader.rs` です。
- 9分割描画は、角を固定して辺と中央だけ伸ばす描画です。パネルやゲージ枠など、角を崩したくないUI部品に使います。
- BGA は静止画 texture と動画 BGA の両方を扱います。動画 decode は `bmz-video`、ffmpeg 初期化は `bmz-ffmpeg`、renderer への texture upload / frame 選択は app 側 play flow を確認します。
- egui はゲーム / スキン描画の上に overlay します。winit event と `egui::Context` は `bmz-app`、`egui-wgpu` による描画は `bmz-render` の責務です。
- ノートのスクロール位置計算は `crates/bmz-app/src/screens/play_snapshot.rs::ScrollContext` に集約。BPM 変化 (timing_map)、STOP、SCROLL、SPEED をすべてここで畳み込みます。
- Mine ノーツは `visible_mines` に振り分け、デフォルトでは `DEFAULT_MINE_NOTE_TEXTURE` (note-mine.png) を使用。
- 主な確認対象は `bmz-render` の unit tests です。

## Change Checklist

変更前:

1. `git status --short` で作業ツリーを確認する。
2. 関連ファイルを `rg` で探す。
3. 既存テスト名を確認する。
4. `.local` や `data` をコミット対象にしないことを確認する。

実装中:

1. 変更範囲を crate / module の責務内に収める。
2. 既存 helper / parser / structured API を優先する。
3. 仕様変更には近い場所の unit test を追加する。

変更後:

1. `cargo fmt --check` を実行する。
2. `cargo check` を実行する。
3. `cargo clippy` を実行する。
4. `cargo test` を実行する。
5. 追加で、変更箇所に応じた絞り込みテストを必要なら実行する。
6. `git diff --stat` と `git status --short` で unrelated diff が無いことを確認する。
7. Conventional Commits でコミットする。

## Common Verification Patterns

required after every task:

```bash
cargo fmt --check
cargo check
cargo clippy
cargo test
```

render/skin:

```bash
cargo test -p bmz-font
cargo test -p bmz-render
cargo test -p bmz-skin
cargo test -p bmz-skin-convert
```

egui/UI:

```bash
cargo test -p bmz-app ui
cargo test -p bmz-render
```

skin loader:

```bash
cargo test -p bmz-app skin_loader
cargo test -p bmz-render
cargo test -p bmz-skin
```

gameplay judge/score/gauge:

```bash
cargo test -p bmz-gameplay
cargo test -p bmz-core
```

chart import/normalization:

```bash
cargo test -p bmz-chart
cargo test -p bmz-app storage
```

audio/video/ffmpeg:

```bash
cargo test -p bmz-audio
cargo test -p bmz-video
cargo test -p bmz-ffmpeg
```

font / default render:

```bash
cargo test -p bmz-font
cargo test -p bmz-render
```

manual smoke:

```bash
cargo run -p bmz-app -- --boot-play-sample --smoke-exit-after-frames 3
cargo run -p bmz-app -- --boot-play-sample --autoplay-on-start --smoke-exit-on-result
cargo run -p bmz-app -- --boot-play-sample --boot-replay 1 --smoke-exit-on-result
```

## IR (Internet Ranking)

bun / Nuxt を使用して、 Internet Ranking 機能の API と Frontend の作成中。

Nuxt 関連のアプリ構造は、bmz-player 本体と混同しないよう `bmz-ir-web/` 配下にまとめます。

- `bmz-ir-web/app/` — Nuxt app root。`app.vue`, `pages`, `components`, `layouts`, `composables`, `plugins`, `assets` など。
- `bmz-ir-web/server/` — Nitro server。`api`, `routes`, `middleware`, `plugins`, `services`, `repositories` など。
- `bmz-ir-web/shared/` — app/server 共通の型、schema、定数、純粋関数。secret や DB query は置きません。
- `bmz-ir-web/public/` — root URL で配信される静的ファイル。

`bun dev` はリポジトリ root から実行します。Nuxt のディレクトリ対応は `nuxt.config.ts` の `srcDir` / `serverDir` / `dir.public` / `dir.shared` で設定します。

`bun dev` 等でエラーが起きる場合は `export TMPDIR=/tmp` を実行してみる。

### Supabase / DB

IR Website の DB は Supabase CLI と migration を正とします。MCP / Hooks は現時点では使いません。

- DB schema は `supabase/migrations/*.sql` を source of truth とし、Dashboard 変更や ad hoc SQL をそのまま本番反映しません。
- remote 側で先に変更した場合は `supabase db pull` で migration 化し、SQL を確認してから commit します。
- migration 追加は `bun run db:new <name>`、local reset は `bun run db:reset`、型生成は `bun run db:types` を使います。
- 生成型は `bmz-ir-web/shared/types/database.types.ts` に置きます。
- local Supabase は `bun run db:start` / `bun run db:stop` で操作します。
- production / remote への `bun run db:push`、destructive SQL、remote write は必ずユーザー確認を取ります。
- RLS policy、grant、index、constraint、SQL function は migration に含めます。
- `.env`, `sb_secret_...`, legacy service role key, DB password, refresh token, production data は commit しません。必要な環境変数名だけ `.env.example` に書きます。
- ブラウザ / desktop / public client へ渡す Supabase key は `sb_publishable_...` を基本にし、`NUXT_PUBLIC_SUPABASE_KEY` / `SUPABASE_PUBLISHABLE_KEY` に入れます。legacy `anon` key は local CLI などで必要な場合だけ互換用として扱います。
- server-only の elevated key は `sb_secret_...` を基本にし、`NUXT_SUPABASE_SECRET_KEY` / `SUPABASE_SECRET_KEY` に入れます。legacy `service_role` key は互換用として扱い、絶対に public env に入れません。
- Supabase DB / migration / RLS / generated types / server-side Supabase 接続処理を触るときは repo skill `supabase-ir-db` を使います。

Supabase 関連の主なコマンド:

```bash
bun run db:start
bun run db:new <name>
bun run db:reset
bun run db:types
bun run db:push
bun run db:stop
```
