# Linux tar.gz配布

Linux x86_64向けに、GitHub Releasesで実行用アーカイブと対応ソースのアーカイブを配布します。
この文書では、実行方法・更新・ビルドと検証・ライセンス表記と対応ソースを扱います。
入力・音声の依存関係、権限、診断、元に戻す手順は[Linux遅延検証](linux-latency.md)を参照してください。

## 概要

- 動作の最低基準はUbuntu 22.04（glibc 2.35）です。他のディストリビューションでの動作は保証しません。Flatpak版も引き続き利用できます。
- 一般の利用者に必要なのは実行用アーカイブだけです。ソースのアーカイブは再ビルド用です。
- ビルドでは`pulseaudio,pipewire,linux-evdev`を有効にします。
- バージョンを固定したPipeWire 1.4.9のクライアント、native-protocol / client-node / adapter / metadata / rtモジュール、SPAのsupport / audioconvertプラグイン、専用のクライアント設定を同梱します。
- 音声サーバー、サービス、PAM、udevのポリシーは同梱しません。
- 音声のAutoはPulseAudio、次にALSAを優先します。PipeWireとevdevは明示的に選んだ場合だけ使います。

ランチャーは、呼び出し元がPipeWire / SPA / 設定の検索パスを設定していない場合だけ、パッケージ内のパスを設定します。
quantumは強制しません。
ランチャーは呼び出し元の値を記録し、BMZはxdg-open・ブラウザ・ファイルマネージャーを起動するときにその値へ戻します（元の値がなければパッケージ内のパスを外します）。
これにより、外部のプログラムが同梱のPipeWireモジュールをホストのクライアントライブラリへ読み込むことはありません。

## 実行

`bmz-player-v<version>-linux-x64.tar.gz`を展開し、中の`./bmz-player`ランチャーを実行します。
`bin/bmz-player`を直接実行せず、ランチャーを使ってください。
展開先は読み取り専用でも構いません。`bin`、`lib`、`resources`の各ディレクトリは同じ場所に置いたままにします。
Flatpakのインストール、rootでのインストール、システムのFFmpegは不要です。

実行ファイルと共有ライブラリは、相対パスのELF RUNPATHを使います。
ランチャーは、動画出力のエンコーダーやデスクトップのブラウザなどの外部プログラムへ、パッケージ全体の`LD_LIBRARY_PATH`を渡しません。

ホスト側には次のものが必要です。

- x86_64のglibc実行環境
- デスクトップセッション（X11またはWayland）
- 動作するVulkanドライバー（`--renderer gl`を指定する場合はOpenGL）
- ALSA、またはPipeWire-PulseなどのPulseAudio互換サーバー経由の音声デバイス

GPUドライバーとそのloader / ICD、ディスプレイサーバー、音声サーバー、デバイスの権限はホスト側で用意します。
最小構成のUbuntu 22.04では、次のコマンドで連携用のパッケージをインストールできます。

```sh
sudo apt-get install libvulkan1 libgl1 libegl1 libx11-6 libxcursor1 libxi6 \
  libxrandr2 libxkbcommon0 libxkbcommon-x11-0 libwayland-client0 libwayland-cursor0 libwayland-egl1 \
  libasound2 libasound2-plugins libpulse0 libfontconfig1 libudev1 ca-certificates
```

GPUドライバー（Mesa Vulkanドライバーや、ディストリビューションのNVIDIAドライバーなど）は別途インストールし、デスクトップのサウンドサーバーを設定してください。
ほかに、機能によって次のものが必要です。

- OSの認証情報ストア：セッションのD-BusとSecret Service
- URLを開く操作：デスクトップの`xdg-open`
- 動画出力：必要なエンコーダーを含む外部のFFmpeg実行ファイル（同梱のFFmpegはデコード専用です）

既定の書き込み先は次のとおりです。

| 内容 | 場所 |
| --- | --- |
| 設定、ライブラリDB、プロファイル、スコア、リプレイ、追加スキン | `${XDG_DATA_HOME:-$HOME/.local/share}/bmz-player` |
| キャッシュ | `${XDG_CACHE_HOME:-$HOME/.cache}/bmz-player` |
| ログ | `${XDG_STATE_HOME:-$HOME/.local/state}/bmz-player/logs` |

空でない`BMZ_RESOURCE_DIR`、`BMZ_DATA_DIR`、`BMZ_CACHE_DIR`、`BMZ_LOGS_DIR`は、これらの既定値より優先します。
`BMZ_DATA_DIR`を明示した場合、個別に指定しない限り、キャッシュとログはアプリの既定どおり`BMZ_DATA_DIR/cache`と`BMZ_DATA_DIR/logs`になります。
アプリと同じく、空のBMZ変数は未設定として扱います。
ランチャーは呼び出し元の作業ディレクトリを変えないため、相対パスの引数や相対パスのBMZ変数も同じ意味のまま使えます。
呼び出し元の`data/`ディレクトリは、パッケージの既定値より優先されません。
XDG変数には絶対パスを指定してください。

## 更新とFlatpakからの移行

BMZを終了し、新しい実行用アーカイブを新しいディレクトリへ展開して、その直下の`./bmz-player`を起動します。
既定の利用者データはパッケージの外にあるため、そのまま引き継がれます。
`BMZ_*`を明示している場合は、特に`BMZ_DATA_DIR`を同じ値のまま使ってください。
追加スキンは利用者データのディレクトリに置きます。
Linux tar.gz版の更新は手動です。Windowsのupdaterと、macOSのSparkleのfeedはこのパッケージには適用されません。

Flatpak版は`~/.var/app/net.hyrorre.BMZPlayer/`以下のsandbox用パスを使います。
そのため、配布形式を切り替えてもプロファイルやスコアは自動では移行されません。
移行する場合は、両方のバージョンを終了してそれぞれのデータをバックアップし、既存のファイルを確認してから、Flatpak版のデータディレクトリの中身をtar.gz版のデータディレクトリへコピーしてください。
ホスト側の譜面のパスやアクセス権限は、Flatpak環境と異なる場合があります。

## ビルドと検証

### ローカルでのパッケージ作成

DockerとGitがあり、同梱スキンのsubmoduleを初期化済みのLinux x86_64ホストで実行します。

```sh
git submodule update --init --recursive
scripts/package-linux-tar.sh
# rootless Podmanを使う場合:
CONTAINER_ENGINE=podman scripts/package-linux-tar.sh
```

先に、追跡しているファイルの変更をコミットしてください。
スクリプトはコミット済みのソースと、記録されたスキンsubmoduleのコミットだけをビルドします。
追跡していないファイル、ローカルでのスキン編集、認証情報、設定、DB、外部スキン、追加曲は含めません。
ホストの実行時データはコピーしません。
Gitで管理しているフォント・スキン・サンプル曲は、すべてそのまま含めます。
Docker / Podmanはビルド用イメージをキャッシュします。数GBのディスク容量、依存関係の取得に使うネットワーク、Rustのコンパイル時間が必要です。

`dist/linux-tar/`には次のファイルを出力します。

- `bmz-player-v<version>-linux-x64.tar.gz`：実行ファイル、ライブラリ、リソース、ライセンス表記
- `bmz-player-v<version>-linux-x64-sources.tar.gz`：対応するソースのworkspace、Cargoのvendor、FFmpegのアーカイブ、対応するUbuntuのソースパッケージ
- `SHA256SUMS.txt`：2つの圧縮ファイルのSHA256

すべての確認が成功すると、実行用アーカイブの隣に`validation.md`を書き出します。
ビルドしたコミット、サイズ、チェックサム、検証結果の要約を記録したローカル用のレポートで、配布する3つのファイルには含めません。

各アーカイブは、[GitHub Releasesのファイルごとの上限](https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases)である2,147,483,648 bytes未満にする必要があります。
ビルドはgzipのレベル9で圧縮し、どちらかのファイルが上限に達した場合は失敗します。
上限に収めるために、必要なソースやライセンス表記を削ることはありません。
確認に失敗した場合も、調査用にアーカイブを残します。コマンドが成功した場合だけ、検証済みとして扱ってください。
2つのアーカイブを再検証するには、実行用アーカイブの隣に`SHA256SUMS.txt`を置いて次を実行します。

```sh
scripts/package-linux-tar.sh --verify \
  dist/linux-tar/bmz-player-v<version>-linux-x64.tar.gz \
  dist/linux-tar/bmz-player-v<version>-linux-x64-sources.tar.gz
```

### 手動のworkflow

**Optional Linux tar.gz** workflowを手動で実行すると、検証済みの2つのアーカイブとチェックサムを1つのActions artifactとして取得できます。
すべての確認が成功した後、その実行で明示した出力パスだけをアップロードします。
job summaryには、サイズ、SHA256、完全なコミットID、検証結果を記録します。
GitHubでは、手動実行の前にworkflowがリポジトリの既定ブランチに存在する必要があります。
その後は、実行ダイアログで別のブランチを選べます。
詳しくは[GitHubの説明](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow)を参照してください。
この手動のworkflowはReleaseを公開しません。
検証後、パッケージ作成スクリプトはRelease連携用に`*-linux-x64-tar-client-manifest.json`も書き出しますが、手動実行のartifactに含めるのは2つのアーカイブとチェックサムだけです。

### 正式リリースのworkflow

`release-apps.yml`は、解決したリリースタグのコミットから同じ2つのアーカイブをビルドします。
アーカイブのバージョンとコミットをリリースメタデータと照合し、Linux tar.gz版のクライアントハッシュをWindows・macOS・Flatpak版のビルドとまとめます。
このハッシュは、`patchelf`を適用した後の検証済み実行ファイルから求めます。ランチャーやtarballのハッシュではありません。
両方のアーカイブと、署名付きの統合`release.json`を最終的な`SHA256SUMS.txt`へ含めます。
リリースには個別のclient manifestを添付しません。rianIRは`release.json`のトップレベルの`builds`を読みます。
詳しくは[Releaseメタデータ](release-metadata.md)を参照してください。
対応ソースは、実行用アーカイブと同じReleaseから取得してください。
GitHubが自動で作るソースのスナップショットには、vendorした依存関係やUbuntu / FFmpegのソースが含まれません。

`upload_to_release=false`を指定すると、workflowは検証した後、まとめたファイルを`verified-release-files`としてActionsへアップロードします。
この場合、Releaseのassetや更新用のfeedは公開しません。
一般へ配布する前に、実機でのGPU・音声・入力の動作を確認し、`docs/rian-ir.md`のとおりにrianIRのallowlistへの登録を依頼してください。

### ビルド段階

ビルド段階はUbuntu 22.04を使い、実際のコンパイラのバージョンを記録します。
バージョンを固定したFFmpegのソースを、GPL / nonfreeのコーデックや自動検出した外部コーデックを含めずにビルドします。
その後、release buildの前に`cargo fmt --check`、`cargo check`、`cargo clippy`、`cargo test`を実行します。
テストは、既存のUI文字列とファイル権限のテストに合わせて、日本語ロケールを既定とし、権限のないユーザーで実行します。
ビルドイメージには`dbus`を追加し、アイドル抑止テストが専用のD-Busを起動できるようにします。
一時的なディスクとメモリの使用量を抑えるため、テストのデバッグシンボルは省きます。
アプリは既存のPulseAudio featureを使います。ネイティブCPU向けの最適化は有効にしません。

### 実行用アーカイブの検証

検証では、実際の実行用tar.gzを展開し、それだけを読み取り専用で別のUbuntu 22.04コンテナへマウントして、権限のないユーザーで実行します。
このイメージには、ビルド段階のファイルシステム、コンパイラ、FFmpegがありません。

- 同梱したすべてのELFの依存関係が解決できることを確認します。glibc以外の依存関係は、ホストの連携用パッケージが同じものを提供していても、アーカイブ内で解決される必要があります。
- 対照として、ライブラリのディレクトリを除いた実行ファイルのコピーでは、FFmpegを解決できないことを確認します。
- help、相対パスでの曲スキャン、SQLiteへの書き込み、既定と上書きしたdata / cache / log / resourceのパス、シンボリックリンク経由のランチャー、Xvfb・ソフトウェアVulkan・null PulseAudio sinkでのサンプル曲のプレイを確認します。
- ファイルを開けたことで、同梱のスキン、CJKフォント、譜面、音声サンプルにアクセスできることを確認します。

この実行用コンテナからは、ソースのアーカイブや元のビルドツリーにアクセスできません。

### ビルド情報とソースの検証

両方のアーカイブには、同じ`build-manifest.json`（schema 1）が入っています。
次の内容を記録します。

- バージョン、コミット、submoduleのコミット
- target / features / コマンド、lockfileのハッシュ
- コンパイラのバージョン、Ubuntuのリリース、ビルド用イメージのID
- FFmpegのURL / ハッシュ / configure設定
- Ubuntuのバイナリパッケージとソースパッケージのバージョン

ファイルの一覧には、ハッシュ、サイズ、実行権限、シンボリックリンクの参照先を記録します。
コミット済みのスナップショットはビルド前に一覧を作り、配置後と展開後にソースとスキンのファイルをその一覧と比較します。
manifest自身は一覧から除きます。圧縮ファイルのハッシュは、外部のチェックサムファイルだけに記録します。

ソースのアーカイブは別のパスへ展開します。
その一覧と識別情報は、実行用アーカイブ、Cargo workspaceのバージョンとlockfile、FFmpegのソースとconfigure設定、Ubuntuのソースのバージョンと一致する必要があります。
すべての`.dsc`の参照について、サイズとSHA256を確認し、`dpkg-source`で展開します。
別のツール専用コンテナは、ソースのアーカイブだけをマウントし、空のCargoキャッシュでvendorの依存関係をオフラインで解決します。
そのうえで、ネットワークを無効にしてFFmpegとBMZのreleaseを再ビルドします。
この確認では、Ubuntuのライブラリをすべて再ビルドするわけではなく、バイト単位で同じバイナリを再現できることも保証しません。

### 検証できない項目

このsmoke checkでは、実際のGPUとの互換性、Waylandでの動作、音が実際に鳴るか、タイミング・遅延、物理コントローラーへのアクセスは確認できません。
一般向けのバイナリリリースを提案する前に、実機のUbuntu 22.04デスクトップで確認してください。

## ライセンス表記と対応ソース

`resources/licenses/`には、次のものが入っています。

- 既存のBMZと第三者のライセンス表記
- 必須のcargo-aboutのレポート
- Rustの実行時ライブラリのライセンス表記
- NotoのOFL
- FFmpegのライセンスとconfigure設定
- ビルドの出所情報

スキン固有のライセンス表記は、変更していない素材の隣に置きます。
非営利・改変禁止などの既存のスキンの制限は、引き続き適用されます。
詳しくはアーカイブ内の`resources/licenses/license-notes.md`（ソースツリーでは`docs/licenses.md`）を参照してください。

FFmpegは動的リンクし、ライブラリ名を変えずに同梱します。
検証済みの上流アーカイブそのものを、対応するソースのアーカイブの`ffmpeg/`に入れています。
記録したconfigureコマンドと`installer/linux-tar/build-ffmpeg.sh`に、再ビルドの方法が書かれています。
これは[FFmpegの配布チェックリスト](https://ffmpeg.org/legal.html)に従ったものです。

ほかの同梱ELFライブラリは、Ubuntuのパッケージから取り出しています。
それぞれについて、次のものを用意します。

- `resources/licenses/ubuntu/packages.txt`に記録したバイナリとソースのバージョン
- 元の著作権表示
- APTで取得し、ソースのアーカイブの`ubuntu/<source-package>/`に入れた`.dsc` / 上流 / Debianのソースアーカイブ

ソースやライセンス表記が欠けている場合は、パッケージ作成が失敗します。
パッチ適用済みのソースを展開するには、`.dsc`に対して`dpkg-source -x`を実行します。
ELF RUNPATHは同梱のパッケージ作成スクリプトで調整しますが、ライブラリのコードには変更を加えません。
これらのファイルは、将来のソース提供の申し出に頼らず、バイナリと一緒に配布します。
ホストのglibc / loaderとGPUドライバーは同梱しません。

ソースのアーカイブには、アプリのworkspaceがそのまま入っています。
実際に同梱したsubmoduleのファイル、スクリプト、lockfile、Cargoのvendorも含みます。
`.cargo/config.toml`は相対パスの`vendor`を使い、既存の設定はそのまま残します。
開発用パッケージ、オフラインでの再ビルドのコマンド、Ubuntuのソースの展開方法は、アーカイブ内の`BUILDING.md`（リポジトリでは[`installer/linux-tar/SOURCE-README.md`](../installer/linux-tar/SOURCE-README.md)）を参照してください。
再配布するときは、対応する2つのアーカイブとすべてのライセンス表記をそのまま保ってください。
同梱した依存関係のセキュリティ更新には、アーカイブの再ビルドと再検証が必要です。
キャッシュしたイメージのレイヤーを再利用せず、Ubuntuのパッケージと安定版Rust toolchainを更新するには、先に次を実行します。

```sh
docker build --pull --no-cache --target build -t bmz-linux-tar-build installer/linux-tar
```

Podmanでビルドする場合は、同等のPodmanのコマンドを使ってください。

## 検証記録

分割アーカイブ・単一アーカイブでの過去の検証結果、artifactのハッシュ、workflowの記録は[2026-09-15の作業記録](../notes/2026/2026-09-15-linux-tar-validation.md)にまとめています。
