# oraja_helper 連携

Windows では `oraja_helper` が作成する `\\.\pipe\oraja_helper` に、
UTF-8 の JSON Lines を送信します。BMZ 側の追加設定は不要です。
helper の設定でプレイヤーを BMZ Player にし、本体フォルダを指定してください。
BMZ モードで pipe を受信し、`source: "bmz-player"` を認識する helper が必要です。
従来の DB 監視専用の BMZ 対応版では、選曲・配置・クイックリトライ情報を受信できません。

| イベント | 送信タイミング | 主な情報 |
|---|---|---|
| `song_select` | 選曲画面の選択譜面が変わったとき、選曲へ戻ったとき | SHA-256、MD5、曲名、アーティスト |
| `song_play` | ロード済みのプレイセッションを設置したとき | 譜面情報、オプション、seed、レーン配置 |
| `song_play_end` | Play 退出時、クイックリトライで前のプレイを破棄する直前 | 譜面・配置、確定判定、処理済みノーツ数、総ノーツ数、経過秒数、`quickRetry` |
| `song_result` | 通常の単曲 Result に入るとき | 譜面・配置、EX スコア、スコア率、ランプ、BP、確定判定 |

全イベントに `source: "bmz-player"` を付けます。フィールド名は
lr2oraja-endlessdream の `OrajaHelperClient` と共通です。
`scoreRate` は 0～100、`clearLampId` は BMZ / beatoraja 共通の 0～10 です。
`judges` は `epg/lpg/egr/lgr/egd/lgd/ebd/lbd/epr/lpr/ems/lms` で、
`epr/lpr` は見逃し POOR、`ems/lms` は空 POOR を表します。
判定は描画用の FAST/SLOW 表示フィルタを通さず、gameplay が確定した結果から取得します。
クイックリトライでも正式な `song_result` は作らず、`song_play_end` の統計だけを送ります。

`randomPlacement` / `randomPlacement2P` は、各配置先にある元レーンを並べた文字列です。
beatoraja のレーン番号に合わせて1から数え、固定スクラッチを末尾から除きます。
例として SP 正規は `1234567`、MIRROR は `7654321` です。
DP の2P元レーンは1Pの鍵盤とスクラッチの後から数えるため、14Kの2P正規は `9101112131415` です。
S-RANDOM 等、ノートごとに配置が変わる側には単一の配置を送らず空文字列にします。
BMZ 独自の F-RANDOM / MF-RANDOM は名前を送り、共通の `optionId` は `null` にします。

プレイ・終了・リザルトは通常の単曲手動プレイが対象です。
AUTOPLAY、REPLAY、PRACTICE、Viewer、コース、スコア保存を無効化するプレイは対象外です。
AUTO SCRATCH のような部分的な自動操作は通常のプレイとして扱います。
選曲時のベストスコア送信は含みません。helper は従来の DB 監視と自分の履歴を併用します。

送信は専用 worker の有界キューで行い、判定・音声・描画スレッドでは pipe I/O を行いません。
1イベントの接続・送信は最大1秒で打ち切り、2秒以上滞留したイベントは捨てます。
helper の未起動・停止・キュー満杯でゲームを止めず、配送の保証や過去イベントの再送は行いません。
受信側は1行を読んだ後に接続を閉じ、次のイベント用に pipe を作り直します。
macOS / Linux ではこの Windows pipe への送信は行いません。

開発時の確認は `cargo test -p bmz-player --locked oraja_helper` を使います。
Windows のテストは、本番とは異なる一時 pipe で4イベントの接続と再接続を確認します。
実際の helper と OBS 表示の確認では、選曲変更、通常完走、中断後のクイックリトライ、
helper 未起動時の操作を確認してください。
