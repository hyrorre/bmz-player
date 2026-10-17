# 自動更新

## 利用者向けの動作

- Windows portableは現在の配置を更新します。installerへの切替はしません。
- macOSの配布`.app`はSparkle 2.9.6でbundle全体を更新します。
- 更新確認の設定・チャンネル・スキップ版は既存の`config.toml`の`[updates]`を使います。
- 確認・通知・更新適用は、起動準備が終わった後の選曲画面だけで行います。Viewer、CLIの動画出力、プレイ画面、リザルト画面では適用しません。
- 「アップデート」で取得・検証し、「更新して再起動」で適用します。自動ダウンロード・無人更新は行いません。
- ダウンロードはキャンセル可能です。Selectを離れると中断し、戻ってから再試行できます。部分ダウンロードは再利用せず先頭から取得します。
- macOSの展開はSparkleの処理で、途中キャンセル要求は遅くともインストール準備完了時に反映します。
- 再起動は同じデータ・キャッシュ・ログ・リソースの保存先とプロファイルを引き継ぎ、Selectへ戻ります。譜面起動・Viewer・export等の元の引数は再実行しません。
- 既存の旧版からは、この機能を搭載する版まで一度手動更新します。

## Windows portable

Windows Setupは配布ファイルのみを上書きし、`resources`を一括削除しません。
配布物に含まれない追加スキン・フォント等は、同梱スキン内の追加ファイルも含めて残します。
旧版にだけ含まれるファイルもSetupでは残します。アンインストールでも未知ファイルとユーザーデータは削除しません。
配布物と同じパスの編集済みファイルはSetupの上書き対象なので、編集用コピーは`data_dir/skins`へ置きます。
この仕様は修正版Setupから適用され、旧Setupによって既に消えたファイルを復元するものではありません。


`updater/bmz-package.json`は配布形式・target・version・updater protocol・配布物のファイル一覧とSHA256を持ちます。
`data`の有無やカレントディレクトリから配布形式を推測しません。メタデータのない旧版・開発ビルドは自動適用しません。
installer作成中だけ`kind=installer`とし、ZIP用stagingは`kind=portable`に戻します。
旧配置の直下`bmz-package.json`も読み込めます。新配置のメタデータが存在する場合は優先し、
壊れていても旧配置や別の保存先へフォールバックしません。
portableのデータは環境変数の上書きを最優先とし、未指定時は必ずexe横の`data`を使います。
保存先の詳細は[Windowsの保存先と優先順位](packaging.md#windows-の保存先と優先順位)を参照してください。

更新アーカイブには`BMZ Player/`が一つあり、本体exe、DLL、`resources/`、
`updater/bmz-updater.exe`、`updater/bmz-package.json`、空の`updater/instance.lock`だけを収録します。
manifestにないファイル・リンク・Windowsの特殊名・展開先外のパスを拒否します。
ダウンロードは署名済みのサイズ・SHA256を照合し、展開ファイルも個別に照合します。

更新先の`updater/job-*/stage`に展開し、同一ボリューム内のrenameで適用します。
`updater/bmz-updater.exe`は`job-*/helper.exe`にコピーして実行するため、updater自身も更新できます。
helperが準備完了してから本体を正常終了し、すべてのpackaged BMZプロセスの共有ロック解放を待ちます。
60秒以内に終了しない別プロセスがあれば更新を中止し、強制終了はしません。
新規配置では共有ロックを`updater/instance.lock`、更新排他ロックを`updater/update.lock`に置きます。
旧配置からの移行時は旧ロックとも協調します。開いたロックファイルの削除による排他の分断を
防ぐため、既存の`.bmz-instance.lock` / `.bmz-updater.lock`は保持します。
旧`.bmz-update/job-*`のバックアップも自動削除・移動しません。

設定、DB、スコア、リプレイ、追加曲、ユーザースキン、未知のファイルは更新対象にしません。
旧版で管理していた廃止DLLも退避して除去します。新しい配布ファイルと未知の既存ファイルが衝突した場合は更新を拒否します。
同梱ファイルへのユーザー変更を含め、旧ファイルは`job-*/backup`に保持します。
更新結果と退避ファイルの対応表は`job-*/result.json`、結果ログは`job-*/update.log`に残ります。
退避ファイルは自動削除しません。容量を空ける際は内容を確認して、完了済みjobディレクトリを削除します。

### 中断・復旧

複数ファイルの置換は一括の原子的操作ではありません。`updater/active.json`の記録と退避ファイルから復旧します。
旧`.bmz-update/active.json`も検出・復旧できます。両方に未完了記録がある場合は処理を拒否します。
通常の適用失敗は自動で旧版へ戻します。途中でプロセス終了・電源断が起きた場合、次回起動は未完了状態を検出して停止します。
すべてのBMZプロセスを閉じてから実行する:

```powershell
& 'C:\Games\BMZ Player\updater\bmz-updater.exe' --recover 'C:\Games\BMZ Player'
```

本体やupdaterの置換途中で通常のexeが存在しない場合は、該当jobのコピーを使う:

```powershell
& 'C:\Games\BMZ Player\updater\job-...\helper.exe' --recover 'C:\Games\BMZ Player'
```

これは適用途中の復旧用です。新版起動後にDB migrationが済んだ状態での旧版への自動ダウングレードは行いません。
旧配置に残っている場合は直下の`bmz-updater.exe`、旧更新作業なら`.bmz-update/job-*/helper.exe`を使います。

### updater の互換性

現在のprotocolは3です。新しい本体は署名付き`release.json`を優先し、assetが存在しない場合だけ
従来の`updates.json`を読みます。新形式の取得・署名・内容検証が失敗した場合は旧形式へ戻りません。
署名済み更新情報の`min_updater_protocol`を満たさない場合は、
`bridge_tag`が指す旧形式の橋渡し版へ先に更新します。次回起動後、新updaterが最新版を取得します。
橋渡しは現在版より新しく、目的版より古く、選択チャンネル内にあることを検証します。最大8段で打ち切ります。
橋渡し版のReleaseと`updates.json`を削除しません。
`bmz-package.json`の`min_updater_protocol`も「このパッケージを読んで適用するのに必要なprotocol」を指します。
新helper自身がprotocol 2でも、橋渡し版のパッケージ形式とこの値は1を維持します。

protocol 1から2への配置移行は次の順序で行う（このコード変更だけではReleaseを公開しない）:

1. Repository variable `BMZ_WINDOWS_UPDATER_LAYOUT=legacy`を設定し、新旧配置対応の本体と
   updaterを旧配置で含む橋渡し版を公開します。package / releaseのminimum protocolは1です。
2. 橋渡し版を公開後、`BMZ_WINDOWS_UPDATER_LAYOUT=grouped`（既定値）に戻し、
   `BMZ_UPDATE_BRIDGE_TAG`にその公開済みタグを設定します。
3. それより新しい版を公開します。minimum protocolは2です。protocol 1の利用者は橋渡し版を
   経由してから新配置へ進みます。移行時は旧helper・旧メタデータを退避して除去し、新メタデータを最後に確定します。

新配置で橋渡しタグが未設定の場合、公開workflowと更新情報生成は失敗させます。
`BMZ_MIN_UPDATER_PROTOCOL`を下げても、新配置をprotocol 1として公開することはできません。
ローカルのパッケージ作成・公開しないdry runには橋渡しタグは不要です。

protocol 3は統合Releaseメタデータへの対応を示します。groupedパッケージ内部の形式は変わらず、
`updater/bmz-package.json`の最低protocolは2を維持します。protocol 3の本体・helperを含む
橋渡し版Bも、旧`updates.json`の最低protocolは2にします。B公開後に最低protocolを3へ
引き上げる場合は、`BMZ_UPDATE_BRIDGE_TAG`をBへ切り替えます。
形式・公開設定・段階的な移行は[Releaseメタデータ](release-metadata.md)を参照してください。

## 更新の署名と公開設定

### Windows

- Repository variable `BMZ_UPDATE_PUBLIC_KEY`: Ed25519公開鍵のraw 32 bytesをbase64化した値です。
- Repository secret `BMZ_UPDATE_PRIVATE_KEY`: 対応するEd25519秘密鍵のPKCS8 PEMです。
- 本体のビルド時に`BMZ_UPDATE_PUBLIC_KEY`を埋め込みます。キー未設定のローカルビルドではportable自動適用を無効にします。
- `scripts/generate-release-metadata.mjs`はキーの対応を確認し、統合`release.json`と互換用
  `updates.json`を同じ配布物情報から生成・署名します。旧形式のpayload bytesへの署名も維持します。
- SHA256だけでなく、公開鍵で署名を検証してから対象URL・サイズ・version・protocolを使用します。

キーは管理者が一度生成し、安全にバックアップします。秘密鍵をリポジトリや配信先へ置きません。
鍵交換時は、旧鍵で署名した橋渡し版に新公開鍵を埋め込んで先に配布する必要があります。
同じ版の更新ファイルを後から異なる内容へ差し替えず、新しいversionを発行します。

### macOS

Apple Developer Program未加入でも公開できます。アプリ本体・同梱framework/helperは
アドホック署名し、更新ZIPにはSparkleのEdDSA署名を付けます。両者は別の署名で、
Developer ID署名・公証を使わない場合も更新ZIPの署名検証は必須とします。

- Repository variable `BMZ_SPARKLE_PUBLIC_KEY`: Sparkle `generate_keys`が表示する公開鍵です。
- Repository secret `BMZ_SPARKLE_PRIVATE_KEY`: Sparkle `generate_keys -x <ファイル>`が書き出す秘密鍵ファイルの内容です。
- 公開には上記の公開鍵・秘密鍵が必要です。鍵は一度生成して継続使用し、安全にバックアップします。
- Appleの証明書・Keychain・`BMZ_MACOS_NOTARY_*` secretsは任意です。未設定ならアドホック署名で公開し、公証・staplingを省略します。
- Developer ID署名用secretsが揃っていればその証明書を使い、公証用secretsも揃っていれば公証・staplingを行います。設定済みの署名・公証処理が失敗した場合は公開を中止します。
- ローカルで組み込む場合は`bash scripts/prepare-sparkle.sh /tmp/bmz-sparkle`を実行し、
  `BMZ_SPARKLE_DIR=/tmp/bmz-sparkle`と`BMZ_SPARKLE_PUBLIC_KEY`を指定し、package scriptに`--bundle-dylibs --ad-hoc-sign`を渡します。
- 配布用`.app`にSparkle.frameworkをコピーし、入れ子のhelper / XPC / frameworkから順に署名します。
- 更新ZIPはコード署名後（公証する場合はstapling後）に最終生成し、SparkleのEdDSA署名を付けます。展開前の署名検証を有効にします。
- Feedは`update-feed` prereleaseの`appcast-{stable|prerelease}-{x64|arm64}.xml`です。
- Sparkleの自動スケジューラー・自動ダウンロード・システム情報送信は無効です。BMZが確認時刻を管理します。
- Read-only mount、App Translocation、権限不足などの失敗はSparkleのエラーとReleaseページへの導線で扱います。
- 再起動時の保存先は`~/Library/Caches/net.hyrorre.bmz-player/update-restart.json`に一時保存します。
  同じインストール先で引数なしに起動した時だけ消費し、10分で失効します。
- 「更新して再起動」の承認後もwinitのイベントループと`NSApplication.delegate`を維持します。
  Sparkleが再起動延期handlerを渡した場合は、winitループ内で設定・未完了スコアを保存し、
  音声・入力・スキン・BGA等のワーカーと描画資源を停止してからhandlerを一度だけ呼びます。
  Sparkleが契約上このdelegate callbackを呼ばず直接終了する場合も、winitの通常終了処理が
  同じ冪等な終了準備を実行します。EventLoop終了後にCocoa run loopを再開しません。

未加入での最小設定はSparkleの公開鍵variable・秘密鍵secretのみです（Windows用更新署名キーは別途必要）。
Appleの署名・公証secretsは未設定のまま、通常のRelease公開または`upload_to_release=true`で配布します。
アドホック署名はAppleによる開発元確認・公証を提供しないため、ダウンロードしたアプリの
初回起動などでGatekeeperの警告や利用者による許可操作が必要になる場合があります。
ZIPを展開して`.app`をApplications等の書き込み可能な場所に配置し、
[Appleの案内](https://support.apple.com/ja-jp/102445)に従って起動します。

API / 配布仕様: [Sparkle](https://sparkle-project.org/documentation/)、
[SPUUserDriver](https://sparkle-project.org/documentation/api-reference/Protocols/SPUUserDriver.html)です。

### CI の公開順

1. Windows portable / installer、macOS両CPU版、Flatpak、Linux tarと対応ソースを生成・検証です。
2. 最終macOS ZIPに署名し、署名付き`release.json` / `updates.json`とSHA256SUMSを生成します。
3. 配布アーカイブ・SHA256SUMSをReleaseへ添付です。
4. 完成した`release.json`、互換用`updates.json`、Sparkleフィードを最後に公開します。

`update-feed`の書込みはworkflow間で直列化します。既存フィードの履歴を保持し、古いOSで動く過去版を消しません。
署名キーを扱わないworkflow_dispatchは配布生成のdry runとして利用できます。

## 検証

```text
cargo test -p bmz-updater
cargo test -p bmz-player update
node --test scripts/generate-update-metadata.test.mjs
node --test scripts/generate-release-metadata.test.mjs
node --test scripts/verify-sparkle-signature.test.mjs
python scripts/test_sparkle_appcast.py
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
pwsh -NoProfile -File scripts/test-windows-installer.ps1
cargo build -p bmz-player -p bmz-updater
pwsh -NoProfile -File scripts/test-windows-package-paths.ps1
```

公開前には、更新署名付きの旧版→新版でWindows実配布物とmacOS両CPUの更新・再起動・最低対応OSを確認します。
Setupの回帰検証はInno Setupの`iscc.exe`を必要とします。独自AppIdと`.local/installer-test-*`の
隔離配置を使い、新規・更新・再インストール・アンインストールで未知ファイルを保持することを検証します。
テスト用のHKCUアンインストール登録とショートカットはアンインストール時に除去し、検証ログは残します。
保存先テストは実exeを`.local/package-path-test-*`に配置し、AppData・環境変数・作業フォルダも
隔離して新旧配置、初回起動、installer、上書き、書込失敗、メタデータのバージョン不一致を検証します。
配布済みstagingで検証する場合は`-BinaryDirectory "dist/windows/BMZ Player"`を指定します。
macOSはアドホック署名・未公証の実配布物をダウンロードして、初回起動とSparkle更新後の再起動を確認します。
通常データとは別のテスト用データを使い、通信中断、容量不足、別プロセス、読み取り専用配置も確認します。

macOSでは固定SDKを用意してネイティブブリッジのコンパイルとJSON境界も確認する:

```bash
BMZ_SPARKLE_DIR=/tmp/bmz-sparkle cargo check -p bmz-player
clang -fobjc-arc -fblocks -F/tmp/bmz-sparkle -framework AppKit -framework Sparkle \
  -Wl,-rpath,/tmp/bmz-sparkle scripts/test-sparkle-bridge.m -o /tmp/bmz-test-sparkle-bridge
/tmp/bmz-test-sparkle-bridge
BMZ_SPARKLE_DIR=/tmp/bmz-sparkle DYLD_FRAMEWORK_PATH=/tmp/bmz-sparkle cargo run -p bmz-player \
  --example sparkle_winit_handoff
```

後者は実際のwinit `EventLoop`とnative bridgeを同時に初期化し、Sparkleへの引き継ぎ待ちに
user eventが届いても処理できること、継続handlerが一度だけ実行されることを確認します。
リリース前の実機確認では、修正を含む更新元テストビルドから、より新しい更新先テストビルドへ
更新し、入力中の再起動、設定・スコア・profile保存先の継承、キャンセルとエラー経路も確認します。

公開時は秘密鍵のバイト長を仮定せず、Sparkleで生成したアーカイブ署名を
`verify-sparkle-signature.mjs`で埋込み公開鍵に照合します。32バイトseed形式と旧形式は
Sparkleの署名ツールが読み込みます。
