# beatorajaリプレイの判定アルゴリズムと先頭余白

[Issue #117](https://github.com/hyrorre/bmz-player/issues/117) で、beatorajaから取り込んだ
リプレイをBMZで再生した際のランプ、EX、BPの差が報告された。

ローカルの `split second [S_Maniac]` は、BMZで記録時のDurationを使うと
beatorajaの履歴と同じEASY CLEAR、EX 5408、BP 137になった。
同じ入力とゲージ自動切替設定でComboに変えるとCLEAR、EX 5393、BP 121になった。

先頭余白がある `FATBROS PingPongMix #Decimalized` では、BMZで約735 msの余白を
入力にも加えるとEX 3580、BP 41がbeatorajaの履歴と一致した。
報告譜面 `INFERNAL REGION [Immortal]` の余白は0 msで、報告者のリプレイは手元にない。

取り込んだ判定アルゴリズムと入力時刻の基準をリプレイに保持し、譜面ロード後の採点セッションで
適用する。BMZで保存するリプレイにもプレイ時の判定アルゴリズムを記録する。
旧インポートは同じ元ファイルの再取込で更新できる。
利用時の挙動は [スコア・リプレイ保存](../../docs/score-persistence.md) を参照。

## 修正後の確認

Windowsで上記2件の元BRDを変換し、保存・再読込を経てBMZの採点処理をオフライン実行した。
プロフィールをComboにしても両方で記録されたDurationが使われ、ランプ、EX、BPが
beatorajaの履歴と一致した。

## 2026-11-17 PR #118レビュー対応

判定アルゴリズムの保存名を`serde(rename)`で固定した。
修正前の読み込み処理は`version`の上限を検査しないため、旧版での再生時の制約を仕様に記載した。
初回の再取込にかかる時間と置換件数は変更履歴に追記した。
