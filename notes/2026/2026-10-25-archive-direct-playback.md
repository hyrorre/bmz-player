# 書庫内楽曲を展開せずにメモリから再生する

## 目的と参照

2026-10-25の依頼により、ZIP / RAR / 7z内の譜面を、書庫全体をキャッシュへ展開せずに再生できるようにし、
従来方式と比較計測した。beatorajaの `721856fb`（`SongResource`）と、
`.local/beatoraja/src/bms/player/beatoraja/song/` の実装を参照した。
先行する実装は [ZIP・RAR・7z内楽曲の直接再生](2026-10-06-archive-song-playback.md)。
現在の仕様は [書庫内譜面の直接再生](../../docs/archive-songs.md) にまとめる。

## beatorajaの方式

- `SongResource` が実ファイルと書庫内エントリを抽象化し、`ArchiveSongResource.openStream()` で
  1エントリずつストリームを開く。音源は `PCM.load(resource)`、画像はImageIO、動画は
  `readAllBytes()` でメモリから読み込む。Pathしか受けないAPIだけ1ファイルを一時ファイルにする。
- エントリを開くたびに書庫を開き直し、存在確認のたびに全エントリを列挙する。
  solid RARでは後ろのエントリほど先頭から復号し直すため、素材数Nに対して O(N²) の展開になる。
  7zには対応していない。

## BMZの方式

beatorajaと同じく書庫内パスのまま資源を扱うが、エントリごとに書庫を開き直さず、
譜面が使うエントリを1回の読み込みでまとめてメモリへ置く。

- `bmz-song-resource`（新規crate、依存なし）が、`pack.zip!/entry` パスの判定と、
  書庫ごとの `SongResourceSet`（全エントリ名と読み込み済みbytes）のregistryを持つ。
  `bmz-chart`（音源候補・BGAの拡張子補完・存在確認）、`bmz-audio`（WAV・FFmpeg）、
  `bmz-render`（画像）、`bmz-video`（動画）は、実ファイルを読む前にこのregistryを確認する。
- FFmpegへは `ffmpeg-next 9` の `format::input_from_stream` と `StreamIo::from_read_seek` で
  メモリ上のbytesを渡す。unsafeや `bmz-ffmpeg` の変更は不要だった。動画BGAのシーク・ループ・
  開き直しもメモリ上で行う。
- `bmz-player` の `archive_resources` が読み込み計画を担当する。
  ZIPと非solid RARは譜面を読み、解析後に使うエントリだけを直接読む。
  solid RAR / 7zは途中から読めないため、譜面フォルダを1回の順読みでまとめて読み、
  使わなかったエントリを捨てる。
- `song_archive::read_entries` は選んだエントリだけを読み、最後の対象を読んだ時点で止める。
  非solid RARは未選択メンバーの本文を読まず（`ExtractionDecision::Skip`）、
  7zは対象の無いブロックを復号しない。
- 名前は完全一致を優先し、無ければ大文字小文字を無視して探す。以前の展開先が
  Windowsのファイルシステムだったため、`KICK.WAV` 指定で `kick.wav` を読めていた互換性を保つ。
- 動画BGAはプレイ中のイベント開始時に描画スレッドで開くため、読み込んだsetを
  `PreparedPlayChart` から `RunningPlaySession` まで保持する。registryは直近3書庫（1GiBまで）を
  強参照し、保持者がいるsetは弱参照でも探す。registryに無い書庫はfallbackが1エントリずつ読む。
- 書庫全体を展開する `song_archive::materialize` と `song-archives` キャッシュは削除した。
  選曲画像・明示試聴・曲テキストは外部へファイルを渡すため、従来どおり
  `materialize_entry` で1エントリだけ `song-archive-assets` へ取り出す。

## 計測

2026-10-25、Windows 11 / i9-12900K / 64GB / NVMe SSD、release build。
従来方式は変更前のmain（`e2ff1478`）、メモリ方式は今回の変更後で、同じ計測コードを使った。
`crates/bmz-player/src/screens/play_session/archive_bench.rs`（`#[ignore]`）が、
書庫を置いたrootを一時DBへスキャンし、曲フォルダごとに最小chart_idの譜面について
`preload_play_session_for_chart`（譜面解析・音源デコード・音量正規化）、静止BGAの読み込み、
最初の動画BGAの1フレーム目デコードを時間計測する。プロセスごとに1回目がcold（書庫の読み込みを含む）、
2回目以降がwarm。ピークのワーキングセットは外部からプロセスを監視して取った。
音源デコードは従来どおり1スレッド。BGA画像は全条件で数ms、動画の1フレーム目は0〜6msで差は無かった。

### 1曲の書庫（preload ms）

| 書庫 | 形式 | 従来 cold | 従来 warm | メモリ cold | メモリ warm |
|---|---|---:|---:|---:|---:|
| 運命論 公式RAR（RAR4・非solid、47MB） | RAR | 4,688 | 3,270 | 3,479 | 2,672 |
| 同 7-Zip再梱包（solid、45MB） | 7z | 5,049 | 3,268 | 3,974 | 2,676 |
| 同 7-Zip再梱包（Deflate、48MB） | ZIP | 3,934 | 3,275 | 2,829 | 2,667 |
| WAV主体の曲（solid、80MB→展開134MB） | 7z | 5,058 | 1,112 | 4,216 | 820 |
| 同（Deflate、102MB） | ZIP | 2,055 | 1,094 | 1,298 | 817 |

- 従来方式は5書庫で431MBをキャッシュへ展開した。メモリ方式はディスクへ何も書かない。
- warmもメモリ方式の方が0.3〜0.6秒速い。約900の音源ファイルを開く処理が無くなったためと見られる
  （ファイル単位の内訳は未計測）。
- メモリに保持した資源は運命論48.8MB、WAV主体の曲135MB。ピークのワーキングセットは
  529MB→673MB（registryが直近3書庫を保持するため）。

### 10曲のパック（13フォルダ、展開約580MB）

各フォルダを初めて開いたときのpreload合計と、2回目の合計（ms）。

| 書庫 | 従来 初回合計 | 従来 2回目合計 | メモリ 初回合計 | メモリ 2回目合計 |
|---|---:|---:|---:|---:|
| `pack.zip`（Deflate、498MB） | 28,970 | 22,485 | 20,010 | 16,324 |
| `pack.7z`（solid・3ブロック、433MB） | 40,587 | 22,394 | 125,208 | 16,375 |

- 従来方式は最初の1曲で書庫全体を展開した（7z 20.5秒、ZIP 8.7秒）。以降の曲は展開済み。
  キャッシュは1.17GB。
- メモリ方式のZIPは、各曲の初回からほぼwarmと同じ時間になる。
- メモリ方式のsolid 7zは、曲ごとの初回に7.1〜13.5秒かかる。曲のファイルが拡張子順で
  ブロック全体に散らばり、ほぼブロックの終わりまで復号するため。5音源しかない曲でも9秒かかった。
  1曲だけ遊ぶなら従来の20.5秒より速いが、同じパックの曲を順に遊ぶと合計は約3倍になる。
- 同じ書庫の別の曲へ移っても1GiBまでは前の曲の資源を捨てないため、保持量はパック全体で529MBまで増え、
  ピークのワーキングセットは755MB→1,629MBになった。

生ログは `.local/archive-bench/results/` に置き、Git管理しない。計測用の書庫も
`.local/archive-bench/` に置く（第三者の曲データのため共有しない）。

## 検証

- `cargo fmt --check` 成功。
- `cargo clippy --workspace --all-targets --exclude bmz-render --locked -- -D warnings` と
  `cargo clippy -p bmz-render --lib` は成功。bmz-renderのall-targetsは、今回触れていない
  `crates/bmz-render/src/renderer/tests/text_gradient.rs` の `clippy::chunks_exact_to_as_chunks`
  （rustc 1.98.1のClippyで追加された指摘）3件で失敗する。
- `cargo test --workspace --locked --no-fail-fast` は4,664件成功、失敗0件。
- 追加した回帰テスト: `bmz-song-resource`（パス判定・大小文字・fetch・registryの退避と保持）、
  `archive_resources`（ZIP / solid 7zで使う資源だけを読む、再利用と世代交代、書庫外参照の拒否）、
  `song_archive` の `read_entries`（RAR4 / RAR5 solid、7zの読み取り停止）。
- 実機でのプレイ・聴取、macOS / Linux、solid RARのパックは未確認。RARの作成ツールが無いため、
  RARは公式配布の非solid RAR4だけを計測した。

## 残課題

- solid 7z / RARのパックで曲を順に遊ぶ用途では、従来の一括展開の方が合計時間は短い。
  複数の曲フォルダを含むsolid書庫だけ一時展開する、または読み込んだ曲フォルダを残すなど、
  方式の切り替えを検討する。
- 同じ書庫内で曲を移るときの保持量（1GiBまで捨てない）を下げるかは未決。
- 選曲画面の画像・明示試聴・曲テキストは、1エントリを `song-archive-assets` へ取り出す方式のまま。
