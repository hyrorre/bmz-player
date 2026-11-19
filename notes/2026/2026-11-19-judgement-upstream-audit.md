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
