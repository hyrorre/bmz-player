# Releaseメタデータの統合と橋渡し版の準備

日付: 2026-10-05。作業開始時HEAD: `70eb1a25`。Windows環境。
仕様と再利用する公開手順は [Releaseメタデータ](../../docs/release-metadata.md) を参照。

## 調査と判断

- アプリにclient manifestの読み込み経路はない。`ir/client_hash.rs` が実行ファイルのSHA256を計算し、
  rianIRの単曲/course送信に付ける。次版から独立client manifestをReleaseへ添付しない方針とした。
- `.local/rianIR/src/Service/ClientManifestImportService.php` はトップレベルのclient/version/buildsだけを
  取り込み、追加項目・schema・署名は検査しない。平文のbuildsを保持した統合JSONはそのまま取り込める。
  実サービスをメモリSQLiteで動かし、5ビルドの登録に成功。本番DBへの登録は行っていない。
- 公開済み`v0.4.3`の`updates.json`を取得したところ、Windows両形式の最低protocolは1、bridgeはnull。
  Repository variableの公開鍵で署名検証に成功。同タグのupdaterソースはprotocol 2。
  既存の配置移行橋渡し版Aとして利用できる構成である。
- 読み取り時のRepository variableは`BMZ_WINDOWS_UPDATER_LAYOUT=legacy`、`BMZ_UPDATE_BRIDGE_TAG`未設定。
  公開準備ではgrouped、橋渡しタグv0.4.3、最低protocol 2への設定が必要。remoteの設定変更はしていない。

## 実装

- 5ビルド/7配布物をまとめる`release.json`を生成。version/commit/target集合を照合し、同じハッシュから
  旧`updates.json`とSHA256SUMSも生成する。client hash sidecarは内部の受け渡しとして残す。
- 既存Ed25519鍵を継続し、非負safe integerに限定したJCS形式でsignature以外の全JSONに署名。
  NodeとRustで同じ公開テスト鍵fixtureを検証する。新しい依存crateは追加していない。
- 本体は新形式がない場合だけ旧形式を使用。不正な新形式から旧形式へはフォールバックしない。
- 対応protocolを3にした。groupedのpackage minimumは2のまま。Bのrelease minimumも2に保ち、
  後続Cは明示設定のminimum=3/bridge=BでB経由にできる。
- CIは配布物を先に添付し、新旧更新メタデータを最後に公開。dry runのsignatureはnullで適用不可。
  dry runは公開用秘密鍵を使わず、旧版用updates.jsonも生成しない。

## 検証

- Nodeの生成・集約・署名テスト: 16件成功。
- `cargo test -p bmz-updater --locked`: 19件成功。最初のsandbox実行は既存のWindowsファイル置換で
  アクセス拒否となったため、通常権限で再実行して復旧・lock・置換を含め成功。
- `cargo check --workspace --locked`: 成功。sandboxでincremental cacheのhard link失敗→copyの環境warningあり。
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: 通常権限で成功。
- `cargo test --workspace --locked --no-fail-fast`: 成功（既定ignoreは28件）。bmz-playerは
  2192成功/19 ignoredで、新旧metadata選択・protocol 1→2→3の橋渡しテストを含む。
- `cargo fmt --check`、対象JS/YAMLのPrettier、YAML parser、`git diff --check`: 成功。
  Python側にPyYAMLがなかったため、既存Node依存のyaml parserを使用した。

公開済み実配布物を使う旧版→A→Bの実更新・再起動、macOS/Linuxの実機・CIパッケージ生成は未実施。
Releaseタグ発行・version確定・署名付き配布物の公開・Repository variables変更は今回行っていない。
ローカル生ログは`.local/release-manifest-validation/`、取得した旧メタデータは
`.local/release-metadata-inspection/v0.4.3/`に保存し、Gitには含めない。
