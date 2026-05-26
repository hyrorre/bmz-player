# AGENTS.md

このメモは、Codex でこのリポジトリを継続開発するときの作業入口です。実装前に構成と約束事を確認し、変更後はここにある検証コマンドを実行してください。

## Project Goal

BMZ Player は LunaticRave2 / beatoraja の後継を目指す BMS プレイヤーです。
技術方針は Rust + wgpu、アプリ内 UI は egui、音声/動画は cpal + ffmpeg-next、BMS パースは `bms-rs` を利用。
対象OSは Windows / macOS / Linux です。

最初のゲームモードは 5K / 7K / 10K / 14K 通常プレイとオートプレイです。

beatoraja の参照ソースは `.local/beatoraja/`、beatoraja 対応スキン例は `.local/skins/` に置かれます。これらは gitignore 管理で、コミット対象にしません。

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

- `assets/skins/default`
  - デフォルトスキン画像と `skin.toml`。
  - `note-blue.png`, `note-red.png` は key2/4/6 と scratch 系テクスチャに使います。

- `assets/songs/sample-playable`
  - 手動確認用のサンプルBMSです。

- `data`
  - runtime data。原則コミットしません。
  - `config.toml`, `library.db`, `profiles/default/profile.toml`, `profiles/default/score.db` など。

## Main Commands

```bash
cargo check
cargo test
cargo test -p bmz-render
cargo test -p bmz-skin
cargo test -p bmz-skin-convert
cargo test -p bmz-video
cargo test -p bmz-ffmpeg
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

- `--boot-play-sample`
- `--autoplay-on-start`
- `--boot-replay <1..4>`
- `--boot-replay=<1..4>`
- `--smoke-exit-after-frames <N>`
- `--smoke-exit-after-frames=<N>`
- `--smoke-exit-on-result`
- `-h`, `--help`
- `table add <URL>`
- `table list`
- `table fetch`
- `songs add <PATH> [--no-recursive] [--disabled]`
- `songs list`
- `songs reload`

新しいデバッグフラグを追加するときも `crates/bmz-app/src/cli.rs` に集約してください。

## Manual Check Keys

アプリウィンドウにフォーカスがある状態:

- `F1`: sample Select scene
- `F2`: sample Play scene
- `F3`: sample Result scene
- `F5`: egui メニューを開閉
- `Escape`: sample scene を抜けて通常状態へ戻る

通常操作:

- `Enter` / `Space`: 選択中チャートを開始
- `Left` / `Right`: ハイスピードを 0.25 刻みで調整
- ハイスピード範囲は `0.5..=10.0`

デフォルトプレイキー:

- `LShift`: Scratch
- `Z S X D C F V`: Key1..Key7

## Coding Rules and Conventions

- コミットメッセージは Conventional Commits にします。
  - スコープに修正対象の crate 名を入れます。例: `fix(bmz-audio): ...`、`feat(bmz-skin): load json skin fonts`。
  - 複数 crate にまたがる変更は、主対象の crate をスコープにするか、スコープを省きます。
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
- 描画: `RenderSnapshot.visible_mines` に振り分け、専用テクスチャ `assets/skins/default/note-mine.png` (`DEFAULT_MINE_NOTE_TEXTURE = TextureId(12)`) で描きます。
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

参照:

- beatoraja source: `.local/beatoraja/`
- skin examples: `.local/skins/`
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
- 未対応 function は `lua skin load warning` としてログに出し、ロード自体は継続します。

未対応/今後の候補:

- Lua `value` / `draw` function の複数 ref 条件、複合 boolean、比率 graph value などの推論拡張
- Lua function warning の object id / source context 付き診断
- destination `center`, `offset`, `offsets`, `filter`
- destination `stretch` for non-static image objects
- `graph` / score graph 系 (type 101/102/110-115/140-147 は実装済み)
- より正確な text outline/shadow。現在の outline は周囲8方向描画の近似です。
- SDF/距離場フォント化
- beatoraja JSON skin の Mine 専用 sprite。現状は `DEFAULT_MINE_NOTE_TEXTURE` をフォールバックで使用。
- SPEED チャネルの線形補間 (現状は階段関数で代用)
- bmz TOML スキン (`assets/skins/default/skin.toml`) はまだ mode 共通の `[play.*]` セクションだけを持ち、mode 別 note 配置の差分は持てません。`play5` / `play7` / `play10` / `play14` の切り替えで別ディレクトリを指定することで対応します。

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

manual smoke:

```bash
cargo run -p bmz-app -- --boot-play-sample --smoke-exit-after-frames 3
cargo run -p bmz-app -- --boot-play-sample --autoplay-on-start --smoke-exit-on-result
cargo run -p bmz-app -- --boot-play-sample --boot-replay 1 --smoke-exit-on-result
```
