# Result・Selectスキンの先読み

2026-11-10に、Result入場時と、ResultからSelectへ戻るときのスキン待ちをなくした記録です。
現在の仕様は[フレームペーシング](../../docs/frame-pacing.md#スキン準備中の画面遷移)を参照してください。

## 経緯

- 2a370cc4でスキン待ちをフレーム間の準備状態へ変え、待機中にeguiのスピナーを重ねた。
  59affe39でSelect一覧の読み込み中にも出るようになり、遷移のたびに目立つためce81076bで削除した。
- Select・Decideは起動時に読み込んで再利用している（同日訂正: 誤り。下の「Selectの先読み」を参照）。
  Playは決定ごとにDecide開始時から読み込み、
  Decide中に終わる。Resultは入場ごとにスコア等をLuaへ渡して読み直すため（26fa4eba以降）、
  Play終了のフェードアウト後にdecodeを始め、毎回待っていた。

## 計測条件

- Windows、release build、`--boot-play-sample --autoplay-on-start --smoke-exit-after-result-frames 120`、各3回。
- スキンは利用者の設定（SocialSkin）。frame間隔は`bmz_player::frame_pacing=trace`のsampleで、約240fps。
- 待ち時間は一時的に入れたscene entryの計測ログで測り、コミットしていない。生ログは共有しない。

## 結果

| 状態 | Result入場の待ち | 組み込み（window thread） | スコア確定→Result入場のframe間隔最大 |
|---|---|---|---|
| 変更前 | 102.6 / 103.2 / 103.4ms | 入場後 | — |
| 先読みしてPlay中に組み込み | 0（待ちなし） | 2.1〜2.6ms | 6.2ms（Play全体の最大5.7msを超える1frame） |
| 先読みして入場時に組み込み（採用） | 0（待ちなし） | 入場時に約2.1ms | 5.6 / 5.8 / 5.9ms |

- Playスキンのdecode/uploadは約185〜200msで、Decide中に完了する前提。Decideを短縮・スキップした場合の
  Play前の待ちは計測していない。

## 実装

- スコア確定の2経路（判定確定後の保存完了、Finished/Failedフレーム）で通常プレイのResult読み込みを開始する。
  2回目の呼び出しは同じ世代が有効なら何もしない。
- Resultの入力を作る処理は`finished_play`を読むため、要求の組み立て中だけ一時的に設定して戻す。
  Play側の状態判定にはResult入場まで現れない。
- Play中に届いた先読み世代のuploadは保持し、Result入場時に組み込む。Play再入場や先読みが無効な入場では破棄する。
- スキン設定の変更（signatureの破棄）や別のResult読み込み（世代の更新）があれば、入場時に従来どおり読み直す。

## 未確認

- 実ウィンドウでの手動プレイ、コースのステージ結果、LR2 Resultスキン（CONTINUE素材の引き継ぎ）の見た目は未確認。
- コースは今回の対象外で、入場時の読み込みのまま。

## Selectの先読み（同日追記）

### 訂正と調査

- 上の経緯にある「Select・Decideは起動時に読み込んで再利用している」は誤りだった。26fa4eba以降、
  SelectとDecideも画面への入場ごとに読み直している。beatorajaも各画面の`create()`で`loadSkin`を呼ぶ。
- 一時的なプローブで、ロードごとのLua document構築時間とdocument cacheの効き方を測った
  （SocialSkinは利用者のスキン設定、他は既定値。release build）。

| スキン | Select | Decide |
|---|---|---|
| SocialSkin | 100〜110ms、cache不可（runtime callbackあり） | 約110ms、cache不可（ロード時に`math.random`） |
| mz-select | 185〜194ms、cache不可（runtime callbackあり） | 初回4.0ms、以後0.1〜0.2ms（難易度・levelで再利用） |
| Luxez-Flat | 140〜181ms、cache不可（runtime callbackあり） | JSONで約1ms |
| ECFN | 32〜37ms、cache不可（runtime callbackあり） | 初回3.1ms、以後0.1〜0.2ms |

- Selectのうちruntime VMの構築は、mz-select 52〜56ms、Luxez-Flat約13ms、SocialSkin 18〜19ms、ECFN約3msで、
  残りは1回目のLua実行とdocument変換。Selectはロード時に累計プレイ数等を読むため、cacheできても毎プレイ後に外れる。
- Decideの選曲中の投機的な先読みは、SocialSkinのような一部のスキンにしか効かず、カーソルの停止ごとにCPUを使うため見送った。

### 実装

- Resultのスキン準備が終わった時点でSelectのdecodeを始め、uploadはResult表示中は保持し、Selectへの入場時に組み込む。
  コースの途中のステージ結果では先読みしない。
- 入場時は、ロード時の入力（パス、オプション、プレイヤー統計、カーソル位置の譜面のhash等）が先読み時と同じで、
  世代が最新、uploadが到着済み、かつメモリ上のスキンファイルに他から書き込みがない場合だけ使う。
- メモリ上のスキンファイルの保存領域（`LuaSkinFileStore`）に版を持たせ、各ロードに開始時・自身の反映直前・反映直後の版
  （`LuaFileStoreSpan`）を記録した。ModernChicのResultのように実行中に履歴を書くスキンは読み直しになり、
  Luxez-FlatのSelectのようにロード自身が`load_log.txt`を書くだけなら再利用できる。Resultの先読みにも同じ確認を加えた。
- Resultの先読みも入場時に未到着なら待たずに読み直す形にし、Result / Selectの先読みを`PrewarmedSceneSkin`にまとめた。
  ResultからPlayへ入った場合は、読み込み中の先読みの世代を進めて破棄する。

### 計測

- 条件: Windows、release build、`BMZ_DATA_DIR`等を一時フォルダへ向けた既定のprofileでSelectスキンだけを差し替え、
  `--boot-play-sample --autoplay-on-start`。Resultを120frame表示した後に一時コードで選曲へ戻り、
  先読みの有無を一時的な環境変数で切り替えた。各2回。一時コードと生ログはコミット・共有しない。
- 曲ライブラリは空で、カーソル位置に譜面はない。

| スキン | 先読みなしのSelect待ち | 先読みあり | Select組み込み（入場時） |
|---|---|---|---|
| mz-select | 218.3 / 227.1ms | 0 / 0 | 2.3〜2.7ms |
| Luxez-Flat | 181.1 / 179.1ms | 0 / 0 | 6.2〜7.3ms |
| SocialSkin | 121.2 / 126.0ms | 0 / 0 | 5.2〜5.4ms |
| ECFN | 65.2 / 72.8ms | 0 / 0 | 2.3〜2.8ms |

- Result表示中のframe間隔の最大は、先読みあり4.8〜5.5ms、なし4.8〜5.8msで差はなかった。
  組み込みは先読みの有無によらず入場時のため、入場直後のframe間隔（5.8〜11.1ms）も同程度だった。
- 検証: workspace全体のfmt、all-targets Clippy -D warnings、テスト（5236 passed, 0 failed, 111 ignored）。
  追加のWinit隔離テスト（`--ignored`）も成功。

### 未確認

- 実ウィンドウでの手動操作（Resultから選曲への通常の退出、リトライ、コース）、曲ライブラリのある環境での待ち。
- Selectの組み込み（2〜7ms）は入場時の1frameに残る。Decideの入場ごとの読み直しは変更していない。
