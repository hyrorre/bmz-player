# Releaseメタデータ

Release添付の `release.json` は、全OSの実行ファイル識別、配布アーカイブのSHA256、Windowsの
更新条件をまとめる署名付きJSON。従来の独立したclient manifestを置き換える。
移行期間は旧版の更新確認用に `updates.json`、汎用検証・Linux再配布用に `SHA256SUMS.txt` も残す。
配布物内部の `updater/bmz-package.json` とmacOSのSparkle appcastは別の役割で維持する。

## 形式と署名

schemaは `bmz-release-manifest-v1`。構造の例は
[テスト用署名付きJSON](../crates/bmz-updater/tests/fixtures/release.json) を参照。
例の公開テスト鍵・架空のハッシュは実際の配布には使用しない。

| フィールド | 内容 |
|---|---|
| `client` / `version` / `git_commit` | `bmz-player`、SemVer、ビルド元の40桁commit |
| `builds[]` | `id`、`platform`、`arch`、`package_kind`、最終実行ファイルの `client_hash` |
| `artifacts[]` | 配布物の `name`、`build`参照、`kind`、`url`、`size`、`sha256` |
| `artifacts[].update` | Windowsにのみ付く `min_updater_protocol` と `bridge_tag` |
| `signature` | Ed25519署名の標準Base64表現 |

v1はWindows x64、macOS arm64/x64、Linux x64 Flatpak/tarの5ビルドと、Windowsのportable/installer、
macOSの2 ZIP、Flatpak、Linux実行用/ソースtarの7配布物を収録する。Windowsの2配布物は同じ
実行ファイルを参照し、ソースtarはLinux tarビルドへ対応付ける。manifest自身のハッシュは含めない。

署名対象はトップレベルの `signature` だけを除いた全JSON。RFC 8785のJCSで正規化したUTF-8へ
署名する。v1の数値はJSON整数表記の非負safe integer（最大2^53-1）に限定し、浮動小数点・指数表記を
必要としない。実際のサイズは2 GiB未満、protocolは正の整数。未知・重複フィールド、不正なUnicode、
不完全な配布物一覧、不正URL・ハッシュ・橋渡し版は拒否する。署名検証には既存の
`BMZ_UPDATE_PUBLIC_KEY` を使用し、ファイルから新しい信頼鍵を受け取らない。

本体は `release.json` assetを優先する。存在しない場合だけ `updates.json` を読み、取得・署名・
内容のエラーでは旧形式へフォールバックしない。公開直前まで新旧メタデータをアップロードしない。

rianIRはトップレベルの `builds` をそのまま取り込む。確認したインポーターは署名を検証しない。
アプリのIR送信は従来どおり自身の実行ファイルをハッシュし、ReleaseのJSONを参照しない。

## 生成とdry run

各ビルドは最終実行ファイルのhash sidecarを作り、最後のジョブでversion・commit・target集合を照合する。
署名・patchelf等の実行ファイルを書き換える処理はハッシュ計算より先に完了させる。

```sh
node scripts/generate-release-metadata.mjs dist/release VERSION COMMIT
```

生成処理は7配布物を一度ずつハッシュし、その値を `release.json`、互換用 `updates.json`、
`SHA256SUMS.txt` で共有する。SHA256SUMSには完成した `release.json` 自身も含む。
hash sidecarは生成後にRelease upload候補から除去する。

`BMZ_UPDATE_PRIVATE_KEY` / `BMZ_UPDATE_PUBLIC_KEY` は既存の鍵を継続使用する。
`BMZ_WINDOWS_UPDATER_LAYOUT`、`BMZ_UPDATE_BRIDGE_TAG`、`BMZ_MIN_UPDATER_PROTOCOL` を参照する。
最低protocol未指定時はgrouped=2 / legacy=1。公開用は必要な橋渡しタグ・署名鍵がなければ失敗する。
橋渡しタグは対象版より古く、Stable公開ではStableタグである必要がある。

公開しないworkflow_dispatchは `--unsigned` を使う。出力の `signature: null` は明示的な未署名データで、
アプリの更新検証では拒否される。dry runは公開用秘密鍵を使わず、`updates.json` も生成しない。
署名形式のNode/Rust相互運用は公開テスト鍵の共通fixtureで検証する。

## 橋渡し版Bの公開準備

既存の配置移行の橋渡し版をA、統合メタデータ対応の最初の版をBと呼ぶ。Bの本体/helperはprotocol 3だが、
Bの旧更新情報・groupedパッケージ内部の最低protocolは2に留め、protocol 2の旧版から導入できるようにする。

1. Bのversionを決め、workspace versionとReleaseタグを一致させる。Aより新しいStable版にする。
2. 公開済みAのタグ・署名・minimum protocol=1・本体のprotocol 2対応を確認する。
3. Repository variablesを `BMZ_WINDOWS_UPDATER_LAYOUT=grouped`、`BMZ_UPDATE_BRIDGE_TAG=Aのタグ`、
   `BMZ_MIN_UPDATER_PROTOCOL=2` に設定する。Bの公開時に最低protocolを3へ上げない。
4. Bのタグで `workflow_dispatch` の `upload_to_release=false` を実行し、5ビルド/7配布物、
   version/commit一致、内部ファイル一覧、Linux起動/オフライン再ビルド、macOS署名を確認する。
5. 公開用と同じ構成の隔離テスト配布物で、Windows portable/installerの旧版→A→B、
   protocol 2版→B、再起動、中断復旧、helper自身の更新を確認する。macOS両CPUのSparkle更新も確認する。
6. 配布物・SHA256SUMSを先に添付し、完成した `release.json`、`updates.json`、Sparkleフィードを公開する。
   rianIRには `release.json` の `builds` を登録する。過去のAの配布物・更新情報は削除しない。

実装時の公開状態確認と未実施の確認は [作業記録](../notes/2026/2026-10-05-release-metadata.md) を参照。
コードを取り込むだけではRepository variablesの変更・タグ発行・公開は行われない。

## B公開後

後続版Cで旧版をB経由にする場合は `BMZ_MIN_UPDATER_PROTOCOL=3` と
`BMZ_UPDATE_BRIDGE_TAG=Bのタグ` をセットで変更する。旧形式schemaは1のまま維持する。
protocol 1版はC→B→Aの更新情報をたどり、再起動を挟んでA→B→Cへ進む。
protocol 2版はB→C、B以降の対応版は `release.json` からCを適用できる。

完全に1ファイルへ減らす変更は別段階とする。旧 `updates.json` 廃止にはB未満からの自動更新サポート終了の
告知、SHA256SUMS廃止にはLinuxの検証コード・再配布手順と利用者向け検証手段の移行が必要。
最新Releaseに旧形式がないと旧版は橋渡し版へ到達できない。過去のA/Bを残すだけでは経路を維持できない。
