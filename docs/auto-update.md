# 自動更新

## 利用者向けの動作

- Windows portable は現在の配置を更新する。installer への切替はしない。
- macOS の配布 `.app` は Sparkle 2.9.6 で bundle 全体を更新する。
- 更新確認の設定・チャンネル・スキップ版は既存の `config.toml` の `[updates]` を使う。
- 確認・通知・更新適用は起動準備完了後の Select だけ。Viewer、CLI の動画出力、Play、Result では適用しない。
- 「アップデート」で取得・検証し、「更新して再起動」で適用する。自動ダウンロード・無人更新は行わない。
- ダウンロードはキャンセル可能。Select を離れると中断し、戻ってから再試行できる。部分ダウンロードは再利用せず先頭から取得する。
- macOS の展開は Sparkle の処理で、途中キャンセル要求は遅くともインストール準備完了時に反映する。
- 再起動は同じデータ・キャッシュ・ログ・リソースの保存先とプロファイルを引き継ぎ、Select へ戻る。譜面起動・Viewer・export 等の元の引数は再実行しない。
- 既存の旧版からは、この機能を搭載する版まで一度手動更新する。

## Windows portable

`bmz-package.json` は配布形式・target・version・updater protocol・配布物のファイル一覧と SHA256 を持つ。
`data` の有無やカレントディレクトリから配布形式を推測しない。メタデータのない旧版・開発ビルドは自動適用しない。
installer 作成中だけ `kind=installer` とし、ZIP 用 staging は `kind=portable` に戻す。

更新アーカイブには `BMZ Player/` が一つあり、exe、DLL、`resources/`、メタデータだけを収録する。
manifest にないファイル・リンク・Windows の特殊名・展開先外のパスを拒否する。
ダウンロードは署名済みのサイズ・SHA256を照合し、展開ファイルも個別に照合する。

更新先の `.bmz-update/job-*/stage` に展開し、同一ボリューム内の rename で適用する。
`bmz-updater.exe` は `job-*/helper.exe` にコピーして実行するため、配布フォルダの updater 自身も更新できる。
helper が準備完了してから本体を正常終了し、すべての packaged BMZ プロセスの共有ロック解放を待つ。
60 秒以内に終了しない別プロセスがあれば更新を中止し、強制終了はしない。

設定、DB、スコア、リプレイ、追加曲、ユーザースキン、未知のファイルは更新対象にしない。
旧版で管理していた廃止 DLL も退避して除去する。新しい配布ファイルと未知の既存ファイルが衝突した場合は更新を拒否する。
同梱ファイルへのユーザー変更を含め、旧ファイルは `job-*/backup` に保持する。
更新結果と退避ファイルの対応表は `job-*/result.json`、結果ログは `job-*/update.log` に残る。
退避ファイルは自動削除しない。容量を空ける際は内容を確認して、完了済み job ディレクトリを削除する。

### 中断・復旧

複数ファイルの置換は一括の原子的操作ではない。`.bmz-update/active.json` の記録と退避ファイルから復旧する。
通常の適用失敗は自動で旧版へ戻す。途中でプロセス終了・電源断が起きた場合、次回起動は未完了状態を検出して停止する。
すべての BMZ プロセスを閉じてから実行する:

```powershell
& 'C:\Games\BMZ Player\bmz-updater.exe' --recover 'C:\Games\BMZ Player'
```

本体や updater の置換途中で通常の exe が存在しない場合は、該当 job のコピーを使う:

```powershell
& 'C:\Games\BMZ Player\.bmz-update\job-...\helper.exe' --recover 'C:\Games\BMZ Player'
```

これは適用途中の復旧用。新版起動後に DB migration が済んだ状態での旧版への自動ダウングレードは行わない。

### updater の互換性

現在の protocol は 1。署名済み更新情報の `min_updater_protocol` を満たさない場合は、
`bridge_tag` が指す旧形式の橋渡し版へ先に更新する。次回起動後、新updaterが最新版を取得する。
橋渡しは現在版より新しく、目的版より古く、選択チャンネル内にあることを検証する。最大8段で打ち切る。
橋渡し版のReleaseと `updates.json` を削除しないこと。
`bmz-package.json` の `min_updater_protocol` も「このパッケージを読んで適用するのに必要なprotocol」を指す。
新helper自身がprotocol 2でも、橋渡し版のパッケージ形式とこの値は1を維持する。

## 更新の署名と公開設定

### Windows

- Repository variable `BMZ_UPDATE_PUBLIC_KEY`: Ed25519 公開鍵の raw 32 bytes を base64 化した値。
- Repository secret `BMZ_UPDATE_PRIVATE_KEY`: 対応する Ed25519 秘密鍵の PKCS8 PEM。
- 本体のビルド時に `BMZ_UPDATE_PUBLIC_KEY` を埋め込む。キー未設定のローカルビルドでは portable 自動適用を無効にする。
- `scripts/generate-update-metadata.mjs release` はキーの対応を確認して `updates.json` の payload bytes に署名する。
- SHA256だけでなく、公開鍵で署名を検証してから対象URL・サイズ・version・protocolを使用する。

キーは管理者が一度生成し、安全にバックアップする。秘密鍵をリポジトリや配信先へ置かない。
鍵交換時は、旧鍵で署名した橋渡し版に新公開鍵を埋め込んで先に配布する必要がある。
同じ版の更新ファイルを後から異なる内容へ差し替えず、新しいversionを発行する。

### macOS

- Repository variable `BMZ_SPARKLE_PUBLIC_KEY`: Sparkle `generate_keys` が表示する公開鍵。
- Repository secret `BMZ_SPARKLE_PRIVATE_KEY`: Sparkle `generate_keys -x` が出力する秘密鍵。
- 既存の `BMZ_MACOS_CODESIGN_*`、証明書、Keychain、`BMZ_MACOS_NOTARY_*` secrets も必要。
- 公開ビルドは Developer ID 署名・公証・stapling・署名キーの設定を必須にする。
- ローカルで組み込む場合は `bash scripts/prepare-sparkle.sh /tmp/bmz-sparkle` を実行し、
  `BMZ_SPARKLE_DIR=/tmp/bmz-sparkle` と `BMZ_SPARKLE_PUBLIC_KEY` を指定して package script を実行する。
- 配布用 `.app` に Sparkle.framework をコピーし、入れ子の helper / XPC / framework から順に署名する。
- 更新ZIPは公証・stapling後に最終生成し、SparkleのEdDSA署名を付ける。展開前の署名検証を有効にする。
- Feedは `update-feed` prerelease の `appcast-{stable|prerelease}-{x64|arm64}.xml`。
- Sparkle の自動スケジューラー・自動ダウンロード・システム情報送信は無効。BMZが確認時刻を管理する。
- Read-only mount、App Translocation、権限不足などの失敗はSparkleのエラーとReleaseページへの導線で扱う。
- 再起動時の保存先は `~/Library/Caches/net.hyrorre.bmz-player/update-restart.json` に一時保存する。
  同じインストール先で引数なしに起動した時だけ消費し、10分で失効する。

API / 配布仕様: [Sparkle](https://sparkle-project.org/documentation/)、
[SPUUserDriver](https://sparkle-project.org/documentation/api-reference/Protocols/SPUUserDriver.html)。

### CI の公開順

1. Windows portable / installer、macOS両CPU版、Flatpakを生成・検証。
2. 最終macOS ZIPに署名し、Windowsの署名付き更新情報を生成。
3. 配布アーカイブ・SHA256SUMSをReleaseへ添付。
4. 完成した `updates.json` とSparkleフィードを最後に公開。

`update-feed` の書込みはworkflow間で直列化する。既存フィードの履歴を保持し、古いOSで動く過去版を消さない。
署名キーを扱わないworkflow_dispatchは配布生成のdry runとして利用できる。

## 検証

```text
cargo test -p bmz-updater
cargo test -p bmz-player update
node --test scripts/generate-update-metadata.test.mjs
python scripts/test_sparkle_appcast.py
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

公開前には、署名済み旧版→新版でWindows実配布物とmacOS両CPUの更新・再起動・最低対応OSを確認する。
通常データとは別のテスト用データを使い、通信中断、容量不足、別プロセス、読み取り専用配置も確認する。
