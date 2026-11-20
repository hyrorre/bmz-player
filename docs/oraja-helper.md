# oraja_helper 連携

Windows では `oraja_helper` が作成する `\\.\pipe\oraja_helper` に、
UTF-8 の JSON Lines を送信します。既定では無効です。BMZ メニューの「連携・録画」で
「oraja_helper 連携」を有効にしてください。設定は本体に保存され、変更は実行中に反映されます。
設定項目は Windows でのみ表示します。macOS / Linux でも保存済みの設定値は保持します。
無効化すると待機中のイベントは破棄されますが、送信中のイベントは完了する場合があります。
helper の設定でプレイヤーを BMZ Player にし、本体フォルダを指定してください。
BMZ モードで pipe を受信し、`source: "bmz-player"` を認識する helper が必要です。
従来の DB 監視専用の BMZ 対応版では、選曲・配置・クイックリトライ情報を受信できません。

| イベント | 送信タイミング | 主な情報 |
|---|---|---|
| `song_select` | 選曲画面で譜面の選択が1秒間変わらなかったとき | SHA-256、MD5、曲名、アーティスト |
| `song_play` | ロード済みのプレイセッションを設置したとき | 譜面情報、オプション、seed、レーン配置 |
| `song_play_end` | Play 退出時、クイックリトライで前のプレイを破棄する直前 | 譜面・配置、確定判定、処理済みノーツ数、総ノーツ数、経過秒数、`quickRetry` |
| `song_result` | 単曲またはコース各曲の Result に入るとき | 譜面・配置、EX スコア、スコア率、ランプ、BP、確定判定 |

全イベントに `source: "bmz-player"` を付けます。フィールド名は
lr2oraja-endlessdream の `OrajaHelperClient` と共通です。
`scoreRate` は 0～100、`clearLampId` は BMZ / beatoraja 共通の 0～10 です。
`judges` は `epg/lpg/egr/lgr/egd/lgd/ebd/lbd/epr/lpr/ems/lms` で、
`epr/lpr` は見逃し POOR、`ems/lms` は空 POOR を表します。
判定は描画用の FAST/SLOW 表示フィルタを通さず、gameplay が確定した結果から取得します。
クイックリトライでも正式な `song_result` は作らず、`song_play_end` の統計だけを送ります。
コース各曲の `song_result` のランプは単曲の保存時と同じ区分に補正し、
フルコンボ・パーフェクト・MAX以外は `NoPlay` にします。`song_play_end` は確定したプレイ結果を送ります。

`randomPlacement` / `randomPlacement2P` は、各配置先にある元レーンを並べた文字列です。
beatoraja のレーン番号に合わせて1から数え、固定スクラッチを末尾から除きます。
正規と MIRROR の配置文字列は ED 版に合わせて空文字列にします。
DP の2P元レーンは1Pの鍵盤とスクラッチの後から数えます。
S-RANDOM 等、ノートごとに配置が変わる側には単一の配置を送らず空文字列にします。
BMZ 独自の F-RANDOM / MF-RANDOM は名前を送り、共通の `optionId` は `null` にします。

プレイ・終了・リザルトは通常の手動プレイとコース各曲の手動プレイが対象です。
コース全体のリザルトは送信しません。
AUTOPLAY、REPLAY、PRACTICE、Viewer、スコア保存を無効化するプレイは対象外です。
AUTO SCRATCH のような部分的な自動操作は通常のプレイとして扱います。
選曲時のベストスコア送信は含みません。helper は従来の DB 監視と自分の履歴を併用します。

送信は専用 worker の有界キューで行い、判定・音声・描画スレッドでは pipe I/O を行いません。
1イベントの接続・送信は最大1秒で打ち切り、2秒以上滞留したイベントは捨てます。
helper の未起動・停止・キュー満杯でゲームを止めず、配送の保証や過去イベントの再送は行いません。
キュー満杯による選曲通知の破棄は debug、プレイ通知の破棄と worker 切断は warning に記録します。
受信側は1行を読んだ後に接続を閉じ、次のイベント用に pipe を作り直します。
macOS / Linux ではこの Windows pipe への送信は行いません。

開発時の確認は `cargo test -p bmz-player --locked oraja_helper` を使います。
Windows のテストは、本番とは異なる一時 pipe で4イベントの接続と再接続を確認します。
実際の helper と OBS 表示の確認では、選曲変更、通常完走、中断後のクイックリトライ、
helper 未起動時の操作を確認してください。
