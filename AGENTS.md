# AGENTS.md

このメモは、Codex でこのリポジトリを継続開発するときの作業入口です。実装前に構成と約束事を確認し、変更後はここにある検証コマンドを実行してください。

## Project Goal

BMZ Player は LunaticRave2 / beatoraja の後継を目指す BMS プレイヤーです。
技術方針は Rust + wgpu、音声は cpal、BMS パースは `bms-rs` 利用を前提にする予定ですが、現在は自前パースです。
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

- `crates/bmz-render`
  - wgpu renderer、描画 plan、scene snapshot、skin document の描画評価です。
  - `SkinContext`、`SkinRenderItem`、texture id との紐付け、`draw` / `op` / `timer` 評価を持ちます。

- `crates/bmz-skin`
  - beatoraja JSON skin / Lua skin の document decode crate です。
  - JSON skin loader、Lua sandbox、`skin_config`、Lua table to JSON、`main_state` function 推論を扱います。
  - v1 はロード時だけ Lua を実行し、描画中の毎フレーム Lua 実行はしません。

- `crates/bmz-skin-convert`
  - Lua skin を JSON に変換する薄い CLI です。
  - 本体ロジックは `bmz-skin` を呼び、アプリ内 Lua load と同じ decode 経路を使います。

- `crates/bmz-app`
  - winit app、画面遷移、CLI、config、SQLite storage、skin loader です。
  - `screens/play_*` がプレイ画面の app 側状態管理です。

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
```

対応済みCLI引数:

- `--boot-play-sample`
- `--autoplay-on-start`
- `--smoke-exit-after-frames <N>`
- `--smoke-exit-after-frames=<N>`
- `--smoke-exit-on-result`
- `-h`, `--help`

以前使っていた `BMZ_*` 環境変数ベースの smoke/debug 操作は CLI 引数へ移行済みです。新しいデバッグフラグを追加するときも `crates/bmz-app/src/cli.rs` に集約してください。

## Manual Check Keys

アプリウィンドウにフォーカスがある状態:

- `F1`: sample Select scene
- `F2`: sample Play scene
- `F3`: sample Result scene
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

## Storage and Config

config:

- serde + TOML 方針です。
- UI / theme / volume は profile 側に寄せます。

database:

- `library.db` はライブラリ/チャート情報です。
- `score.db` は profile ごとに分けます。
- 現在は `data/profiles/default/score.db` のような構成です。
- 別ツール連携時にファイルサイズを軽くするため、score DB は profile 単位を維持してください。
- 現在のアプリ利用は開発者1人のため、破壊的なスキーマ変更の提案を行っても問題ありません。

実装場所:

- config: `crates/bmz-app/src/config`
- paths: `crates/bmz-app/src/paths.rs`
- storage: `crates/bmz-app/src/storage`
- migrations: `crates/bmz-app/src/storage/migration.rs`

## Skin System Notes

方針:

- beatoraja JSON skin と Lua skin 互換を進めます。
- `bmz-skin` は decode 専用 crate とし、GPU texture upload や renderer 操作は持たせません。
- `bmz-app` は `.json` / `.luaskin` / `.lua` を profile の `[skin] select/play/result` から同じように受け付けます。
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
- `skin_config.get_path()`: `filepath.def` を優先し、なければ wildcard の最初の実在候補を返します。
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
- `graph` / score graph 系 (type 101/102/110-115/140-147 は実装済み。best/target score は play_snapshot.rs で DB 連携 TODO)
- BGA
- より正確な text outline/shadow。現在の outline は周囲8方向描画の近似です。
- SDF/距離場フォント化

実装の入口:

- Skin decode API: `crates/bmz-skin/src/lib.rs`
- Lua sandbox / conversion / function inference: `crates/bmz-skin/src/lua.rs`
- JSON schema / render context types: `crates/bmz-render/src/skin.rs`
- draw plan: `crates/bmz-render/src/plan.rs`
- GPU/text renderer: `crates/bmz-render/src/renderer.rs`
- app side skin decode/install: `crates/bmz-app/src/skin_loader.rs`
- Lua to JSON CLI: `crates/bmz-skin-convert/src/main.rs`

## Rendering Notes

- 描画は rect から実テクスチャ描画へ移行中です。
- Text は `DrawCommand::Text` から毎フレーム atlas を作って描きます。
- JSON skin の text は `TextStyle.font_id` でフォントを選び、未登録ならデフォルトフォントへ fallback します。
- 9分割描画は、角を固定して辺と中央だけ伸ばす描画です。パネルやゲージ枠など、角を崩したくないUI部品に使います。
- 主な確認対象は `bmz-render` の unit tests です。

## Change Checklist

変更前:

1. `git status --short` で作業ツリーを確認する。
2. 関連ファイルを `rg` で探す。
3. 既存テスト名を確認する。
4. `.local` や `data` をコミット対象にしないことを確認する。

実装中:

1. 変更範囲を crate/module の責務内に収める。
2. 既存 helper / parser / structured API を優先する。
3. 仕様変更には近い場所の unit test を追加する。
4. Play 画面優先。Select 画面は大きな設計変更がありそうなら後回し。

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

manual smoke:

```bash
cargo run -p bmz-app -- --boot-play-sample --smoke-exit-after-frames 3
cargo run -p bmz-app -- --boot-play-sample --autoplay-on-start --smoke-exit-on-result
```
