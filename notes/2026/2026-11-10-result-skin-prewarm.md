# Resultスキンをスコア確定時に先読みする

2026-11-10に、Result入場時のスキン待ちをなくした記録です。
現在の仕様は[フレームペーシング](../../docs/frame-pacing.md#スキン準備中の画面遷移)を参照してください。

## 経緯

- 2a370cc4でスキン待ちをフレーム間の準備状態へ変え、待機中にeguiのスピナーを重ねた。
  59affe39でSelect一覧の読み込み中にも出るようになり、遷移のたびに目立つためce81076bで削除した。
- Select・Decideは起動時に読み込んで再利用している。Playは決定ごとにDecide開始時から読み込み、
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
