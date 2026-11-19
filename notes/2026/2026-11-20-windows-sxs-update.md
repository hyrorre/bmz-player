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
[互換ビルドの検証手順](../../docs/windows-compat.md#sxs-版の採用前検証) と
[持ち込み手順](../../scripts/windows-sxs-poc/VM-README.md) に従い、ロード元を確認してから公開条件を見直す。
最終候補から作った持ち込み用 ZIP は `bmz-player-windows81-sxs-protocol4-validation-final.zip`。
標準スキンとサンプルに絞った比較用パッケージで、Windows 11 で展開後の起動と DLL ロード元を確認した。
ZIP、SHA-256、更新・復旧のログ（`release-proof-1.log` から `release-proof-3.log`）は
`.local/windows-sxs-adoption/` に保存した。
