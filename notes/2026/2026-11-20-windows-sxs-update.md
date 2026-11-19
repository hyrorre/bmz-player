# Windows SxS 配置の protocol 4 実装と更新・復旧試験

## 範囲

作業開始時は `codex/windows-sxs-poc` の `a098422e`。#137・#138 の互換ビルドと
[dll/ PoC](2026-11-19-windows-sxs-poc.md) を基点に、通常の包装処理へ検証用の
`-DllLayout sxs` を追加した。正式配布は flat を使用する。GitHub Actions の
`windows_sxs=true` は検証用で、SxS の Release アップロードを拒否する。

- SxS パッケージは x64 互換ターゲット、grouped updater、最低 protocol 4 を使う。
  flat・legacy の橋渡しパッケージは最低 1 で、収録する本体と helper は protocol 4。
- `dll/dll.manifest` と PE の埋め込み依存の identity、DLL 一覧を照合する。
  従来の import/export 検査には SxS で解決するファイルを渡し、VC/UCRT の同梱条件を維持した。
  ローカル FFmpeg 9.0.2 の `avfilter-12.dll` が要求する `msvcp140.dll` と
  `vcruntime140_1.dll` も同じ v143 再配布セットから含めた。
- 旧 manifest 管理下のルート DLL は既存のジャーナルに沿って退避する。新しい DLL と同名の未管理ファイルや
  探索を先取りする `dll.dll`、`dll.manifest`、外部アプリケーション manifest は適用前に拒否する。
- 更新確定前に新 exe の `--version` を実行する。失敗時は以前のファイルを復元し、設定や DB に触る前に
  loader の不具合を処理する。その後の通常起動や DB migration は既存の経路を使う。

## Windows 11 での検証

Windows 11 x64、`nightly-2026-11-19`、`x86_64-win7-windows-msvc`、MSVC v143。
ローカルの vcpkg `x64-windows` FFmpeg 9.0.2 と SDK の UCRT 10.0.10240.16384 を使った。
通常の包装スクリプトから flat と SxS を新規生成し、SxS は 52 DLL を `dll/` に収録した。
Windows 11 の起動試験では FFmpeg 5 本と VC を `dll/`、UCRT を `System32` からロードした。
Result 描画、CLI、portable / installer の保存先を確認した。

公開済みの v0.4.2・v0.4.3・v0.5.2 のタグから旧 helper をビルドして、次の試験を隔離コピーで行った。
通信・配信の代わりにテスト用バージョンのステージを用意し、実 helper の `READY` / `APPLY` と通常の再起動を使った。
更新情報の生成・署名と protocol 1〜3 の橋渡し選択は Node / Rust テストでも確認した。

| 旧 helper | 旧配置 → flat 橋渡し → SxS | SxS identity を壊した更新 |
|---|---|---|
| protocol 1（v0.4.2） | 成功、旧ルート DLL を退避 | エラー 14001、橋渡し版へ復旧・再起動 |
| protocol 2（v0.4.3） | 成功、旧ルート DLL を退避 | エラー 14001、橋渡し版へ復旧・再起動 |
| protocol 3（v0.5.2） | 成功、旧ルート DLL を退避 | エラー 14001、橋渡し版へ復旧・再起動 |

各経路で、同梱 DLL 52 本と試験用の廃止 DLL 1 本を退避し、適用前の実ファイルと SHA-256 を照合した。
テストで改変した管理対象 DLL の内容も保持され、
対象外の DLL、スコア、追加スキンの検証ファイルは変更されなかった。復旧では旧 exe、helper、
manifest と管理対象ファイルを実測した。中断後のジャーナル復旧と未管理の同名ファイルは Rust テストでも確認した。

新規 installer は起動し、ルート DLL は 0 本。旧 installer への上書きでも起動し、
テスト用ユーザーデータを保持した。上書きでは旧 DLL 52 本が残る現行動作を確認した。
portable の退避と異なり、installer の上書きに旧 DLL の除去は追加していない。

最終版で workspace の check、all-targets Clippy、test を確認した。テストは 5,811 成功、失敗 0、144 件除外。
初回のテストで不足していた同梱スキン資材は作業用 worktree に補った。
Node の署名・包装テスト 14 件、PowerShell 構文、workflow の actionlint も確認した。

## Windows 8.1 で残る確認

登録済み VirtualBox VM のディスクは 2 MiB で、起動ログは `No bootable medium found`。
起動可能な Windows 8.1 が手元になく、中央導入なしの app-local UCRT は未確認。
[互換ビルドの検証手順](../../docs/windows-compat.md#sxs-版の検証) と
[持ち込み手順](../../scripts/windows-sxs-poc/VM-README.md) に従い、ロード元を確認してから公開条件を見直す。
最終候補から作った持ち込み用 ZIP は `bmz-player-windows81-sxs-protocol4-validation-final.zip`。
標準スキンとサンプルに絞った比較用パッケージで、Windows 11 で展開後の起動と DLL ロード元を確認した。
ZIP、SHA-256、更新・復旧のログ（`release-proof-1.log` から `release-proof-3.log`）は
`.local/windows-sxs-adoption/` に保存した。

## 2026-11-20 追試 — Windows Server 2012 R2

protocol 4 実装（`6b3cae8b7776ac21ae89d290c61a3e0a21ead73c`）をコミットする直前の作業ツリーで
生成した比較用 ZIP を、Microsoft 公式の Server 2012 R2 Standard Evaluation x64 の隔離 VM で試験した。
実行ファイルのビルド情報には作業開始時の `a098422e` と `-dirty` が記録されている。
OS は 6.3 / build 9600、`BuildLabEx` は `9600.17031.amd64fre.winblue_gdr.140221-1952`。
KB2919355 はインストール済み。KB2999226 と KB3118401 は更新一覧になく、System32 に
`ucrtbase.dll`、`vcruntime140.dll`、`api-ms-win-crt-runtime-l1-1-0.dll` はなかった。
VM のネットワークを無効にし、空の作業ディレクトリと OS のみの PATH を使用した。
[OS バージョン表](https://learn.microsoft.com/en-us/windows/win32/sysinfo/operating-system-version)、
[UCRT 配置仕様](https://learn.microsoft.com/en-us/cpp/windows/universal-crt-deployment)

| 試験 | Server 2012 R2 の実測 |
|---|---|
| 従来配置 | ルートから対象 DLL 18 本をロード |
| クリーンな `dll/` 配置 | ルート DLL 0 本、`dll/` から対象 DLL 18 本をロード。従来配置と全 18 本の SHA-256 が一致 |
| CLI・日本語と空白を含む配置・移動 | CLI の stdout/stderr と終了コード、updater の起動、移動前後の portable プロファイル、installer マーカーでの Roaming 保存を確認 |
| 旧配置への上書き | 旧 DLL がルートに残存しても、対象 18 本のロード元は `dll/` |
| manifest 欠落・identity 不一致 | いずれも Win32 14001。SideBySide イベントにアセンブリ解決失敗を記録 |
| `avcodec-63.dll` 欠落 | `0xC0000135` で起動失敗 |
| `ucrtbase.dll` 欠落 | `0xC0000139` で起動失敗。ゲストの Windows ダイアログで `swresample-7.dll` の `_cexit` 解決失敗を確認 |
| 選曲・Result・入力初期化 | VirtualBox 7.2.22 の 3D を有効にして OpenGL 4.1 で描画。選曲と Result は両配置、移動後の Result は `dll/` 版で各 120 フレーム。Raw Input の初期化をログで確認 |
| サンプルプレイ・動画 BGA | 両配置で仮想 WASAPI 出力を開けず、プレイと BGA の確認は未完了。音声デコードは試験ログに `decoded_sources=1` を確認 |

`dll/` からロードされたのは FFmpeg 9.0.2 の 5 本（`avcodec-63.dll`、`avformat-63.dll`、
`avutil-61.dll`、`swresample-7.dll`、`swscale-10.dll`）、VC 14.44.35211.0 の
`vcruntime140.dll`、UCRT 10.0.10240.16384 の `ucrtbase.dll` と CRT 転送 DLL 11 本。
`ucrtbase.dll` の SHA-256 は `2ab2a74bcb5bfd8248d232eb3bc56698fb5173b9ff7fc0daf87d8120d0f448d7`。
名前・絶対パス・version・ハッシュの明細は `.local/windows-sxs-server/local-package-gate/` に保存した。
描画を含む再試験は `-SkipPlay -RequireAppLocalUcrt` で終了コード 0。
クリーンな build 9600 での app-local UCRT ロード条件を確認した。Windows 8.1 クライアント機は未試験。

### Windows Server 2022 の GitHub Actions

[Windows-only CI](https://github.com/hyrorre/bmz-player/actions/runs/44583190074) は
`6b3cae8b7776ac21ae89d290c61a3e0a21ead73c`、`nightly-2026-11-19`、
`x86_64-win7-windows-msvc`、FFmpeg 9.0.2#7 (`x64-windows-release`)、MSVC v143 で成功した。
包装時の import/export 検査は 2,106 imports、updater のテストは 23 件成功。
installer のファイル保護、保存先、CLI と updater の起動、DX12 のソフトウェア描画で Auto / Raw Input の
Result 各 120 フレームを確認した。FFmpeg 5 本と VC 14.44.35211.0 は `dll/`、
UCRT 10.0.20348.5756 は `C:\Windows\System32` からロードされた。
同梱 UCRT 10.0.10240.16384 は配布物に含まれる。Release へのアップロードは実行していない。

### 同じ CI 成果物での Server 2012 R2 照合

CI の portable ZIP（SHA-256 `4cd19992c2c2186e4b2b8f9152a24c00a3ed0adc1e7bceddd7f4b9798965dae6`）を
VM に転送し、ZIP の SHA-256 と展開した exe の client hash を照合した。ZIP の本体 exe はルートに 1 本、
ルート DLL は 0 本、`dll/` には DLL 51 本と `dll.manifest` がある。
仮想 GPU を使った再試験では flat 比較側の起動中に VirtualBox が異常終了した。
3D を無効にし、同じ ZIP を新たに展開して `-SkipRendering -RequireAppLocalUcrt` で再試験した。
CLI、保存先、updater、移動、故障ケースと overlay で合格。
`dll/` から FFmpeg 5 本、VC、UCRT 10.0.10240.16384、CRT 転送 DLL 11 本を実ロードした。
FFmpeg と VC の SHA-256 は Server 2022 の実ロード結果と同じだった。
raw CSV・JSON・イベントログは `.local/windows-sxs-server/server2012r2-ci-gate/` と
`.local/windows-sxs-server/server2022-startup/` に保存した。

この結果で `dll/` を正式配布に使える構成として採用した。公開用の `BMZ_WINDOWS_DLL_LAYOUT=sxs` は、
旧クライアントが読める protocol 4 橋渡し版を先に公開してから設定する。
追試時点で公開済みの最新版は v0.5.2 で、橋渡し版は未公開。
候補のアーカイブと結果は CI artifact で確認し、公開用タグ・更新フィード・repository variables は変更していない。

### Installer のルート DLL 退避

`codex/windows-8.1-compat` の `44a228a9` を基点に、SxS 用 Setup の上書きも退避に対応した。
旧 installer の有効な manifest をインストール前に記録し、新しい本体と `dll/` のハッシュを
インストール後に確認する。旧 manifest のファイル名と SHA-256 が一致する直下 DLL を
`updater/installer-dll-backup/` へ移す。以前の manifest と退避 DLL は再実行・復旧用に保持する。
Setup は保持したファイルや退避できなかったファイルをログに記録する。

- Inno Setup 6.7.3 の隔離 fixture で、初回導入、ハッシュ不一致時の旧 DLL 保持、
  共有ロック中にほかの DLL を退避、ロック解除後の再実行、再インストール、アンインストールを確認した。
  変更済み DLL、追加 DLL、スキン、設定、スコア用ファイルを上書き時に保持し、退避 DLL はアンインストール後も残った。
- 実行ファイルと同梱ランタイムを含む比較パッケージを Inno Setup 6.7.3 で再包装し、Windows 11 build 26300 と
  中央導入の VC/UCRT がない Server 2012 R2 build 9600 で `flat → dll/` を試験した。
  空白・日本語を含むパスへ上書きし、両環境で 52 本の旧 DLL の退避前後の SHA-256 が一致した。
  管理対象 DLL はルートに残らず、追加 DLL とデータを保持し、空の作業ディレクトリから更新後の `--version` が成功した。
  本体は既存のローカル比較ビルド、退避 helper は新規に `nightly-2026-11-19` の
  `x86_64-win7-windows-msvc`・CRT 静的リンクでビルドした。
- Rust updater の check、Clippy、25 テスト、更新メタデータ 14 テストを確認した。
  公開 workflow の橋渡し条件は、installer が不足する場合と旧 protocol から読めない場合も拒否することを試験した。
- 通常の `package-windows.ps1` の SxS 経路で import/export 検査とフル Setup の生成も確認した。
  上書き試験では別 AppId の比較用 Setup を使用した。

Windows 8.1 クライアントと公開済みの更新フィード経由の installer 更新は未試験。
生ログと検証素材は `.local/installer-dll-real/` と `.local/installer-dll-test-temp/` に保存した。
