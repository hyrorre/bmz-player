# 選曲バーごとのバナーと非同期サムネイルキャッシュ

2026-11-15。基点はmain `8a5b701c37bb572e312c70a643814857267df334`（#104マージ後）。
既存の13スコア・状態フィールドとは別PRとして、各バーのbanner画像を追加する。
確定仕様とJSON / Luaの定義例は[skin.md](../../docs/skin.md#bmz-songlist-row-banners)を参照。

## 調査と境界

- `SelectRowSnapshot` は行単位のメタデータを持ち、`bar_scroll_rows` がスクロールの物理的な表示窓を表す。
  選択行だけを基準にすると、大きいスクロール残量がある間に表示されている画像を読み損ねる。
- 従来の `select_assets` の選択譜面用bannerは102 / -102の共有テクスチャであり、各行には流用しない。
  同runtimeに独立した行バナー管理を置き、ライブラリ無効化でも2本のワーカープールを維持する。
- rendererは前回のplanとSnapshotを保持する。Snapshotへtexture leaseを持たせ、古いplanが再描画される間も
  GPU画像とIDを維持する。元のSnapshotを置き換えて参照がなくなった後に、画像とbind groupを削除する。
- Qwilightの `View/MainWindow/DefaultMode/EntryView.xaml` は行の `NoteFile.BannerDrawing` を参照し、
  `NoteFile/BaseNoteFile.cs` はgetterから `Task.Run` で遅延ロードする。
  参照元は[Qwilight](https://github.com/thenist/Qwilight/tree/6d3887f6688a1f97c1fbe2bffd28feaedc9920b6)。
  BMZでは行ごとの遅延表示を参考にし、スレッド数とキャッシュ量は明示的に制限する。

## 実装

- `songlist.banner` は通常destinationの単体／配列。画像定義は不要で、各行のSnapshotから直接取得する。
  共通の画像geometry・stretch・clip・timer・draw評価を使い、バー本体直後へ追加する。
  既存の部品は順序と意味を維持する。G-BATTLEなどの疑似譜面行へは付与しない。
- 解決・デコードは固定2ワーカー、要求と結果のchannelも各2件。
  世代の古い結果は適用せず、ライブラリ更新時にも旧ジョブを含め2件までに制限する。
- 参照元（譜面フォルダ＋banner名）と、物理画像（canonical path＋metadata stamp、
  書庫はさらにgeneration＋entry）を分離し、別譜面／相対パスから同じ画像を共有する。
  ネガティブ結果は30秒、成功した参照元は1秒間隔でワーカーから更新確認する。
- 25表示行＋前後2行＋選択行だけを要求し、先読みはCPUまで。GPU uploadは表示行を優先し、
  2枚・2 MiB/frameまで。CPU画像／GPUは各32 MiB・256枚、参照元と失敗情報は各256件まで。
  Snapshotが保持する解放待ちGPU画像も予算へ計上し、早すぎるID再利用を防ぐ。
  上限はRGBA画素の保持量であり、ワーカーの一時バッファ・書庫辞書・GPUドライバー内部領域とは区別する。
- 最大1024×256へ縮小し、1枚は最大1 MiB。入力16 MiB、RGBA換算16 MiB、各辺8192pxの制限を
  デコード前に確認する。アニメーション画像は静止画像のみ。CIMと動画は今回の対象外。
- 書庫は既存のentry解決と読み込み経路を使用し、選択entry専用のバイト上限を追加した。
  他の大きな音声等を理由にバナーを拒否せず、読み込むentryの確保と実書き込みを制限する。
  一覧をスクロールすることで展開ファイルがディスクへ増え続けることはない。
  参照元の定期確認では共有indexをArcで参照し、同じパックの全entry名を行ごとに複製しない。

## 検証

Linux / Rust 1.100.0。`cargo fmt --all --check`、`cargo check --workspace --locked`、
`cargo clippy --workspace --all-targets --locked -- -D warnings` は成功。
既存の `bmz-audio` manifestの未使用 `anyhow` 警告は変更していない。

`LANG=ja_JP.UTF-8 LC_ALL=ja_JP.UTF-8 cargo test --workspace --locked --no-fail-fast` を実行。
ローカルソケットを禁止したサンドボックスではOBS / DBus等が失敗するため、ローカル通信を使える条件で再実行した。

| 対象 | 結果 |
|---|---|
| bmz-player lib | 3266 passed / 2 failed / 67 ignored |
| bmz-render | 1066 passed / 0 failed / 29 ignored |
| bmz-skin | 434 passed / 0 failed / 5 ignored |
| bmz-skin-document | 19 passed / 0 failed |
| その他workspaceターゲット | 失敗なし |
| 今回追加した21テスト | すべて成功 |

失敗した変更対象外のscanテストは次の2件。

- `storage::scan::link_tests::overlapping_external_links_read_every_folder_once`：
  子フォルダから親を指すリンクで、期待する `a/parent/c.bms` が列挙されない。
- `storage::scan::tests::scan_song_roots_skips_unreadable_subdirectory_and_keeps_accessible_files`：
  `discovery_skipped` が期待値1に対し0。

新規テストは画像デコードと上限、JSON / Luaの単体・配列、行固有の描画アイテム、条件・clip・スクロール、
非同期世代と結果の逆順到着、欠損／破損の再試行、ファイル／書庫更新、画像共有、GPU leaseとID再利用、
10,000回の合成スクロールおよび小さい画像の件数上限を検証した。
Rmz-skin、mz-select、Luxez-Flatのsubmodule素材あり。ADFX02等の未導入外部素材を必要とするテストは
早期returnまたはignoreしており、そのスキンの実素材確認済みとは扱わない。
実GPUの目視確認、Windows/macOSでの実行と実曲ライブラリでのFPS計測は未実施であり、
合成テストの上限確認とは区別する。進行中の書庫読み取り2件の即時中断は実装していない。

生ログは実行環境の `/tmp/bmz-banner-{check,clippy,workspace-tests}.log` に置き、Gitには含めない。
