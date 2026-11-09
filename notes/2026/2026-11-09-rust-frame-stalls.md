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

## 続く確認対象

4. SCROLLの可視候補抽出。
5. 描画用判定状態の公開。
6. 判定候補・キー音ID・HCNの探索。
7. 文字レイアウトキャッシュとatlas。
8. BGAイベント検索。
9. 非表示動画の保持。
10. 音声・動画資源の後片付け。
