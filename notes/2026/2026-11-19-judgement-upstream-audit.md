# 判定・ゲージの上流互換監査

監査基準はBMZ `ab1808788f5d6dcc639cd6013b370463bccdda3e`。作業ツリーの基点は
`4cd70074bff426dac58d6515906f711c99ba1953` で、監査基準より前のDX POP窓を持っていた。
DX POPの窓と旧リプレイ用の分岐は固定上流SHAと監査基準に従い復元した。

## 参照

| 系統 | 固定コミット | 判定・ゲージの確認箇所 |
|---|---|---|
| beatoraja | `93e704d9eedcb3b52e558f6581c00d6077ceb639` | `AbstractJudgeManager.updateMicro`・`processFifoEvents`、`JudgeManager` の候補走査・見逃し、`JudgeAlgorithm`、`JudgeProperty.JudgeWindowRule`、`BMSPlayerRule.validate`、`GrooveGauge.MODIFY_DAMAGE`、`GaugeProperty` |
| LR2oraja | `c1c8e4e0148abd318a9375de1631227cc383d0e4` | `lr2oraja` ブランチの `JudgeManager`・`MultiBadCollector`、`JudgeProperty.createLR2`、`BMSPlayerRule.validate`、`GrooveGauge.create` |
| DX MODE | `3bc4b9b40dc6a44f7ebf0594a6ec1b8ffb7370fe` | `core/src` の `JudgeProperty.IIDX_RULE`・`POP`、`JudgeManager` のDX倍率・通過・MultiBad、`BMSPlayer` のAssist判定 |

beatoraja 0.8.9 `83574048ba489082eb46df7fe9806b464bf1f526` と
ED-rian 1.7.0 `6f81c2b78e3de545b2ed86c7651a1479aa14395f` も比較用に取得した。
採用する判定窓・式の基準は表のコミット。

## 互換性の確認箇所

- BSSの同方向Releaseと、直後30,000μs以内の最初の逆方向Pressの吸収はBMZの従来仕様。
  同時刻の地雷はこのエッジで起動せず、その後の保持で通過した地雷は通常どおり判定する。
- 固定LR2oraja SHAの `EXHARDCLASS_LR2` ではコンストラクタの `border` が0、
  `death` が2。ゲージが2%未満で閉店し、2%ちょうどは継続する。
  [GaugeProperty](https://github.com/wcko87/lr2oraja/blob/c1c8e4e0148abd318a9375de1631227cc383d0e4/src/bms/player/beatoraja/play/GaugeProperty.java#L117-L168)
  · [GrooveGauge](https://github.com/wcko87/lr2oraja/blob/c1c8e4e0148abd318a9375de1631227cc383d0e4/src/bms/player/beatoraja/play/GrooveGauge.java#L215-L223)
- 固定DX SHAのMultiBadで終端時刻より前の始点を追加BADの対象から外す分岐は
  `isHellLongNote` が判定するHLN（`TYPE_HELLLONGNOTE`）に適用される。
  HCN（`TYPE_HELLCHARGENOTE`）はCNと同じ経路で、始点がMultiBadで消費されると、
  未処理の終端は通常ノーツのlate BAD幅を過ぎた時点でPOORになる。
  [ノーツ型](https://github.com/rian-eimu/lr2oraja-endlessdream/blob/3bc4b9b40dc6a44f7ebf0594a6ec1b8ffb7370fe/core/src/bms/player/beatoraja/play/JudgeManager.java#L249-L269)
  · [MultiBad](https://github.com/rian-eimu/lr2oraja-endlessdream/blob/3bc4b9b40dc6a44f7ebf0594a6ec1b8ffb7370fe/core/src/bms/player/beatoraja/play/JudgeManager.java#L1005-L1021)
  · [見逃し](https://github.com/rian-eimu/lr2oraja-endlessdream/blob/3bc4b9b40dc6a44f7ebf0594a6ec1b8ffb7370fe/core/src/bms/player/beatoraja/play/JudgeManager.java#L1346-L1395)
- DXでは `JudgeManager` が任意倍率を無視し、`BMSPlayer` は100%超の設定をAssistとして扱う。
  設定を保持し、同じ保存・IR条件を使う。

## レビュー後の追従

固定SHAとの行単位の比較で見つかった差を次のとおり直した。

- beatoraja／LR2orajaは判定用の`JudgeWindow`を鍵盤側の倍率だけで作る
  （beatoraja `AbstractJudgeManager.java:158`、LR2oraja `JudgeManager.java:200`）。
  BMZもスクラッチとlong scratch endに鍵盤側の倍率を使う。
- 倍率を記録していない対戦相手のリプレイは、本人のリプレイと同じく現在の設定とコース制約を使う。
- ED `JudgeManager.java:963` はMultiBadの候補を`state == 0`だけで集めるので、
  9K DX POPでは非消費BADの履歴があるノーツにも再度BADを付ける。
- ED `JudgeManager.java:1008-1013` に合わせ、MultiBadの対象になったHLNは押下が終端時刻以降のときだけBADにする。
  終端前のHLNを始点BADとして捕捉しない。
- ED `JudgeManager.java:973` に合わせ、LR2oraja／DXのLN遅BAD除外をHLNの始点にも適用する。
- beatoraja `JudgeManager.java:486-500` と`finishTime`に合わせ、LNのGOOD以内の早離しを終端時刻まで保留する。
  終端前の押し直し（同322行）で取り消す。

地雷の通過判定は、上流ではチェックポイント（beatorajaは入力時刻とフレーム終了、
LR2oraja／EDはフレーム終了）の保持状態を使うため、フレームの刻みで結果が変わる。
BMZは時刻付き入力で判定し、ライブとリプレイの再採点を一致させる。
上流のフレーム依存の挙動には合わせないと決めた（2026-11-19）。

## 結果の読み方

| 再現 | 監査時 | 修正後の期待値 |
|---|---|---|
| 9K 通常LNの非消費BAD後に取り直す | PGREAT×1、BP 0 | BAD×1、PGREAT×1、BP 1、ghost 1件 |
| 9K 700／750msの空POORと見逃し | 空POOR×2、POOR×1、BP 3 | 空POOR×1、BP 1、見逃しで消費・ghost POOR |
| 9K 非消費BAD後に2ノーツ処理して中断 | 保存BP 1 | 処理済み2、ライブBP 1、保存BP 2 |
| 7K DURATION 1000／1380ms | BAD×1、BP 1 | 空POOR×1、POOR×1、PGREAT×1、BP 2 |
| LR2oraja／DX 7KのCN・HCNのMultiBad後の孤立終端 | BAD×2、ライブBP 2 | BAD×2、POOR×1、ライブ・CLEAR保存BP 3 |
| DX NO_GOODで990／1000ms | BP 2 | PGREAT×2、BP 0 |
| LR2oraja BMSON 100%、1000採点ノーツ | TOTAL約460.909 | TOTAL 352、NORMAL PGREAT +0.352 |

判定差が確定した範囲は上の入力列と条件。通常7KのCOMBOで通常ノーツだけをプレイした場合の
BP報告には、入力の実測、音声と入力の共通時計、描画と判定の位置関係の比較が必要。

## 検証

Debian Linux上でRustのワークスペースを検証した。全体テストでは同梱スキンの
サブモジュールを初期化し、ローカルHTTP通信と日本語ロケールを使った。
整形、workspace check、全ターゲットClippy、アプリのビルドが通過した。
`bmz-gameplay` 352件と `bmz-chart` 211件が通過した。全体テストで見つかった
ジャストをSLOW扱いしていた保存テストのfixtureを修正し、判定、コース、リプレイ、
結果確定、保存のアプリ側テスト358件を再実行して確認した。
譜面スキャンの別領域に、親ディレクトリを指すシンボリックリンクの走査と
読み取り不可ディレクトリの警告数に関する失敗が残った。
入力機器・音声・画面の実機計測と、GPUや外部素材を前提にした任意テストは未実施。

現行仕様は [判定とゲージ](../../docs/rule.md)、[LN](../../docs/ln.md)、
[実行経路](../../docs/gameplay-runtime.md)、[スコア保存](../../docs/score-persistence.md) を参照。

## 2026-11-20 の追加照合

PR #140 の `8ddda0ca` で見つかった差を、同じ固定上流と比較した。
beatorajaとLR2orajaの同梱パーサーにランク未指定のBMSを渡した結果は、いずれも `BMS_RANK 2`。
ED-rian側にも同じ[モデル初期値](https://github.com/rian-eimu/jbms-parser/blob/b18d8129969ab2036d3d9c5dd613901492553265/src/bms/model/BMSModel.java#L65-L72)がある。

| 条件 | 修正後の期待値 |
|---|---|
| beatoraja 7Kでランク未指定・18ms遅れ | BMSはGREAT、BMSONはPGREAT |
| beatoraja 9Kで通常LNの終点170ms前に離す | 200msの猶予が切れる前に終点を通過しても、離した時のBADを1回確定 |
| DX 9Kで1000／1500msの通常ノーツを850／1150msに押す | 2回目は先行ノーツのBADと次の空POOR。通算BP 3 |
| DX 7Kの対戦相手がNO_GOODで80ms遅れ | 毎フレームの再計算後もBAD |
| 両方向の皿を押した後、片方だけ離して地雷を通過 | 本人・相手とも被弾。両方離した後の地雷は回避 |
| beatoraja 5K／10KのBSSでランク100%、倍率100／50／100% | 終端のPGREAT・GREAT・GOOD幅は各130ms。145ms前の切り返しはBAD |
| LR2oraja PracticeでPMS／NORMALを選択 | 初期30、上限120。カテゴリ未指定時はLR2 |
| beatoraja 7KのGOOD幅がFAST 165ms・SLOW 210ms | JUDGE AREAの接近側の端は165ms |
| LR2oraja BMSON 100%、1000採点ノーツ | PRELOAD・Play・ResultのTOTALはすべて352 |

Windowsで修正後のworkspace check、全ターゲットClippy、全体テスト（5873件成功、144件ignore）を確認した。
同梱7K譜面を判定倍率100／100／200%、JUDGE AREA有効で184フレーム書き出し、判定帯の表示を確認した。
境界の数値は判定エンジンと描画スナップショットの回帰テストで確認した。物理コントローラーでの実プレイは未実施。

## 2026-11-20 のPractice旧設定の再読込

`c32fefa2` では、形式1の9Kの設定（PMS・NORMAL・開始117）をLR2orajaで開いて保存すると、
同じ譜面をbeatorajaで開き直しても開始100・上限100になった。先にbeatorajaで保存すると、
今度はLR2orajaでもPMS・開始117になった。ルール間で同じ設定ファイルを使うため、最初の自動保存が影響していた。

旧設定は共有カテゴリと開始値を保持し、Practiceの画面と区間プレイでルールに応じた値を使う。
形式0の判定ランクは形式1へ移行し、LR2orajaでカテゴリを切り替えたときに形式2へ移行する。
形式2としてすでに上書きされたファイルには、以前のカテゴリと開始値を特定できる情報がない。

Windowsの回帰テストで、beatoraja・LR2oraja・DXをそれぞれ最初に開き、
区間プレイのゲージ値と次の読込まで確認した。egui画面のLR2・開始100表示、
上限でのキー操作、PMSを明示的に選択したあとの保存と再読込も確認した。
整形、`bmz-player` のcheck・全ターゲットClippyが通過した。同crateの全テストは3379件成功、96件ignore。

`dc034d9b` の時点では、新規の設定（形式2）を読み込むと、そのルールの既定カテゴリを保存していた。
beatorajaやDXで作った7Kの設定をLR2orajaで開くとSevenKeys、LR2orajaで作った設定をbeatorajaで開くとLR2になった。
カテゴリを選ぶまでは保存せず、開いたルールの既定を使うようにした。開始ゲージも旧形式と同じく保存値を保ち、
区間プレイでは各ゲージが自分の上限に収める。GASで選んだゲージより上限の高いゲージは、保存値のまま始まる。
