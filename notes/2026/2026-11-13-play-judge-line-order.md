# ECFN・Qwilightの判定線とノーツの描画順

## 目的と結論

ECFN向けに追加されていた判定線の前面化を再検証し、スキンの定義順を保つようにする。
調査開始時のHEADは`99979ac13ac209a767494305436b1f58675527db`。

前面化は不要。ECFNもQwilightのCR_minimalも、判定線をノーツより先に描く定義になっている。
BMZが寸法・位置から判定線を推定して手前へ移す処理を削除する。
画像の読み取り・LIFTの座標補正は維持する。
現在の契約は[スキン仕様](../../docs/skin.md#判定線とノーツの描画順)を参照。

## 元の補正の由来

`f3616c30563af24679afa4d78ce3b2f5d1644ebb`（2026-06-03、`show ECFN judge line`）は、
次の二つを同時に変更していた。

- `src=0`を無条件に黒画像扱いせず、`x=16,y=0`などの明示的な切り出しを保持する。
- `notes`直前の、timerなし・LIFTのみ・幅100以上・高さ1〜48・判定線付近の画像をノーツより手前へ移す。

前者はECFNの実画像を表示するために必要。後者はスキン形式に関係なく作用し、
CR_minimalの`qw-paint-10`も誤って手前へ移していた。今回削除するのは後者だけ。

## 参照コードと実スキン

ローカル参照コピーを読み、次の経路を確認した。第三者スキンのファイルは変更していない。

- ECFN `data/skins/ECFN/play/play7main.lua:2106`は、`id=15,offset=3`の直後に`notes`を登録する。
  画像15は`src=0,x=16,y=0,w=8,h=8`。5K・14K・7K wideも判定線が先。
- beatoraja `.local/beatoraja/.../skin/json/JsonSkinLoader.java:316`は`destination`の順に`skin.add`する。
  `skin/Skin.java:325`は登録順に描画し、`play/SkinNote.java:60`の時点でノーツを描く。
  判定線を後回しにする特例はない。
- インストール済みCR_minimalの`CR_minimal.yaml:274`は`...,31,110,7,50,...`。
  31はJudgmentMain、110は赤い判定線の`paintProperty10`、7はNote。
  `.local/qwilight-reference/DrawingSystem.cs:728`は`PaintPipelineValues`を順に処理する。
- BMZのQwilight変換は既に`pipeline`順を保持している。
  renderer側の前面化を削除すれば、判定線と装飾を本来の順序で描ける。

## 回帰確認

- rendererのテストは、同じLIFT付き判定線を`notes`の前後に置き、両方で定義順を保つことを検証する。
- Qwilightの合成YAMLでも`31,102,7`と`31,7,102`を比較し、装飾画像の前後関係を検証する。
- 実ECFNのloaderテストは、`src=0`の切り出し座標・UVを保持したまま背面へ入ることを検証する。
- `src_zero_with_explicit_crop_keeps_pixel_rect`で、元の黒画像誤認の修正が残ることを検証する。

実素材GPUテストは`skin_loader::tests::play_layer_order::installed_play_judge_lines_render_behind_notes`。
外部素材・GPUが必要なため通常はignoreし、環境変数を指定して明示的に実行する。
`BMZ_QWILIGHT_UI_DIR`はQwilightのUIディレクトリ、`BMZ_PLAY_LAYER_PREVIEW_DIR`は画像出力先。
ECFNは`data/skins/ECFN`を使用する。
外部ファイルを変更せず、読み込み後のメモリ上で判定線とノーツだけを残し、
ノーツなし／判定線と重なるノーツありのGPU画像を比較する。

### Windowsでの実行結果

合成Qwilightの前後順序テストと、ECFN 7K・CR_minimal 6Kの実素材GPUテストは成功。
GPU画像は1920×1080、ノーツはKey1に一つ、LIFTなし、既定のスキンオプションで比較した。

| スキン | 判定線の可視画素 | ノーツが上から覆った画素 | そのまま見える画素 |
|---|---:|---:|---:|
| ECFN 7K | 3448 | 408 | 3040 |
| CR_minimal 6K | 15120 | 2408 | 12712 |

画像は`.local/play-layer-order-previews/{ecfn,cr-minimal}-{line,overlap}.png`へ保存し、
両方の重なり画像も確認した。画像・外部素材はコミットしない。
通常の実ウィンドウでの手動プレイ、beatoraja／Qwilight実行画面の比較、他OSは未実施。
参照元との比較は上記コード・定義、BMZの表示確認はオフスクリーンGPU描画の範囲。

以下もすべて成功した（`CARGO_INCREMENTAL=0`）。

- `cargo fmt --check`
- `cargo check --workspace --locked`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- `cargo test --workspace --locked --no-fail-fast`
  - bmz-player: 3111 passed / 80 ignored、bmz-render: 1028 passed / 24 ignored。
  - ECFN実素材のloaderテスト、判定線の順序テスト、`src=0`のcrop保持テストを含む。

ignoreされた実素材GPUテストは上記のとおり別途明示実行した。
全体検証ログは`.local/play-layer-{fmt,check,clippy,test}.log`（ローカルのみ）。
