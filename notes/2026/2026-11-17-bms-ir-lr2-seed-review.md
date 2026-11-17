# BMS-IRのseed対戦とLR2譜面再現のレビュー

[PR #120](https://github.com/hyrorre/bmz-player/pull/120)の`7b712a97`と`2bcf84b7`を確認した。現行の操作と対応範囲は[操作ガイド](../../docs/controls.md#g-battle)と[IR仕様](../../docs/ir.md#bms-ir)に記載した。

## クライアント側の修正

- `7b712a97`のLR2リゾルバは、最初の抽選前に`#IF 0`を通し、`#RANDOM`の直後にも直前の有効状態を維持していた。互換動作に合わせて比較値の初期値を`-1`にし、`#RANDOM`を処理した直後に無効へ戻す。`#SWITCH`系の行では乱数を消費せず、譜面行は`#IF`の状態に従って読み込む。
- `2bcf84b7`の`RIVALCHART`でLR2の分岐を使った場合、選択値とレーン表だけでは同じ分岐を再現できない。適用したLR2 seedをリプレイと同一配置リトライへ引き継ぎ、通常のbeatoraja seedとしては送信しない。
- 空白だけの`seed_scheme`も欠落と同じ既定値を使う。BMS-IRでは形式不明のまま、従来のbeatoraja系providerでは従来の形式として扱う。BMS-IRの`rival-scores`の`play_seed`は形式の契約が確認できるまで配置に使わない。ランキング行でseedがあり形式だけ不明なときは、ローカルseedで開始する方針を維持した。

## 本番BMS-IRの観測

2026-11-17に保存済みアカウントで読み取りAPIを9回呼んだ。アカウント設定やスコアの書き込みは行っていない。

| 対象 | 観測 |
|---|---|
| 単曲ランキング | Beatorajaルールの40件は`beatoraja` 33件、`beatoraja_24bit_v1` 1件、空文字かつseedが`null` 6件。LR2orajaルールの50件は`lr2` 15件、`beatoraja` 1件、空文字かつseedが`null` 4件、形式欠落かつseedが`null` 30件。seedがある行は数値で、`lr2`の15件はすべて0〜32766。 |
| コースランキング | 過去に提出したコースの20件では配置・seed項目が無かった。もう1件の照会は空のランキングだった。 |
| `/v1/ghost` | LR2の1件で200と`lr2-ghost-text`、`seed_scheme: lr2`、英数字42文字のデータを確認した。BMZの1件は200と`ghost: null`。形式が不正な`score_id`では400と`invalid_score_id`。確認した応答にレート制限ヘッダーは無かった。 |
| `/v1/rivals` | 登録ライバルのある応答は得られず、`rival-scores`の形式は未確認。 |

beatoraja本体は`P1 + P2 × 2^24`をスコアに保存する。BMZの現行送信で通常の配置seedがある場合は`play_options.seed`と`play_options.random_seed`が同値なので、ランキングの読み取りからサーバーの採用元は判別できない。[beatorajaのスコア作成](https://github.com/exch-bms2/beatoraja/blob/master/src/bms/player/beatoraja/play/BMSPlayer.java#L947-L952)

BMZ側は判定イベント順に記録したghostをgzip・URL-safe base64で送る。配列で返すときにイベント順・コード・長さが保持されるかは、`ghost: null`だった今回のBMZスコアからは判断できない。運営者の説明では自己ベスト1件の保存であるため、同じIDで取得したゴーストの永続性も別途確認する。[BMZの判定履歴](../../crates/bmz-gameplay/src/score.rs) [BMS-IR運営者の回答](https://github.com/hyrorre/bmz-player/issues/8#issuecomment-6887601644)

LR2のスコアは`rseed`を保持する。運営者の説明ではLR2形式の値をそのまま返し、本番の値域も整合した。サーバー内部の列との突き合わせは残る。PRの実装は、スクラッチを含めて回す設定が不明なため鍵盤だけを回し、seed 0は有効として処理する。LR2には0を未設定として再抽選する経路もある。[BMS-IR運営者の回答](https://github.com/hyrorre/bmz-player/issues/8#issuecomment-6887601644)

## 運営者に確認が残る項目

- `beatoraja-judge-history`配列のコード、ノーツ順、長さと、`lr2-ghost-text`が`score.db`の`ghost`をそのまま返すか。
- 有効な形式の未登録IDでの`/v1/ghost`応答、レート制限、同じ`score_id`でベスト更新などによりゴーストが変わる条件。
- ランキングの`random_seed`の採用順序、LR2行の`rseed`との対応、コースランキングのseed項目の契約。
- `rival-scores`の`play_seed`が取り得る形式と、`seed_scheme`の有無。

ゴースト再生は別の変更で扱う。LR2スコア取り込み時の既存ラベルも別件とする。

## 検証

Windowsで`cargo fmt --check`、`cargo check --workspace --locked`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked --no-fail-fast`が通過した。全体テストはローカル通信とフォルダーリンクを使える環境で実行し、`bmz-player`は3,299件成功、91件はignoredだった。プレイ画面での対戦は未確認。

## 配置seed変換のレビュー対応（2026-11-17）

- 譜面読み込み時にLR2方式で分岐と鍵盤配置を解決する。確定した並びに一致するbeatoraja形式のseedを既存の探索で求め、そのseedを配置・保存・IR送信に使う。DPは各サイドを独立に変換する。
- `RIVALCHART MIRROR`の反転は、相手の配列を確定した後に行う。ランキングから直接選んだ相手と、既存の配置パターンを持つリプレイの両方で、LR2の分岐解決を維持して反転できる。
- 以前の記録などで再現できる配置seedが無い場合、rianIRのリクエストでは`play_seed`を省略する。明示された0は引き続き送る。

Windowsで整形、`bmz-player`のcheck・all-targets Clippy・テストが通過した。単体テストは3,303件成功、91件ignoredだった。7Kの反転、DP FLIPを含む両サイドのseed再現、3つのIRの送信キュー、BMS-IRの模擬HTTPリクエストを確認した。実プレイと本番送信は未確認。

## 同一配置リトライの再反転への対応（2026-11-17）

リザルトとFAILED演出からの同一配置リトライは、初回の自分側の配置を復元する。相手の配置設定は再適用せず、LR2の分岐用seedと確定した両サイドのseedを保持する。DPで固定配置とS-RANDOMなどを併用する場合は、固定側のレーン表だけを当てず、両側を記録済みseedで復元する。

ZIP内の14K譜面を再読み込みするテストで、1Pの反転済みseed `2975`、2PのS-RANDOMを含む全レーンのノーツ配置と分岐選択の一致を確認した。Windowsで整形、`bmz-player`のcheck、all-targets Clippy、テストが通過した。単体テストは3,304件成功、91件ignoredだった。プレイ画面からの操作は未確認。

### beatoraja形式の混在DP

同じZIPテストに、beatoraja形式の対戦相手を `RIVALCHART` で選び、1PをRANDOM（seed `77`）、2PをS-RANDOM（seed `89`）にするケースを追加した。修正前は2Pのレーンで、初回に1ノーツ、リトライ後に9ノーツとなった。

固定側だけを含むレーン表を配置の適用時に識別し、同一配置リトライでは確定済みのseedから両側を再生成する。ZIP再読み込み後の全レーンでノーツのIDと時刻を比較し、分岐選択と配置seedの一致も確認する。

Windowsで整形、`bmz-player`のcheck、all-targets Clippy、テストが通過した。単体テストは3,304件成功、91件ignoredだった。プレイ画面からの操作は未確認。
