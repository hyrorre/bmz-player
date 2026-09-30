# AGENTS.md

BMZ Player の継続開発で常に参照する作業方針です。機能の詳細仕様と手順は、作業別の参照先を確認してください。

## 1. 作業方針

- 回答・提案・Planは日本語で行います。
- BMZ Player は LunaticRave2 / beatoraja の後継を目指すBMSプレイヤーです。
  Rust + wgpu、egui、cpal + ffmpeg-next、bms-rsを使用し、Windows / macOS / Linuxを対象にします。
- 「調査」「レビュー」「改良案」「実装計画」の依頼では、調査結果と提案を先に示します。
  修正を依頼されたら、合意した範囲の実装・検証まで進めます。
- 最初に `git status --short` を確認し、ユーザーや別ツールの変更を保持します。
  作業と無関係な差分を戻したり、コミットへ混ぜたりしません。
- ファイル・テキスト探索は `rg --files` / `rg`、手編集は `apply_patch` を使います。
  PowerShellではファイル引数のワイルドカード展開に頼らず、実在するパスを渡します。
- 既存のhelper・境界・テストを調べてから変更します。関係のないwarningや大規模整形は原則触りません。
- 不具合は設定・保存データ・非同期処理・入力・描画まで関連する経路を追います。
  検証結果は自動テスト、実機確認、未実施の確認を区別します。
- `killall` による一括停止を避け、必要なプロセスを特定して扱います。

## 2. リポジトリ構成

| 場所 | 責務と主な入口 |
|---|---|
| [bmz-core](crates/bmz-core/src/lib.rs) | Lane、Judge、TimeUs、ChartTick、replay/input/clear等の共通型 |
| [bmz-chart](crates/bmz-chart/src/lib.rs) | BMS/BMSONのimport・normalize・timing。`src/import/`、`src/model.rs` |
| [bmz-gameplay](crates/bmz-gameplay/src/lib.rs) | 判定、score、gauge、session、autoplay、入力変換 |
| [bmz-audio](crates/bmz-audio/src/lib.rs) | cpal backend、mixer、sample decode、audio clock |
| [bmz-ffmpeg](crates/bmz-ffmpeg/src/lib.rs) | プロセス単位のFFmpeg初期化・ログレベル調整 |
| [bmz-font](crates/bmz-font/src/lib.rs) | 同梱フォント優先・OS fallbackの解決。path/memory bytes/TTC indexを扱う |
| [bmz-video](crates/bmz-video/src/lib.rs) | FFmpegによるBGA decodeとフレーム供給 |
| [bmz-skin-document](crates/bmz-skin-document/src/lib.rs) | SkinDocumentのschema、JSON load/include、runtime値型 |
| [bmz-skin](crates/bmz-skin/src/lib.rs) | JSON / Lua / LR2 skin decode、sandbox、Lua推論・runtime callback |
| [bmz-skin-convert](crates/bmz-skin-convert/src/main.rs) | bmz-skinを呼ぶLua-to-JSON CLI |
| [bmz-render](crates/bmz-render/src/lib.rs) | wgpu、draw plan、scene snapshot、skin評価、egui描画 |
| [bmz-player](crates/bmz-player/src/lib.rs) | winit app、画面遷移、config、SQLite、CLI、skin install、egui状態 |
| [bmz-updater](crates/bmz-updater/src/lib.rs) | Windows更新パッケージ検証・署名・helper・復旧 |
| [bmz-ir-web](bmz-ir-web/) | Nuxtのapp/server/shared/public。ルートのpackage.jsonから操作 |
| [docs](docs/) | 継続更新する現在の仕様・操作・開発手順・対応状況 |
| [notes](notes/README.md) | Git管理する日付付きの調査・実装・計測記録 |
| [data](data/) | 同梱アセットとローカルruntime data。Git管理対象は次節を参照 |

`bmz-player/src/app/` がapp側の連携、`screens/` が画面・プレイ状態、
`skin_loader/` がdecode/install、`ui/` が設定UIの主な入口です。

## 3. 重要な設計制約

### crateとスレッドの責務

- `bmz-skin-document` はschema/decode用です。wgpu / egui等の描画依存を追加しません。
  `bmz-skin` と `bmz-render` がこの型を共有します。
- `bmz-skin` にGPU uploadやrenderer操作を持ち込まず、app側でdecode結果をinstallします。
  `SkinDocumentRenderExt` による描画評価は `bmz-render` の責務です。
- eguiのContext・イベント・UI状態はapp側、paint primitivesの描画はrenderer側に置きます。
- `bmz-ffmpeg` は初期化の共通化に留め、音声・動画decodeの型と処理は利用側crateに置きます。
  `bmz-video` にBMSや判定の責務を持ち込まず、`bmz-updater` に音声・GPU依存を追加しません。
- Play開始後のGameSessionはgameplay専用スレッドが更新します。
  window threadはsnapshotを受け取り、描画・画面遷移・永続化を担当します。
  入力取得、判定、audio callbackの所有権と時計は [gameplay-runtime.md](docs/gameplay-runtime.md) を確認します。
- 非同期ロードは要求の世代と適用済み状態を区別し、古い結果が新しい状態を上書きしないようにします。

### スキン互換と描画

- beatoraja互換部分は、参照ソース `.local/beatoraja/` と、同じスキン・条件での実行結果を基準にします。
  BMZ独自拡張は [skin.md](docs/skin.md) の契約に従い、互換仕様と区別します。
- 第三者製スキンのLua/JSONをBMZ向けに書き換えず、`bmz-skin` / `bmz-render` / `bmz-player` で互換性を修正します。
  編集・再配布の可否はライセンスを確認します。
- Luaのruntime callbackは推論用VMとは別の永続VMに保持し、closure/module stateを維持します。
  stateful callbackの評価を描画キャッシュで省略しません。
- Luaのファイル解決は `SkinPathContext` の許可root内に限定します。
  `os` / `io` は限定的な互換APIであり、任意のOS操作や実ファイル書き込みを許可しません。
  命令数・メモリ・table量の上限を維持します。詳細は [Lua仕様](docs/skin.md) を確認します。
- skin type、ref/option/timer ID、profileのslot一覧は [skin.md](docs/skin.md) と設定型を確認します。
  対応状況は [skin-compatibility.md](docs/skin-compatibility.md) に集約します。
- フォント解決は `bmz-font` を利用し、同梱フォント優先とOS fallback、memory bytes、TTC indexを維持します。
  スキン内のfont/sourceパス解決はapp側のskin loaderで扱います。
- 文字描画はatlas cacheを利用します。フォント変更時の無効化と描画用データの再利用を区別します。

### データと配布物

- configはserde + TOMLです。表示・音量・プレイ設定はprofile、曲rootと難易度表sourceはapp configに置きます。
- `library.db` はライブラリ・難易度表、`score.db` はprofileごとのスコアです。
  score DBをprofile単位で維持します。保存条件は [score-persistence.md](docs/score-persistence.md) を確認します。
- 破壊的なschema変更も提案できます。変更時は既存データへの影響と移行・復旧方法を明示します。
- runtimeのconfig・DB・profile・追加曲・外部スキン、`.local/` の参照コピーや検証ログはコミットしません。
  `data/` 全体が除外対象ではなく、既存のGit管理対象を維持します。
- 同梱データは `data/skins/default`、`data/songs/sample-playable`、同梱フォント等です。
  `data/skins/Rmz-skin`、`data/skins/mz-select`、`data/skins/Luxez-Flat` は [.gitmodules](.gitmodules) 管理のsubmoduleです。
  初期化・worktree用データの準備は [development.md](docs/development.md) を参照します。
- worktreeのDB/configはmainと独立させます。既存ファイルを無断で上書きせず、DBコピーは書き込み停止または整合したbackupを使います。
- FFmpeg・フォント・スキン等を配布物に含める変更では、必ず [licenses.md](docs/licenses.md) を確認します。
- 秘密情報、`.env`、token、production dataをコミットしません。環境変数の例は `.env.example` に置きます。
  production / remoteへの破壊的書き込みは、対象と内容についてユーザーの明示的な承認を得てから実行します。

## 4. 作業別の参照先

| 作業 | 先に読む文書・入口 |
|---|---|
| build・worktree・smoke | [README](README.md)、[development.md](docs/development.md) |
| キー操作・設定UI | [controls.md](docs/controls.md)、`crates/bmz-player/src/input/`、`src/ui/` |
| 判定・ゲージ・コンボ | [rule.md](docs/rule.md)、`crates/bmz-gameplay/src/judge/`、`src/score.rs`、`src/gauge.rs` |
| LN・score identity | [ln.md](docs/ln.md)、[score-persistence.md](docs/score-persistence.md) |
| HS・SCROLL・SPEED | [hs.md](docs/hs.md)、`crates/bmz-player/src/screens/play_snapshot/scroll.rs` |
| 入力・時計・描画停止時の進行 | [gameplay-runtime.md](docs/gameplay-runtime.md)、[frame-pacing.md](docs/frame-pacing.md) |
| スキンdecode・描画・互換差分 | [skin.md](docs/skin.md)、[skin-compatibility.md](docs/skin-compatibility.md) |
| CLI・Viewer・動画出力 | `cargo run -p bmz-player -- --help`、[cli-overrides.md](docs/cli-overrides.md)、[viewer.md](docs/viewer.md)、[video-export.md](docs/video-export.md) |
| Select・曲scan・難易度表 | [select-folders.md](docs/select-folders.md)、`src/songs_cmd.rs`、`src/table_cmd.rs`、`src/storage/`（bmz-player） |
| config・保存パス・migration | `crates/bmz-player/src/config/`、`src/paths.rs`、`src/storage/migration.rs` |
| IR client / server | [ir.md](docs/ir.md)、[rian-ir.md](docs/rian-ir.md)、`crates/bmz-player/src/ir/`、`bmz-ir-web/` |
| 翻訳 | [i18n.md](docs/i18n.md) |
| 配布・更新・ライセンス | [packaging.md](docs/packaging.md)、[auto-update.md](docs/auto-update.md)、[linux-tar.md](docs/linux-tar.md)、[licenses.md](docs/licenses.md) |

操作やキー割り当てを変更するときは `docs/controls.md` を確認・更新します。
新しいCLI/debug optionは `crates/bmz-player/src/cli.rs` の既存構成に集約します。
仕様や対応範囲を変更したら、その仕様を管理しているdocも更新します。

作業経緯・過去の計測や検証結果は `notes/YYYY/YYYY-MM-DD-topic.md` に記録します。
確定した仕様・再利用する手順は `docs/` に反映し、必要に応じて記録へリンクします。
一時的な下書きは `.local/notes/`、生ログや個別環境の計測素材は `.local/performance/` 等へ置き、コミットしません。
命名・追記・索引のルールは [notes/README.md](notes/README.md) を確認します。

## 5. 検証

### 変更の種類で選ぶ

| 変更 | 必要な確認 |
|---|---|
| 調査・提案のみ | 根拠となるコード・文書・ログを確認。編集・commitは行わない |
| 文書のみ | 差分、リンク先、コード参照、記載コマンドの定義を確認。Rust全テストは原則不要 |
| 単一crateのRust変更 | fmt、対象crateのcheck / all-targets Clippy / testと必要な利用側テスト |
| 共有API・依存関係・複数crate | workspace全体のcheck / all-targets Clippy / test |
| IR Web | 対象ファイルのPrettier、型検査、関連するIRテスト |
| 配布・入力・音声・GPU | 上記に加えて対象OS/backendで必要なbuild・実機確認 |

引数なしの `cargo check` / `cargo test` は、ルートCargo.tomlの
`default-members = ["crates/bmz-player"]` が対象です。
workspace全体の確認では `--workspace` を指定します。exampleも公開APIの利用側として確認します。

Rust変更の基本形（`bmz-player` は変更対象に置き換える）:

```bash
cargo fmt --check
cargo check -p bmz-player --locked
cargo clippy -p bmz-player --all-targets --locked -- -D warnings
cargo test -p bmz-player --locked
```

共有変更の検証:

```bash
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked --no-fail-fast
```

lockfile自体を更新する依頼では、その更新を先に行ってから `--locked` で検証します。
既存の無関係な問題で失敗した場合は、その失敗と今回の変更の検証範囲を分けて報告し、
warningの抑制や無関係な修正で結果を取り繕いません。

### テスト範囲の選び方

- skin loader変更では `bmz-player` のskin_loaderテストと、影響する `bmz-skin` / `bmz-skin-document` / `bmz-render` を確認します。
- 判定変更では `bmz-gameplay` とapp側のsession / replay / 保存条件まで確認します。
- 音声・動画変更では利用側と `bmz-ffmpeg`、フォント変更では `bmz-font` とrenderer / eguiの利用側を確認します。
- バグ修正には問題の状態遷移を再現するテストを追加します。配置変更など、挙動が変わらない修正には既存テストを利用します。
- 外部アセットが無いと早期returnするテストは、成功件数だけで互換確認済みと判断しません。
  素材の有無、検証したスキン、未実施の手動確認を報告します。
- 正常起動やsmoke成功を性能改善の証拠にしません。性能を報告する場合は条件を揃えて比較し、
  CPU処理時間、表示FPS、surface/present待ちを分けます。
- 必要な検証が通った後、追加変更や新たな懸念がなければ同じテストを繰り返しません。

Webのコマンドはリポジトリrootから実行します。対象ファイルに対するPrettier確認に加え、
`bunx vue-tsc --noEmit` と `bun run test:ir` を使います。scriptの定義は [package.json](package.json) を確認します。

## 6. コミット

- 実装・ドキュメント変更は、検証後に適切な粒度でコミットします。ユーザーがcommit不要と指定した場合は従います。
- ステージ前に `git diff --stat` / `git status --short` を確認し、対象ファイルを明示してstageします。
  `git diff --cached` で最終内容を確認し、無関係な変更を含めません。
- Conventional Commitsを使い、scopeは主対象crate、必要なら主要crateのカンマ区切りとします。
  リポジトリ全体の文書・設定変更ではscopeを省略できます。
- subjectは具体的で短い英語とし、小文字の命令形動詞で始め、末尾にピリオドを付けません。
  `feat` は機能追加、`fix` は不具合修正、`test` はテスト変更、`chore` は文書・整形等に使います。
- 本文に必要性、主要変更、検証結果を記載します。日本語で構いません。
  Footerには実際に使用したagent/modelを `Co-Authored-By:` で記載します。

```text
fix(bmz-player): discard obsolete result skin refreshes

必要だった背景と変更内容を書く。

- 検証したテストと、未実施の確認があれば記載する。

Co-Authored-By: Codex <実際のモデル名> <noreply@openai.com>
```

- 本文やFooterを含む複数行のcommit messageは、OSやshellを問わずBOMなしUTF-8の一時ファイルへ保存し、
  `git commit -F <file>` で渡します。shell引数内の `\n` による改行表現は、literalな `\n` の混入を防ぐため使いません。
  一時ファイルは `.local/commit-message.txt` 等のGit管理外の場所へ作成し、commit後に削除します。
- macOS / Linuxでは `apply_patch` 等の通常のファイル編集で一時ファイルを作成します。
  Windows / PowerShell 7では、例として `Set-Content -Encoding utf8NoBOM .local/commit-message.txt` を使えます。
- commit後は `git show -s --format=fuller HEAD` で本文とFooterを確認し、literalな `\n` が混入していれば、
  正しいメッセージファイルを使って直ちに `git commit --amend -F <file>` で修正します。
