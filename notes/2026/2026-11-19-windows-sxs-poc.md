# Windows配布のDLLをプライベートSxSアセンブリへまとめる検証

## 判断

**条件付きで採用可能。** Windows 11 x64 では、配布ルートの `bmz-player.exe` に埋め込んだ manifest で、同梱 DLL 52 本を `dll/` に配置して起動できた。必要な FFmpeg 5 本と `vcruntime140.dll` のロード元も確認した。中央導入のない Windows 8.1 での UCRT 検証と、配布・更新処理の変更が採用条件になる。

## 対象と環境

- 開始時の worktree は detached HEAD `ab1808788f5d6dcc639cd6013b370463bccdda3e`。local / remote `main` も同じコミットだった。確認時点で [#137](https://github.com/hyrorre/bmz-player/pull/137) は open、[#138](https://github.com/hyrorre/bmz-player/pull/138) は #137 のブランチへマージ済み。両方を含む `2d3d96754ed2cf7ce3fc0c63a9511a1460ae6ad2` をビルド対象にした。
- Windows 11 Pro x64 26H2、build 26300.9974。NT のレジストリ上の `ProductName` は Windows 10 Pro と表記される。Windows 10 の別環境では実行していない。
- Rust `nightly-2026-11-19`、`1.102.0-nightly (65ce8611f 2026-11-18)`、release / default features / `x86_64-win7-windows-msvc`、`-Z build-std=std,panic_abort`。MSVC v143 14.44.35207、Windows SDK の `mt.exe` 10.0.26100.0、ASIO SDK 2.3.4。updater は同じターゲットで CRT を静的リンクした。nightly で既知の wgpu 型の `recursion_depth_exceeding_limit` 警告が出た。
- FFmpeg 9.0.2（ローカル vcpkg `x64-windows`、port revision 1）と pkgconf 3.0.3（同 revision 1）。#137 の CI は `x64-windows-release` の FFmpeg 9.0.2 port revision 7。今回の比較はローカル DLL セットで固定した。[#137 の CI 記録](2026-11-19-windows-81-compat.md)
- SDK の app-local UCRT は 10.0.10240.16384（`ucrtbase.dll` と API 転送 40 本）。VC は VS 2022 の 14.44.35211.0。現行包装でコピーされたローカル DLL 50 本では、#138 の検査が `avfilter-12.dll` の `MSVCP140.dll` 欠落を検出した。同ファイルの import 表にある `VCRUNTIME140_1.dll` と合わせ、同じ v143 再配布元から比較入力に追加した。既存検査で 2,177 件の同梱 import/export と updater の CRT 非依存を確認した。
- 対象の Git archive を worktree 内の別ディレクトリに展開した。初回は `build.rs` が展開先を囲む Git HEAD を表示した。最終版は `BMZ_BUILD_COMMIT_OVERRIDE` で展開元の SHA を指定して再ビルドした。

## manifest と配置

`build.rs` の `winres` はアイコンとバージョン情報を生成する。完成した exe の `mt.exe -inputresource:<exe>;#1` は `c101008c`（リソースの種類がない）だった。`llvm-readobj --coff-resources` でも ICON 6、GROUP_ICON 1、VERSIONINFO 1 の計 8 リソースで、RT_MANIFEST（type 24）は存在しなかった。

SDK の `mt.exe` で未署名の検証用 exe コピーの `RT_MANIFEST #1` に次の内容を埋め込んだ。元の exe に実行レベル・対応 OS・DPI・long path の明示設定はなく、PoC でもそれらを指定しない。再抽出した XML を正規化して照合し、既存 PE リソースのデータと GUI subsystem、OS/subsystem version 6.00 が一致することを確認した。

```xml
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity type="win32" name="BMZ.Player" version="1.0.0.0" processorArchitecture="amd64"/>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="dll" version="1.0.0.0" processorArchitecture="amd64"/>
    </dependentAssembly>
  </dependency>
</assembly>
```

```text
BMZ Player/
  bmz-player.exe
  dll/
    dll.manifest
    （FFmpeg 7 本、pkgconf 1 本、VC 3 本、UCRT と転送 DLL 41 本）
  resources/
  updater/
    bmz-updater.exe
    bmz-package.json
    instance.lock
```

[アセンブリ manifest のテンプレート](../../scripts/dll.manifest) が同じ name・type・version・processorArchitecture を定義する。生成時に `<file name="avcodec-63.dll"/>` のような実ファイル名を 1 本ずつ追加する。準備スクリプトは許可位置、宣言と DLL の一覧、同名や過不足、移動前後の SHA-256 を照合する。現行配置はルート 52 本、FFmpeg だけを移した途中段階はルート 45 本、全 DLL 移動版はルート **0 本**。

Windows は共有アセンブリを検索した後、exe の隣と同名子フォルダーで `<assemblyname>.dll`、`<assemblyname>.manifest` を探す。初回の `BMZ.Player.Runtime` から、フォルダー名、manifest ファイル名、両側のアセンブリ名をまとめて `dll` に揃え、`dll/dll.manifest` が選ばれる構成にした。API set の解決に続いて SxS リダイレクトが働く。依存 DLL の間接 import もモジュール名で解決される。Windows 10/11 は OS の UCRT を使い、API set は実ファイルのロードを伴わないことがある。Microsoft が記す Windows 8 より前のサブフォルダー制限は、8.1 での成功を保証しない。

- [Assembly Searching Sequence](https://learn.microsoft.com/en-us/windows/win32/sbscs/assembly-searching-sequence)
- [Application manifests](https://learn.microsoft.com/en-us/windows/win32/sbscs/application-manifests)
- [Assembly manifests](https://learn.microsoft.com/en-us/windows/win32/sbscs/assembly-manifests)
- [Using Side-by-Side Assemblies](https://learn.microsoft.com/en-us/windows/win32/sbscs/using-side-by-side-assemblies)
- [Dynamic-link library search order](https://learn.microsoft.com/en-us/windows/win32/dlls/dynamic-link-library-search-order)
- [Universal CRT deployment](https://learn.microsoft.com/en-us/cpp/windows/universal-crt-deployment)

## ロードと起動の結果

試験はプレイヤーの設定やスコアを持たない新規コピーを使った。すべての起動はパッケージと別の空ディレクトリから行い、子プロセスの PATH を Windows システムディレクトリに限定した。FFmpeg と VC のロード元をプロセスの Modules で実測し、ファイルの version と SHA-256 を採取した。

`dumpbin /dependents` は exe の直接 import に avcodec、avformat、avutil、swscale と VC を列挙した。`swresample-7.dll` は avcodec の間接 import で、同じ private assembly からロードされた。

| 対象 | 想定（全 DLL 移動版） | Windows 11 実測 |
|---|---|---|
| `avcodec-63.dll`、`avformat-63.dll`、`avutil-61.dll`、`swresample-7.dll`、`swscale-10.dll` | private assembly | すべて `dll/`。現行は exe 横、FFmpeg のみ移動版も assembly |
| `avdevice-63.dll`、`avfilter-12.dll`、`pkgconf-8.dll` | 使用時は private assembly | 実行経路でロードされず。同梱と import/export は確認 |
| `vcruntime140.dll` | private assembly | `dll/`、14.44.35211.0。現行と FFmpeg のみ移動版は exe 横 |
| `msvcp140.dll`、`vcruntime140_1.dll` | 使用時は private assembly | 実行経路でロードされず。同梱と import/export は確認 |
| `ucrtbase.dll` | 11 は System32、中央導入なしの 8.1 は同梱先を検証 | `C:\Windows\System32\ucrtbase.dll`、10.0.26100.9711 |
| UCRT の API 転送 40 本 | 11 は API set 解決が優先、8.1 は必要な転送 DLL を同梱先から解決できるか検証 | 個別のファイルモジュールは一覧に現れず |

全 DLL 移動版の実ロード元と SHA-256 は `.local/windows-sxs/dll-layout/trial/sxs-result.modules.csv`。FFmpeg の DLL に PE FileVersion がないため、バージョンはリンクに使ったローカル vcpkg のヘッダーと provenance による。

| 実ロードファイル | バージョン | SHA-256 |
|---|---|---|
| `avcodec-63.dll` | 63.1.102 | `bdc5e1ac1bb9eeac17c5fd07db0bb00d97861624976f6fa572a2e731875ac2e0` |
| `avformat-63.dll` | 63.1.102 | `e2ecfe3c2b060f0883d356ba2742fb78c8a30d00eadd15c669672b7ed64f5bbe` |
| `avutil-61.dll` | 61.1.102 | `cf632de9bcd3e82b1c87ad1487646fd89b67d57a45e79c76ff939dfb699dea6a8` |
| `swresample-7.dll` | 7.1.102 | `b52435e4de0e969bdd61ba20ff076ec64ab2d381d86f31d91f920c88ed5c7dc0` |
| `swscale-10.dll` | 10.1.102 | `cc0bc1808d8b0b966ce1a6e6e74df959d515857002185888ee4481559f4a1241` |
| `vcruntime140.dll` | 14.44.35211.0 | `d5e4d9a3e835fa679450145d6a7d94e36573a509317111904d9b3712c30d9066` |
| OS の `ucrtbase.dll` | 10.0.26100.9711 | `dedb7611e1abc1ff3852dcac3d4c93a048d729e25b061973b9969f22698f14ca` |

| ケース | Windows 11 x64 | Windows 8.1 x64 |
|---|---|---|
| 現行／FFmpeg のみ／全 DLL 移動、クリーン | CLI、選曲起動、Raw Input 指定の Result 描画が成功 | 未実施 |
| SxS 構成を空白・日本語を含むパスから別フォルダーへ移動 | 隣接 profile と Result が動作。DLL も移動先からロード | 未実施 |
| サンプルと動画付きサンプル | Autoplay、WAV と MP3 のデコード、WASAPI 48 kHz の出力コールバックとキー音コマンドを確認。動画デコーダと RGBA scaler が動作し、SxS のプレイ画面に動画を確認 | 未実施 |
| assembly manifest 欠落 | Win32 14001、SideBySide イベント 33。sxstrace で assembly が見つからない | 未実施 |
| assembly version 2.0.0.0 と exe が要求する 1.0.0.0 の不一致 | Win32 14001、イベント 35。sxstrace に両 identity の差異 | 未実施 |
| assembly manifest に記載した `avcodec-63.dll` を削除 | NTSTATUS 0xC0000135。CLI 実行に到達しない | 未実施 |
| 同梱 UCRT 41 本と対応する `file` 宣言を削除 | 初回のアセンブリ名で、別コピーの `--version` が終了コード 0 | 実施対象外。中央導入なしで同梱 UCRT を確認する |
| 現行配置に SxS を上書きするシミュレーション | profile 起動、FFmpeg/VC は SxS からロード。ルートに旧 DLL 52 本が残った | 未実施 |

Windows 8.1 の VirtualBox VM は登録済みだったが、ディスクに起動可能な OS がなく、前回ログも `No bootable medium found` だった。該当のローカルデータに起動メディアはなかった。Windows 10 の別環境での起動と 8.1 の app-local UCRT は未確認。

コンソール回帰試験では、現行と全 DLL 移動版で親コンソール、コンソールなし、PowerShell、cmd、パイプとファイルへのリダイレクトを確認した。`--help` と `--version` は終了コード 0、不正な gauge は stderr と終了コード 1。既存の互換起動試験は現行配置で Auto と Raw Input の各 120 Result frames を描画した。PoC は実際の DirectX 12 / NVIDIA GeForce RTX 5090 と WASAPI / RME Babyface Pro を使用した。物理出力の聴取やコントローラー実操作は未実施。

## 保存先・配布・更新への影響

- [paths.rs](../../crates/bmz-player/src/paths.rs) は `current_exe()` の親の package manifest を検証してから保存先と `resources/` を決める。許可されるファイルだけの PoC 用 marker で portable の隣接 `data/` と installer の分離した AppData を実測した。全ファイルを記載した marker は `profile create` が終了コード 1 で拒否した。全ファイルの別 inventory は `sxs.files.json`。
- [generate-update-metadata.mjs](../../scripts/generate-update-metadata.mjs) の package 生成と、[manifest.rs](../../crates/bmz-updater/src/manifest.rs) の `managed_path()` は新しい `dll/` 配置を扱えない。採用時は `dll/dll.manifest` と、その宣言にある直下の DLL だけを許可する。archive・更新・復旧で同じパス規則を使い、余分な階層、reparse point、大小文字違いの重複を拒否する。
- [verify-windows-compat-imports.ps1](../../scripts/verify-windows-compat-imports.ps1) はルートの DLL を前提にする。許可位置と manifest の identity / `file` 一覧を照合して DLL 名→実ファイルの表を作り、import/export に使用する。#138 の VC/UCRT 同梱必須判定と updater の CRT 非依存判定を維持し、実行試験でロード元も確認する。
- exe の manifest 埋め込み、必要な Authenticode 署名、[IR client hash](../../scripts/generate-client-hash-manifest.mjs) と package のファイルハッシュ、installer・ZIP の生成、アーカイブハッシュと署名付き release metadata の順で確定する。`dll` 名の SxS 版の client hash は `.local/windows-sxs/dll-layout/client-hash.json` に記録した。PoC の値は IR へ登録していない。
- [Inno Setup](../../installer/inno/bmz-player.iss) の再帰コピーはサブフォルダーを配置でき、上書きでは旧 DLL が残る。既存ファイル保護試験は 6.7.3 のダミー installer で成功した。SxS 版の実 installer による移行は未実施。採用時には旧 package manifest に記録された管理済みファイルに削除・退避対象を限定する。変更済みの DLL、同名ユーザーファイル、書き込み不可、中断時の復旧も確認する。
- [transaction.rs](../../crates/bmz-updater/src/transaction.rs) は旧 inventory と新 inventory の差分を取り、旧ファイルをバックアップする。旧 DLL・以前の exe・package manifest を同じトランザクションで戻せることを検証する。protocol 3 の reader も新パスを拒否するため、現行配置のまま新 reader / helper を先に届ける橋渡し版が必要。新配置の最低 protocol は 4 が候補。旧 1/2/3 からの橋渡し経路と署名済みメタデータを確認する。[自動更新](../../docs/auto-update.md)

## 再現手順

別 checkout に対象コミットとサブモジュールを用意し、[Windows互換ビルド](../../docs/windows-compat.md) に沿って v143、LLVM、Node.js、FFmpeg、ASIO SDK を準備する。archive 展開の場合は `BMZ_BUILD_COMMIT_OVERRIDE` を対象 SHA に設定する。PowerShell 7 で実行する。

```powershell
./scripts/build-windows-compat.ps1 -Toolchain nightly-2026-11-19
./scripts/package-windows.ps1 -SkipBuild -Target x86_64-win7-windows-msvc `
    -UpdaterLayout grouped -OutDir .local/windows-sxs/package -DllDir $dllDir
```

現行配置は `scripts/test-windows-compat.ps1` でも実行した。`scripts/test-windows-package-paths.ps1` では portable と installer の保存先・override・異常時の分離を確認した。

今回の DLL セットでは `msvcp140.dll` と `vcruntime140_1.dll` が追加で必要だった。コピー元は同梱 `vcruntime140.dll` と SHA-256 が一致する v143 の `x64/Microsoft.VC143.CRT`。両方の名前・version・SHA-256 を比較入力の `resources/licenses/windows-runtime.json` に追加し、`node scripts/generate-update-metadata.mjs package <staging> portable windows-x64 0.5.2 grouped` で比較入力を再集計する。

```powershell
./scripts/windows-sxs-poc/prepare.ps1 -PackageDirectory '.local/windows-sxs/package/BMZ Player' `
    -OutputDirectory .local/windows-sxs/prepared -Mt $mtPath -ReadObj $llvmReadobjPath
./scripts/windows-sxs-poc/test.ps1 -Packages .local/windows-sxs/prepared `
    -OutputDirectory .local/windows-sxs/trial
```

`-Mt` は SDK の x64 `mt.exe`、`-ReadObj` は `llvm-readobj.exe`。出力先は毎回新規にする。音声デバイスがない環境では `-SkipPlay`、GPU が使えない環境では `-SkipRendering`。動画付き譜面は `-BgaChart <bms-path>` で指定する。Windows 8.1 VM での確認は [持ち込み手順](../../scripts/windows-sxs-poc/VM-README.md) を参照。

動画素材は FFmpeg CLI で生成した。出力した `bga.mp4` と `key.wav` をサンプル BMS のコピーと並べ、末尾に `#BMP01 bga.mp4` と `#00104:01` の二行を加える。

```powershell
ffmpeg -f lavfi -i 'testsrc2=size=320x240:rate=30' -t 9 -c:v mpeg4 -q:v 4 -an bga.mp4
```

生ログ、生成バイナリ、`dll/` 版の検証用 ZIP（`bmz-player-sxs-dll-2d3d9675-windows81-x64-poc.zip`）と SHA-256 は `.local/windows-sxs/` に置き、Git では管理しない。ZIP は標準スキン、同梱サンプル、動画素材と各配置を含み、リリースの追加スキンは省いた。展開した ZIP を Windows PowerShell で再試験し、33 回の起動・失敗試験で期待どおりの結果を得た。`dll/` 版の SxS 探索トレースは `dll-layout/sxs.txt`。初回の構成で確認した動画画面は `visual/sxs-video-bga.png`、MP3 試験は `audio-ffmpeg/stderr.txt`。
