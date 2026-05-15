# AGENTS.md

このメモは、Codex でこのリポジトリを継続開発するときの作業入口です。実装前に構成と約束事を確認し、変更後はここにある検証コマンドを実行してください。

## Project Goal

BMZ Player は LunaticRave2 / beatoraja の後継を目指す BMS プレイヤーです。技術方針は Rust + wgpu、音声は cpal、BMS パースは `bms-rs` 利用を前提にしています。対象OSは Windows / macOS / Linux です。

最初のゲームモードは 7K 通常プレイとオートプレイです。Select 画面は後で設計変更される可能性が高いので、当面は Play 画面と互換性基盤を優先します。

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
  - wgpu renderer、描画 plan、scene snapshot、beatoraja JSON skin の解釈です。
  - Play skin 互換作業の主戦場です。

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
  - 例: `feat: load json skin fonts`
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

実装場所:

- config: `crates/bmz-app/src/config`
- paths: `crates/bmz-app/src/paths.rs`
- storage: `crates/bmz-app/src/storage`
- migrations: `crates/bmz-app/src/storage/migration.rs`

## Skin System Notes

方針:

- Lua skin は仕様が複雑でセキュリティ面も重いので、まず beatoraja JSON skin 互換を進めます。
- LR2 csvskin / luaskin は将来検討です。
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

未対応/今後の候補:

- destination `center`, `offset`, `offsets`, `filter`
- destination `stretch` for non-static image objects
- `graph` / score graph 系
- BGA
- より正確な text outline/shadow。現在の outline は周囲8方向描画の近似です。
- SDF/距離場フォント化

実装の入口:

- JSON parse/eval: `crates/bmz-render/src/skin.rs`
- draw plan: `crates/bmz-render/src/plan.rs`
- GPU/text renderer: `crates/bmz-render/src/renderer.rs`
- app side skin loading: `crates/bmz-app/src/skin_loader.rs`

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
```

skin loader:

```bash
cargo test -p bmz-app skin_loader
cargo test -p bmz-render
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
