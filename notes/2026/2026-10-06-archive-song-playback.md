# ZIP・RAR・7z内楽曲の直接再生

## 目的と参照

2026-10-06の依頼により、楽曲フォルダ内のZIP・RAR・7zをスキャンし、内部譜面を通常のSelectから
選択・再生できるようにする。手動展開は不要とし、音声・動画の既存ファイルAPIへ渡す際には
BMZのcache内に展開する。

beatorajaの `d9ab59be`（SongArchive）、`721856fb`（SongResource）、`adcde8f3`（Explorer）を参照。
`.local/beatoraja/src/bms/player/beatoraja/song/archive/` に実装がある。
[先行する互換修正](2026-10-06-upstream-compatibility-fixes.md)と
[任意演奏設定](2026-10-06-optional-play-behaviors.md)に続く追加実装で、main上で進める。

## 実装方針

- DBの譜面pathは `archive!/entry` 形式の安定locatorとし、一時展開先をsource identityにしない。
- 譜面SHA-256/MD5は書庫内の元bytesから計算する。既存のスコア・Replay・Practiceのidentityを利用する。
- 元書庫の場所で楽曲rootの有効性を判断し、同hashの別コピーを選んだ場合も、そのコピーの素材を利用する。
- 譜面のbytes読込と全素材の展開を分ける。展開先は呼出側から渡す `AppPaths.cache_dir` 配下とし、
  テストでは独立したtempを利用する。
- cacheは書庫内容を表す世代ごとに作成し、完成前のstagingを公開しない。実行中の世代を自動削除しない。
- CLI/Viewerの譜面pathにも `archive!/entry` を指定できるようにする。書庫だけを指定した場合は
  曖昧な譜面選択をせず、楽曲スキャンを案内する。

## backendの選定と事前検証

| 形式 | 採用構成 | 選定根拠 |
|---|---|---|
| ZIP | 既存 `zip 4.6.1` | updaterでも利用しているRust実装 |
| RAR | `rars 0.10.0`、default features無効 | [公式公開版](https://github.com/bitplane/rars/tree/v0.10.0)。Apache-2.0、外部ツール不要の読み取り実装 |
| 7z | `sevenz-rust2 0.22.2` | 既存0.21.3から既知のoverflow修正とcoder properties公開を含む版へ更新。[変更履歴](https://github.com/hasenbanck/sevenz-rust/blob/main/CHANGELOG.md) |

2026-10-06、製品依存を変更する前に `.local/archive-probe/` の独立crateで検証した。

- Windows/MSVCでrarsのread-only構成をビルド。write/recovery/encryption/parallel機能が無効であることを確認。
- 実曲RAR4（v29、非solid、非分割）の全1,040ファイル、51,555,355 bytesについて、
  rarsと7-Zip 26.03のファイル集合・サイズ・SHA-256が全一致。
- 自作RAR4/RAR5 solid各6ファイル、295,108 bytesについて、原データ・7-Zip・rarsが全一致。
  日本語Unicode名、空ファイル、後続で辞書を共有するpayloadを含む。

UnRAR/junrarの固有ライセンス条項と、libarchiveのRAR4 solid制限も比較した。
今回の依存には採用せず、上流での採用実績だけをBMZのライセンス整合性の根拠にしない。
参照: [junrar LICENSE](https://github.com/junrar/junrar/blob/master/LICENSE)、
[libarchive RAR decoder](https://github.com/libarchive/libarchive/blob/master/libarchive/archive_read_support_format_rar.c)。

7zではBMZ側でentry/path/count/圧縮・展開bytesと本文dictionaryを検証する。
compressed-headerのdecode段階は公開APIで厳密なRAM上限を指定できないため、
全decoderメモリが制限されるとは扱わない。独自decoderやOS別sandboxは今回追加しない。

## 実曲検証データ

保存先: `G:/BMS/OTHERS/BMZ-archive-validation/`。
曲は「運命論 / ルゼ ☆ えみゅう。 ☆ 石王マサト」。
[作者の配布ページ](https://www.luzeria.net/?p=387)からの
[公式RAR](https://www.luzeria.net/music/71_unmei_ogg.rar)を利用する。

| ファイル | 由来 | SHA-256 |
|---|---|---|
| `71_unmei_original.rar` | 公式DLを変更せずコピー | `7B73B588652BAF641E59183AFB4B41CFACB27C6F6C0CA60178A7A8CC80AFE802` |
| `71_unmei_repacked.zip` | 同じ素材を7-Zipでローカル再梱包 | `32E78938EE25E02601497B08D541C89A1DD1006DD762151828037BC8466EDA20` |
| `71_unmei_repacked.7z` | 同じ素材を7-Zipでsolid再梱包 | `8817DF38120B75362974DF8B13C370C23989333B235178EAF17EB46F45A0FA58` |

各書庫は同じ4つのBMEと音声・画像・動画を含む。第三者の曲データはGit管理しない。
出典と再梱包の区別は同フォルダの `README_BMZ_ARCHIVES.txt` にも記録した。
先行する空白区切り検証用PMSは `G:/BMS/OTHERS/71_unmei_ogg` 側にあり、この3書庫には含めていない。

## 統合検証

2026-10-07、Windows/MSVCで検証を継続した。

- 製品の `song_archive` / `ChartLocator` APIで3形式を展開し、各1,040ファイル・51,555,355 bytesの
  SHA-256が元の展開データと全一致。各4譜面のbytes読込・一括スキャン・通常ファイルimportで
  譜面identityが一致し、各書庫のMPG BGAから256×256のフレームをdecodeできた。
- 独立した `BMZ_DATA_DIR` / `BMZ_CACHE_DIR` / `BMZ_LOGS_DIR` でCLIスキャン。
  書庫内12譜面（4種類のhash）と同梱sampleを登録し、failed=0。
  `chart_files` は `G:/BMS/OTHERS/BMZ-archive-validation/...!/71_unmei_ogg/...` を保持し、
  成功世代は3書庫分だった。実利用のDB・profileは変更しない。
- `cargo fmt --check`、`cargo check --workspace --locked` 成功。
- `cargo test --workspace --locked --no-fail-fast` で3,897 passed / 3 failed / 30 ignored。
  3失敗はCourse metricsのsource同一性check漏れ1件と、Windows拡張path prefixの比較方法2件だった。
  条件の復元と双方canonicalizeによる比較へ修正後、`cargo test -p bmz-player --lib --locked` は
  2,291 passed / 0 failed / 19 ignored。ほかのcrateと合わせ3,900 passed / 0 failed / 30 ignored。
  子プロセスの同一テスト1件は集計へ重複加算しない。今回追加した回帰は39件。
- 最終 `cargo build -p bmz-player --bin bmz-player --locked` とfmt確認は成功。
- `cargo clippy --workspace --all-targets --locked -- -D warnings` は既存の
  `input/capture.rs:111` の `clippy::let_unit_value` で失敗。今回の差分にエラーは報告されなかった。
  backend独立harnessのClippyは成功。
- backend独立harnessは15 passed / 0 failed。ただしcache symlink回帰1件はWindowsの
  symlink作成権限不足（1314）で早期returnしている。書庫内リンクの拒否はZIP/7zで検証済み。
- 配布用ライセンス一覧は既存の `cargo-about generate --workspace --locked --fail` と
  `cargo vendor --locked` の対象になることを確認。`about.toml` の追加許可は不要。
  この環境にcargo-aboutがないため、一覧の実生成は未実施。

- Windows / DX12 / WASAPI、960×540の通常Autoplayで3形式を起動。
  各書庫で音声894ソース・987区間、静止BGA32件（failed=0）、動画BGAのdecoder起動と
  gameplay専用スレッドへのinstallを確認。全プロセスの終了コードは0、ERRORログは0件。
  ZIP/RARは再生中のフレーム数上限で終了し、7zは曲全体を自動演奏してResult到達まで確認した。
  検証profileのmaster volumeは0のため、実デバイスでの聴取確認とは扱わない。
  初回1,800フレームのsmokeはロード途中の画像だったため、再生確認の根拠から除外した。

ログは `.local/validation/2026-10-06-archive-playback/` に保存し、Git管理しない。

## 対応範囲

現在の利用方法と形式・資源量の制限は [書庫内譜面の直接再生](../../docs/archive-songs.md)、
DB移行・rootとスキャンの扱いは [曲の取得元とスキャン](../../docs/song-sources.md) にまとめる。
暗号化・分割・自己解凍書庫は非対応。キャッシュの自動GCは行わない。
macOS/Linuxの実行、実デバイスでの聴取、手動のSelect/Practice/Replay/Course/Viewer操作は未実施。
