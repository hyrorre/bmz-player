# Windows 8.1 x64 と Server 2012 R2 x64 での SxS 検証

この ZIP はローカル検証用です。各配置の起動入口はルートの `bmz-player.exe` です。
`test.ps1` は各配置の検証用コピーを作り、空の作業ディレクトリから実行します。

1. 検証専用の Windows 8.1 x64 または Server 2012 R2 x64 VM に ZIP を展開します。
   両 OS は 6.3 / build 9600 です。同梱 UCRT のロード条件の確認にはどちらも使用できます。
2. OS の build、更新一覧（KB2999226、KB3118401 など）、System32 の
   `ucrtbase.dll`、`vcruntime140.dll`、`api-ms-win-crt-runtime-l1-1-0.dll` の有無を記録します。
   最初は中央導入のない VM スナップショットで実行します。
3. 展開先を開いた 64-bit Windows PowerShell で次を実行します。出力先は毎回新規にします。

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\test.ps1 -Packages .\packages -OutputDirectory .\trial -RequireAppLocalUcrt -BgaChart .\video-fixture\sample-video.bms
```

GPU なしの環境では `-SkipRendering`、描画可能で音声デバイスがない環境では
`-SkipPlay` を指定します。CLI とロード元を先に確認できます。

CI のバイナリを検証する場合は、同じ候補コミットの Windows-only CI から取得した
`release-windows-x64` の portable ZIP を使います。ZIP 内の `BMZ Player` をそれぞれ
`packages/baseline`（flat）と `packages/sxs`（`windows_sxs=true`）へ展開し、上の手順を実行します。
転送前後で ZIP の SHA-256 が一致することを確認します。

保存する結果は `results.json`、`*.modules.csv`、`os.json`、`system-runtime.json`、
`hotfixes.csv`、`side-by-side-events.txt`、個別の stdout/stderr です。
中央導入のない build 9600 では現行配置でルート、全 DLL 移動版で `dll/` から
`ucrtbase.dll` と使用した CRT 転送 DLL がロードされることを確認します。
中央導入済みの場合は UCRT の実ロード元を記録し、中央導入のない試験と区別します。
中央導入済みの OS と、システム UCRT を使う Windows 10/11・Server 2022 の比較では
`-RequireAppLocalUcrt` を省きます。
manifest 欠落と version 不一致は起動エラー 14001、
必須 FFmpeg DLL 欠落は 0xC0000135（または Win32 エラー 126）を期待します。
同梱 `ucrtbase.dll` の欠落では、DLL 欠落に加え、CRT 転送先のエントリポイントを
解決できないエラー 0xC0000139（または Win32 エラー 127）も検査します。

結果が不明なケースは `sxstrace.exe Trace -logfile:<path> -nostop` を開始し、
対象 exe を別プロセスで起動してから `sxstrace.exe Stoptrace` と
`sxstrace.exe Parse -logfile:<path> -outfile:<text>` で記録します。
必要なら Windows Update や VC が導入済みの別スナップショットでも比較します。
起動不能な場合は最初に現行配置と SxS 配置のエラーを比べてください。

初期 PoC の `packages/sxs.files.json` は SxS ファイル一覧と SHA-256 です。
protocol 4 の検証用パッケージでは配布物内部の `updater/bmz-package.json` に全ファイルを記録します。
