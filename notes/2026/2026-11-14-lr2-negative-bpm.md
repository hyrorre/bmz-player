# BMSの負のBPMをLR2互換で処理

負のBPMを参照するチャンネル08のオブジェクトから、正のテンポ変更と折り返し点を
生成する。時刻マップは絶対値でノーツ・音源・STOPを配置する。描画と手動判定は
それぞれスクロール座標、入力時刻を折り返す。正のBPMだけの変更では折り返し点を
更新しない。仕様は [ハイスピード](../../docs/hs.md#bmsの負のbpm) と
[採点の確定](../../docs/gameplay-runtime.md#採点の確定とresultのゲージ履歴) に記載した。

## 実譜面での再現

ローカルの `data/songs/OTHERS/mrym2014/foon_tournamix_2014.bms` を使用。
テストは実ファイルを別途用意した環境で実行できる。

```sh
cargo test -p bmz-player --locked --lib mrym2014_fixed_branches -- --ignored
```

| 分岐 | BMZの `--seed` | 確認する挙動 |
|---|---:|---|
| 8 | 72 | 負のヘッダ定義が未参照なら折り返し点を生成しない |
| 26 | 1 | 開始83 BPM、50ノーツ、開始余白前の最終ノーツ約694 ms |
| 72 | 117 | 83 BPMでLN終端が約266.893秒 |
| 81 | 70 | 45 BPMへの切替でノーツ座標が折り返し、入力再生でFAST 1 msを再現 |
| 128 | 201 | 830 BPMとSTOPを共に保持 |
| 133 | 94 | 256 BPMで折り返し始め、69 BPMへ変わった後も継続 |

133分岐をそれぞれ固定して取り込み、36分岐の折り返し点を確認した。
BMZの開始余白込みの位置を基準に検証した。81番では入力バックエンドにオフセット付きの
打鍵を送り、記録したリプレイの再生でも同じ判定を確認した。133番は既定スキンを使い、
macOSのMetalで音声付きの動画を292フレーム出力した。LR2実機での開始処理、
フレーム境界、判定窓や描画の細部との比較は未実施。

変更前の `4a4fb59a` でも、曲スキャンの `overlapping_external_links_read_every_folder_once` と
`scan_song_roots_skips_unreadable_subdirectory_and_keeps_accessible_files` が失敗した。
