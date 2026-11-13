# Rust実行経路の停止・負荷候補の計測と修正

## 条件と範囲

- 基準: `a5044120042c449d9ff7a48199cb2401bd2f6bf8`。
- ブランチ: `perf/rust-frame-stalls`。
- 2026-11-09、Windows、rustc/Cargo 1.99.1。CPUプローブはreleaseで単独実行する。
- 静的調査の候補を順に測定し、実測した処理コストと画面全体のFPSを区別する。
- CPUプローブの結果は、GPU描画・実表示間隔・実音声・実コントローラの検証ではない。
- 普段の設定・library/score DBは書き換えず、一時的な合成素材を使う。

## 1. 選曲中の書庫metadata取得

既存処理は画像・試聴のキャッシュキーを作るたびに、同じ書庫へ同期metadata取得を行う。
比較プローブは同じローカル一時ファイルから7個のキーを生成し、100回warm-up後の
1000回を計測する。生成するキー以外の描画処理・書庫展開・音声decodeは含まない。

```powershell
cargo test -p bmz-player --release --lib profile_select_archive_cache_keys --locked -- --ignored --nocapture
```

| 版 | 中央値 | p95 | p99 | 最大 |
|---|---:|---:|---:|---:|
| 修正前 | 317.9 us | 559.3 us | 717.7 us | 1388.6 us |
| 非同期stamp導入後 | 2.4 us | 3.0 us | 3.1 us | 9.7 us |

上表はsandbox内の初回比較、各1回。修正後は初回metadata確認完了後に測り、毎回の非ブロッキングpollも含む。
この短い試行は1秒周期の再確認開始を含まず、HDD・NAS・実選曲画面のFPSは未計測。

通常権限でも、同一プローブ内で既存の同期キー生成関数と修正版を比較した。
外部からのファイル更新がない定常状態を測り、OS・sandboxの差と混同しない。

| 通常権限・同一プローブ | 中央値 | p95 | p99 | 最大 |
|---|---:|---:|---:|---:|
| 同期取得 | 111.1 us | 150.8 us | 216.3 us | 669.1 us |
| 非同期確認済みstamp | 1.7 us | 1.8 us | 1.8 us | 14.2 us |

選択中の書庫だけを同時1件のworkerで確認し、再確認間隔を完了後1秒とする。
選択変更中の要求は最新の選択へ集約し、世代が異なる結果を捨てる。画像・明示試聴・
生成試聴は同じ確認結果からキーを作る。資源workerのロード前後の再検査は従来どおり。
仕様は[書庫内楽曲](../../docs/archive-songs.md)を参照。

検証: fmt、`cargo check -p bmz-player --locked`、all-targets Clippy（`-D warnings`）を確認。
通常権限での`cargo test -p bmz-player --locked`は2986 passed、58 ignored、失敗0。
別途startup_stackの2件はignored。sandbox内の初回全テストはWindowsネイティブ処理で
異常終了したが、通常権限で再実行して通過した。新しい回帰は要求の集約、世代破棄、
周期的な再確認、実ファイル更新後の画像・生成試聴キー一致を確認する。

## 2. スキン完了待ちと画面遷移

完了通知を10 / 50 / 200ms遅らせる合成プローブを、通常権限のreleaseで実行した。
修正前の`ensure_skin_ready`呼び出し時間は10.658 / 50.447 / 200.413msで、
decodeがworker上でも、呼び出し元は通知まで停止することを確認した。
実スキンのdecode所要時間を測った値ではない。

```powershell
cargo test -p bmz-player --release --lib profile_skin_entry_wait --locked -- --ignored --nocapture
```

対話中の画面遷移は完了をフレーム間で確認し、タイマー・READY・Result入力解禁を
準備完了後に開始する。待機中は直前の描画プランとeguiの読み込み表示を描画する。
起動時にSelectを最初のフレームから表示する同期ゲートは維持する。
詳細は[frame pacing](../../docs/frame-pacing.md)を参照。

同じ遅延の修正後の要求時間は0.008 / 0.001 / 0.001ms。通知待ちを計測区間外へ
移した値であり、スキンの総ロード時間が短縮したという意味ではない。

fmt、対象crateのcheck、all-targets Clippyを確認。通常権限の全テストは
2986 passed / 59 ignored / 0 failed（別途startup_stackは2 ignored）。
単独のoffscreen GPU回帰は、Playの譜面情報・世代・再利用、およびコースの終了・
キャンセル・失敗リトライを確認。遅延注入プローブでは旧世代破棄、Decideの
キャンセル、Resultの入力と自動退出、準備後のタイマー再開始も確認した。

既存のPlay GPUテストは素材を許可された`data/skins`配下に移した。
コーステストはフレームループの代わりにスキン完了を明示的に待ってから操作する。
途中のWindows linker失敗は実行中テストexeへの上書き競合で、終了後の単独実行で通過。
実ウィンドウの手動操作・実機音声・コントローラは未確認。

## 3. 難易度表保存・選曲DBアクセス

ネットワークを使わず、一時DBへ未所持譜面だけの合成表を保存する。
各サイズで1回warm-up後、同じ表の置換とレベル一覧生成を5回測定した。

```powershell
cargo test -p bmz-player --release --lib profile_table_database_work --locked -- --ignored --nocapture
cargo test -p bmz-player --release --lib profile_table_store_dispatch --locked -- --ignored --nocapture
cargo test -p bmz-player --release --lib profile_select_list_dispatch_and_install --locked -- --ignored --nocapture
```

| 件数 | 修正前保存 中央値 / 最大 | 修正前一覧生成 中央値 / 最大 |
|---|---:|---:|
| 1,000 | 5.528 / 5.626 ms | 2.586 / 2.667 ms |
| 10,000 | 57.442 / 65.306 ms | 24.087 / 24.489 ms |
| 50,000 | 313.810 / 390.958 ms | 119.386 / 138.545 ms |

表保存は同時1件のworkerへ移し、INSERT文を行ごとにprepareし直さないようにする。
Select外では新しい保存を開始せず、既に開始したtransactionはworkerで完了させる。
旧アカウントの削除は保存完了後に直列化する。ダウンロード結果の待機channelは2件に制限する。

一覧準備は別のread-only接続で行う。1 worker・最新1要求に集約し、画面遷移や
プロフィール変更で旧世代を無効化する。refreshがフォルダ移動要求を取り消さないようにし、
選曲カーソル・検索成功履歴・空フォルダ・失敗時の扱いを維持する。

失敗した同一フォルダ情報・クリア履歴の問い合わせは1秒待って再試行する。
INSERT文再利用後の保存中央値は、1,000 / 10,000 / 50,000件で
2.545 / 30.285 / 164.927ms。workerへの保存要求は0.027 / 0.034 / 0.036ms。
一覧生成そのものは約116ms（50,000件）残るが、window threadから外れる。

| 件数 | 一覧要求 中央値 / 最大 | 完了一覧適用 中央値 / 最大 |
|---|---:|---:|
| 1,000 | 0.012 / 0.020 ms | 0.101 / 0.138 ms |
| 10,000 | 0.013 / 0.028 ms | 0.088 / 0.125 ms |
| 50,000 | 0.014 / 0.017 ms | 0.120 / 0.127 ms |

適用時の古い一覧破棄も同じ1-worker枠へ移し、選択位置が同じ場合の全件hash検索を省いた。
破棄中に次の要求を受けた場合、query開始は次のpollになる。上表の要求時間にはその
後続pollのthread作成は含まない。単一回の5試行比較で、実ライブラリ・NAS・全体FPSは対象外。
同一フォルダ・クリア履歴・replay slotの初回問い合わせは依然同期である。

fmt、対象crateのcheck、all-targets Clippyを確認。通常権限の全テストは
2992 passed / 65 ignored / 0 failed（別途startup_stackは2 ignored）。
追加回帰は要求集約、世代破棄、worker失敗、保存完了通知とcommit順、失敗cache期限。
releaseの一覧プローブでは読み込み中のカーソル移動、空フォルダ、旧アカウントの
保存完了後の削除も確認した。実ウィンドウ操作は未確認。

## 4・5・8. プレイ描画の反復走査

release、各10回warm-up後にSCROLL 200回、判定公開/BGA検索1,000回を単独実行。
定速120 BPM、SCROLL=1のノーツ列、全件PGREATの判定履歴、Baseだけを持つBGA列を使う。
GPU・実ウィンドウ・音声を使わないCPUプローブである。

```powershell
cargo test -p bmz-player --release --lib screens::play_snapshot::tests::performance --locked -- --ignored --nocapture --test-threads=1
```

| 処理・条件 | 修正前 中央値 / p95 | 修正後 中央値 / p95 |
|---|---:|---:|
| SCROLL 1万ノーツ・10%地点 | 77.9 / 105.9 us | 2.3 / 2.8 us |
| SCROLL 5万ノーツ・10%地点 | 387.9 / 491.4 us | 2.5 / 2.5 us |
| SCROLL 5万ノーツ・90%地点 | 71.2 / 123.9 us | 2.4 / 2.5 us |
| 判定履歴1万件 | 96.5 / 113.9 us | 2.3 / 2.7 us |
| 判定履歴5万件 | 518.4 / 613.8 us | 8.6 / 8.8 us |
| BGA 1万イベント | 9.5 / 9.6 us | 0.1 / 0.1 us |
| BGA 5万イベント | 60.0 / 69.5 us | 0.1 / 0.1 us |

SCROLLがすべて有限の正値なら積分の逆変換で可視候補を絞る。倍率変更・重複tick・
最初のイベントより前も扱い、f32丸めで上端に残るノーツを捨てない余白を加える。
0・負値・非有限値を含む場合は全候補走査を維持する。
判定履歴はHashMap容量が同じ場合にbucketをコピーし、成長中は事前確保した容量を保つ。
この計測は全件判定済みで容量が一致する場合であり、途中の全状態を代表しない。
BGAは既存の種類別cacheを動画・アニメ画像と共有し、消去イベントとPoor起点を維持する。
0.1usの表示は計時分解能に近く、厳密な倍率比較には使わない。

fmt、check、all-targets Clippy、bmz-player全テスト2995 passed / 65 ignoredを確認。
初回の単純なclone_fromは事前確保容量を失う回帰で検出され、同容量時だけ使う方式へ修正した。
SCROLL境界、ゼロ/負/非有限倍率、BGA同時刻・消去・Poor開始/終了を追加検証した。

最終確認で、先行する極端なSCROLLの累積値に可視距離を足すと、f64丸めにより
同じ値に戻る境界も補強した。逆変換に累積値に応じた余白を加え、非有限値は
全候補走査へ戻す。`1e100`から`1e-100`へ変化する回帰例で、表示距離が0へ
丸められる未来ノーツを候補から落とさないことを確認した。追加後のfmt、check、
all-targets Clippyとbmz-player全テストは成功（2997 passed / 66 ignored）。
同じreleaseプローブを単独で再実行し、1万ノーツ10%地点は中央値2.3us / p95 2.6us、
5万ノーツ10%地点は2.5 / 2.5us、90%地点は2.4 / 2.5usだった。

## 6. 判定候補・ノートID・HCN

releaseの`profile_gameplay_searches`。各10回warm-up後1,000回。1レーンのTap列後半へ
同じ入力を繰り返し、ID検索と、1区間だけが現在時刻を含むLN/HCN列の更新を測る。
全入力・全判定規則を代表するものではなく、判定結果の互換は既存回帰で別に確認する。

| 50,000件 | 修正前 中央値 / p95 | 修正後 中央値 / p95 |
|---|---:|---:|
| 入力処理 | 87.7 / 102.3 us | 0.2 / 0.2 us |
| ノートID検索 | 42.4 / 48.7 us | 計時分解能未満 / 0.1 us |
| 通常LNのHCN状態更新 | 91.9 / 100.1 us | 計時分解能未満 / 0.1 us |
| HCN状態更新 | 91.9 / 105.2 us | 0.1 / 0.1 us |

入力候補・LR2orajaの複数BAD・Mineを時刻の下限から調べる。ノートIDとHCN区間はロード時に
索引を作り、live判定mapも必要容量を予約する。対戦相手も同様。HCNの重複区間は元の順序を保つ。
索引は対象Arcを所有し、別譜面やArc::make_mut後は元の探索へ戻るため、古い索引を適用しない。

## 7. 文字レイアウトキャッシュ

releaseの`profile_layout_cache_overflow`。同梱テストフォントで4,096種類を組んだ後、
100項目を再使用してから新規1項目と同じ100項目を描く。1回warm-up後10回。
満杯時の全消去を古い1/4の退避に変え、同frameで描いたglyphのatlas行保護は維持する。

| 満杯フレーム | 修正前 | 修正後 |
|---|---:|---:|
| 100項目のcache miss | 100 | 0 |
| CPU 中央値 / 最大 | 579.9 / 665.7 us | 182.0 / 198.9 us |

文字色・装飾の互換とatlas shelf recyclingは維持する。GPU転送・実スキン・全体FPSの値ではない。

6・7の検証: fmt、workspace check、all-targets Clippy、workspace全テストが成功。
主対象はgameplay 265 passed、render 1014 passed / 23 ignored、player 2995 passed / 65 ignored。
索引の譜面差し替え・HCN重複/順序/個別mode、cacheの同一キー置換・同frame上限も追加確認した。

## 9・10. 動画保持・待機、音声/動画の破棄

音声は実デバイスなしで、1MiBのPCM配列64 / 256個を最後に所有するsourceを破棄する。
1回warm-up後5回。修正前はwindow相当の呼び出し元で64MiBが中央値2.372ms、
256MiBが9.559ms（最大9.831ms）を要した。修正後は別workerの完了を計測区間外で待つ。
PCM解放の総コストが消えるわけではない。
最終版の通知中央値は64MiBで0.1us、256MiBでは計時分解能未満、両条件の最大は0.1us。
この区間にはworkerでの解放を含まず、先にworkerが回収済みなら空queueの確認だけになる。

動画はjoin対象が10 / 50 / 200ms遅れて終了する合成プローブを使う。
修正前Dropは10.628 / 50.530 / 200.289ms、退避後の呼び出し元は0.011 / 0.003 / 0.005ms。
これはFFmpegの実ファイル終了時間を測った値ではない。

実FFmpegのqueueプローブでは一時的な640×360 Y4Mを32本開き、frame 0を受け取って
100ms待機後、200msの待機ループ回数を測る。ポーリング時は4,232回、park後は0回。
選択中+queue内のRGBA合計は32本で168.75MiB、同条件2本で10.55MiB。
native codec内部と送信待ちframe、GPU textureのmemoryは含めない。
実アプリでは表示中の最大4 channelと非表示2本を保持し、削除は退避workerで行う。
32→2本の値は保持数を変えた同一decoderプローブで、実画面のprocess working set比較ではない。

```powershell
cargo test -p bmz-audio --release --lib profile_retired_pcm_destruction --locked -- --ignored --nocapture
cargo test -p bmz-video --release --lib profile_video_ --locked -- --ignored --nocapture --test-threads=1
```

停止・restart・EOF・full queueの復帰、終了待ち中の非ブロッキングDrop、退避中も数える
admission上限、HCNを含む既存音声回帰を検証する。fmt、workspace check、all-targets Clippy、
workspace全テストは成功（player 2996 passed / 66 ignored、audio 189 passed / 4 ignored、
video 42 passed / 2 ignored）。EOF後の繰り返しrestartも実FFmpegの一時Y4Mで追加し、
videoのClippyと全テスト43 passed / 2 ignoredを確認した。

## 最終確認

Rustの最終コミットは`968000e3371ec49af2c0b11452fa32da4132c714`。
共有変更時のworkspace check / all-targets Clippy / 全テストが成功した後、
追加したvideo回帰は同crate全テスト、SCROLL境界はplayer全テストで再確認した。
fmtとreleaseアプリのbuildも成功した。ignoredの性能プローブとoffscreen GPU回帰は、
上記に記載した対象を明示的に実行したもので、全ignoredテストを実行したわけではない。

Windowsの実ウィンドウでも、独立したデータ領域で以下を確認した。
全プロセスが終了コード0で終了し、SelectとResultの1280×720画像を目視確認した。

| 起動条件 | 確認結果 |
|---|---|
| Select、描画60frame後終了 | 既定スキンで一覧描画、`select.png`保存 |
| 同梱サンプル自動演奏、Result 5frame後終了 | Select→Decide→Play→Resultの遷移、正常終了 |
| 合成Result、描画120frame後終了 | 既定スキンのスコア・判定内訳描画、`result.png`保存 |

共通設定は`--window-mode windowed --window-size 1280x720`。
`BMZ_RESOURCE_DIR`はrepoの`data`、`BMZ_DATA_DIR`は
`.local/performance/rust-frame-stalls/window-smoke-1`を指定し、
`BMZ_CACHE_DIR` / `BMZ_LOGS_DIR`の上書きを外した。通常のDB・設定は使用していない。
呼び出し側は各120秒の制限を設けたが、timeoutは発生していない。

```powershell
cargo run --release -p bmz-player --locked -- --window-mode windowed --window-size 1280x720 --smoke-exit-after-frames 60 --smoke-screenshot .local/performance/rust-frame-stalls/select.png
cargo run --release -p bmz-player --locked -- --window-mode windowed --window-size 1280x720 --boot-play-sample --autoplay-on-start --smoke-exit-after-result-frames 5
cargo run --release -p bmz-player --locked -- --window-mode windowed --window-size 1280x720 --boot-result-sample --smoke-exit-after-frames 120 --smoke-screenshot .local/performance/rust-frame-stalls/result.png
```

初回は継承された`RUST_LOG=warn`で警告・エラーログなし。
遷移とbackendの証拠を残すため、自動演奏を`RUST_LOG=info`で1回追加した。
NVIDIA GeForce RTX 5090 / DX12 / Fifoでsurface描画し、WASAPIはRME Babyface Proを
48kHz / 2ch / 480frameで開いた。ログ上のSelect・Decide・Play・Result到達と
Result 5frame描画、`status="success"`、`dropped_file_lines=0`を確認した。
この試行にもWARN / ERRORはない。別途、バックグラウンドのスキンcatalog走査中に
外部mz-selectのLuaから`m-select error: nil`というINFOのprintが1件出ている。
これは既定スキンの実描画確認と区別し、外部スキンの全互換を確認済みとはしない。

生ログと画像は`.local/performance/rust-frame-stalls/`に保存した。
`stage910-tests.log`、`scroll-precision-tests.log`、`scroll-precision-profile.log`、
`window-autoplay-info.log`などを参照。実ウィンドウsmokeは動作確認であり、
表示FPS・frame timeの修正前後比較ではない。音声出力の初期化とサンプル進行は
確認したが、聴感・物理出力時刻・手動演奏・コントローラ・macOS / Linuxは未検証。

## 残る同期処理と検証範囲

選択譜面の未cache問い合わせもreleaseで計測した（1譜面/フォルダ、履歴なし、1,000回）。
同一フォルダ中央値58.7us / p95 74.5us、クリア履歴3.3 / 3.4us、replay slot 11.5 / 11.7us。
この条件では大きな停止を観測せず、初回問い合わせの同期経路は維持する。
実ライブラリの巨大フォルダ・DB競合・NASを代表しない。
config保存、Luaからのファイル読み取り、GPU upload/acquire/present、OS schedulingや
他プロセスによる負荷は、この個別CPU比較だけでは原因から除外できない。

## 2026-11-09 追記: レビュー後の修正

- channel方式の動画decoderは、受信queueのdrain後に起こすよう変更した（`25f0d811`）。
- 難易度表の保存と曲scanは、どちらもlibrary.dbへ別接続で長い書き込みを行うため直列化した（`b5b2e3d7`）。
  rusqliteは接続時に5秒のbusy timeoutを設定するため、競合は即失敗ではなく待機になる。
- 非表示のBGA動画decoderの保持数を2本から10本へ変更した。ローカルlibrary.db（BGAあり約11万譜面）で
  BGAチャンネルが参照する動画を数えると、3本以上は24曲・184譜面だった。#RANDOMを
  2000回シミュレートした1プレイあたりの本数は、多くが11本以下だった。12本以上はりりりくろ！！
  （Re:Barthで23〜32本）、Taken The First Step、一期一会、Stellar Blueに限られる。
  7z/rar書庫内の2,640譜面は未解析。10本は、この分布での開き直しと待機decoderのメモリの折衷である。

## 2026-11-09 追記: お気に入りフラグの読み込みコスト

一覧の読み込みはworkerが毎回新しいread-only接続を開く。`SelectCollectionCache`の更新判定
（`total_changes()` / `data_version`）は接続ごとの値なので、読み込みをまたいだcacheは使えない。
app側に残っていた未使用のcacheフィールドは削除した。

ローカルlibrary.db（約12.6万譜面）のbackupと合成collection.dbで、1,000行の一覧へ
お気に入りフラグを付ける時間を測った。release、各10回warm-up後100回。
お気に入りは譜面と曲を同数登録した。

```powershell
$env:BMZ_PROBE_LIBRARY_DB = "$PWD/.local/performance/collection-cache/library.db"
cargo test -p bmz-player --release --lib profile_collection_flags_per_list_load --locked -- --ignored --nocapture
```

| お気に入り数 | 毎回読み直し 中央値 / p95 / 最大 | cache再利用 中央値 |
|---:|---:|---:|
| 0 | 0.074 / 0.107 / 0.163 ms | 0.025 ms |
| 10 | 0.471 / 0.563 / 0.700 ms | 0.053 ms |
| 100 | 3.626 / 4.177 / 4.617 ms | 0.055 ms |
| 1,000 | 37.889 / 41.180 / 42.528 ms | 0.065 ms |

お気に入り1件あたり約38usで、件数に比例する。お気に入り曲ごとにlibrary.dbへ
1回ずつ問い合わせる`favorite_song_folder_set`が主因と見られる（内訳は未計測）。
処理はworker上なのでwindow threadは止まらないが、一覧の切り替えまでの時間に加わる。
ローカルprofileのお気に入りは譜面7件・曲11件で、0.5ms程度に相当する。
単一環境・単一回の計測で、NASや巨大フォルダは代表しない。

## 2026-11-09 追記: お気に入り曲のフォルダ解決を1回の問い合わせへ

`favorite_song_folder_set`はお気に入り曲ごとに`list_charts_by_sha256`を呼んでいた。
全曲のSHA-256をJSON配列として渡し、`json_each`で1回の問い合わせにまとめた。
temp tableは使わない。library接続の`total_changes()`が進むと、それを更新判定に使う
collection cacheが毎回無効になるためである。結果は曲ごとの解決と同じで、回帰テストで比較する。

同じprobe・条件での再計測（毎回読み直し、中央値 / p95 / 最大）:

| お気に入り数 | 変更前 | 変更後 |
|---:|---:|---:|
| 0 | 0.074 / 0.107 / 0.163 ms | 0.066 / 0.089 / 0.148 ms |
| 10 | 0.471 / 0.563 / 0.700 ms | 0.142 / 0.160 / 0.182 ms |
| 100 | 3.626 / 4.177 / 4.617 ms | 0.599 / 0.665 / 1.344 ms |
| 1,000 | 37.889 / 41.180 / 42.528 ms | 8.135 / 9.164 / 9.581 ms |

単一環境・各1回の比較。1,000件の残り約8msの内訳は未計測。

## 2026-11-10 追記: 設定保存のworker化

プレイ終了・クイックリトライ・詳細オプションを閉じたとき、window threadが
`profile.toml`と`config.toml`を同期保存していた。profileは全体を複製してTOML化し、
一時ファイルへの書き込みと`sync_all`の後にrenameする。ローカルの`profile.toml`は291KBで、
76%がスキン設定の履歴`skin.history`（152スキン分）だった。履歴に件数の上限はない。

ローカルのprofile / app configのコピーを`.local/performance/config-save/`に置き、
一時ディレクトリへ保存して測った。release、各10回warm-up後100回、中央値 / p95 / 最大。

```powershell
$env:BMZ_PROBE_CONFIG_DIR = "$PWD/.local/performance/config-save"
cargo test -p bmz-player --release --lib profile_config_save_stages --locked -- --ignored --nocapture
cargo test -p bmz-player --release --lib profile_queued_config_save --locked -- --ignored --nocapture
```

| 処理 | 中央値 / p95 / 最大 |
|---|---:|
| profile 複製 | 0.686 / 1.067 / 1.406 ms |
| profile TOML化 | 6.576 / 7.088 / 7.614 ms |
| profile 書き込み + fsync + rename | 1.283 / 1.664 / 2.120 ms |
| profile 保存全体 | 7.630 / 8.764 / 9.289 ms |
| app 保存全体 | 1.198 / 1.448 / 1.689 ms |
| プレイ終了時（profile + app） | 8.728 / 9.978 / 12.658 ms |

TOML化が大半を占めるため、書き込みだけでなくTOML化もworkerへ移した。
アプリ実行中の保存はすべて1本のworkerが受け付け順に行い、同じファイルの未処理の保存は最新1件にまとめる。
同じ一時ファイル名へ並行して書かないよう、結果を必要とする呼び出しもworker経由で完了を待つ。
仕様は[frame pacing](../../docs/frame-pacing.md)の「設定の保存」を参照。

呼び出し側の時間（プレイ終了時のprofile + app、各2回の中央値 / p95 / 最大）。
保存を待たない方式では、各試行の前にworkerが空になるまで待ち、プレイ終了時と同じ状態から測った。

| 方式 | 1回目 | 2回目 |
|---|---:|---:|
| 変更前の直接保存 | 8.837 / 9.962 / 10.526 ms | 9.097 / 16.999 / 22.248 ms |
| 変更後・待たない | 0.525 / 0.703 / 1.219 ms | 0.521 / 0.654 / 1.039 ms |
| 変更後・完了を待つ | 9.468 / 11.127 / 12.169 ms | 9.278 / 11.147 / 11.389 ms |

残る約0.5msはprofileとapp configの複製である。直接保存のp95・最大は試行間で大きく変わり、
fsyncのばらつきと見られる（原因は未確認）。完了を待つ方式は直接保存より中央値で約0.5ms長い。
複製と受け渡しの分で、設定画面の保存、キー設定、ライバル切り替え、プロフィール切り替えなどに残る。
詳細オプションを閉じたときの保存は失敗しても再試行の印を残さず、ログに記録する。
profileは常に全体を保存するので、次の保存で同じ内容が書かれる。
`skin.history`の件数制限は仕様変更になるため行っていない。
HDD・NASでの書き込み時間と、実ウィンドウでのframe time比較は未計測。

検証: fmt、check、all-targets Clippy（`-D warnings`）。新しい回帰テストで次を確認した。

- 同じファイルへの連続保存のまとめ
- 完了を待つ保存より前に受け付けた保存の書き込み
- 書き込み失敗の通知
- worker停止後に呼び出し側で保存すること

bmz-playerの全テストは3000 passed / 1 failed / 69 ignored。失敗は
`skin_video_hidden_frames_keep_playback_time_without_uploading_or_reopening`の末尾の
一時動画削除で、単独では3回とも通過した。動画decoderを退避workerで閉じる前に
Windowsでファイルを消そうとする既存テストの不安定さで、設定保存とは別の経路である。
ignoredのリトライ・コース遷移、Playの譜面長、詳細オプションのapp回帰も単独で通過した。

実ウィンドウ（独立したデータ領域、release）ではOSの既定音声出力が無く、自動演奏はPlayを
開始できずSelectへ戻った。プレイ終了時の保存は実ウィンドウでは未確認である。
ウィンドウを閉じる要求で終了させ、`game exit`のprofile / app保存が終了準備より前に
workerで完了し、両ファイルが更新され、一時ファイルが残らないことを確認した。
新しい設定の既定値で難易度表の取得も走った（27件中3件はHTTPエラー）。
生ログは`.local/performance/config-save/`に保存した。

## 2026-11-10 追記: 動画削除テストの安定化

`skin_video_hidden_frames_keep_playback_time_without_uploading_or_reopening`は、
最後に一時Y4Mを`remove_file(...).unwrap()`で削除していた。`f8fbbd2e`以降、動画decoderの
Dropは退避workerへ渡すだけで、FFmpegがファイルを閉じるのはdecode threadの終了後になる。
Windowsでは開いているファイルを削除できず、負荷が高いと`Os { code: 32 }`
（別のプロセスが使用中）で失敗した。bmz-videoの`ThreeFrameMovie`は同じコミットで
削除の再試行に変えていたが、bmz-player側のこのテストは直していなかった。

一時ファイルの削除をDropで行うguardに移し、削除できるまで最大5秒再試行する。
guardはsourceより先に宣言し、decoderの破棄後に削除されるようにする。
期限を過ぎたら失敗させるが、既にassertionで失敗している場合は二重panicにしない。

同じテストのdebugビルドを、24プロセス同時に8回、計192回実行して比較した。

| 版 | 失敗 |
|---|---:|
| 変更前 | 128 / 192（すべて削除の`Os { code: 32 }`） |
| 変更後 | 0 / 192 |

変更前の失敗で`%TEMP%`に残った`bmz-skin-video-*.y4m`143件は削除した。
比較の集計は`.local/performance/skin-video-test/`に保存した。

fmt、all-targets Clippy（`-D warnings`）を確認した。bmz-playerの全テストを3回続けて実行し、
3回とも3001 passed / 0 failed / 69 ignoredだった。
