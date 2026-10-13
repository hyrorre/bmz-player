# LR2選曲の未所持譜面・コースのバー画像

## 症状と原因

WMIX_HDで未所持譜面がRIVAL、コースがNEW SONGの画像で表示されていた。
LR2のCSV読込はバー本体の0..7をそのまま保持し、8以降を通常フォルダで埋めていた。
描画側はbeatorajaの番号（未所持=4、コース=3）をLR2にも適用していた。
LR2では4がライバルフォルダ、3が新曲フォルダなので、異なる素材が選ばれていた。

根拠は `.local/OpenLR2/LR2/Scene02_Songselect.cpp` の
`src_BAR_BODY[folderType]` による描画と、
`data/skins/WMIX_HD/select/select.csv` の `SRC_BAR_BODY` 定義。
第三者スキンのCSVや画像は編集していない。

## 変更

LR2の変換結果は本来の0..9を保持し、描画側でBMZの行種別をLR2用に解決する。

| BMZの行 | LR2の画像番号 | WMIX_HDでの見た目 |
|---|---|---|
| 未所持譜面・未所持コース・NoSong | 5 | 機能フォルダと同じ灰色 |
| コース | 8 | 灰色 |
| ランダムコース | 9 | 黄色のCOURSE |
| 設定フォルダ・戻る・閉じる | 1 | 青色のFOLDER |

5はLR2ではライバルモードの曲バーであり、BMZでは機能フォルダにも使用している。
未所持項目に5を使うのはBMZの表示方針。曲名と未所持の状態は維持する。
beatoraja形式の未所持=4、コース=3という契約は維持する。
現在の仕様は [skin.md](../../docs/skin.md#lr2のselect--result) に記載する。

## LR2の機能フォルダ

LR2にもカスタムフォルダがあり、参照ソースではRANDOM、FAVORITE、TOP10、PLAYLEVEL、
CLEAR、RANK等の定義を読み込む。`.lr2folder` の条件で曲を抽出する。
通常フォルダは画像1、カスタムフォルダは2、新曲フォルダは3、ライバルフォルダは4、
コースフォルダは6を使用する。WMIX_HDでは順に青FOLDER、緑CUSTOM、NEW SONG、
赤RIVAL、黄COURSEになる。
BMZのLAMP UPDATE、DENSITY、FEATURE等の組み込み仮想フォルダは
[select-folders.md](../../docs/select-folders.md) に記載された定義に従い、灰色の機能バーを使う。

## 検証

- CSV変換が画像8/9を保持する回帰テストと、LR2の未所持・コース・ランダムコース・設定行を
  実際の描画項目へ変換する回帰テストを追加した。
- WMIX_HDのCSVと画像素材から、画像5/8が灰色、9が黄色であることを確認した。
- `cargo fmt --check`、`cargo check --workspace --locked`、
  `cargo clippy --workspace --all-targets --locked -- -D warnings` は成功。
- `cargo test --workspace --locked --no-fail-fast` ではbmz-renderの731件とbmz-skinの273件が成功。
  bmz-playerで実行タイミングを扱う `autoplay_and_replay_are_invariant_to_render_stalls` と
  Viewerパイプの `play_stop_and_quit_requests_round_trip_to_persistent_listener` が失敗したが、
  それぞれ `cargo test -p bmz-player --locked --lib <test-name> -- --exact` で個別実行すると成功した。
  全体テスト自体の終了コードは1であり、全件成功とは扱わない。
- GPUでの実画面確認は未実施。
