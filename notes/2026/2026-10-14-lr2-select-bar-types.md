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

## 2026-10-14 追加調査後の修正

上の修正ではNEW COURSEの行種別がsnapshotで失われ、コース一覧・機能フォルダには
beatorajaの番号を流用していた。機能フォルダを灰色とした前の説明はLR2の分類に一致しない。
`codex/lr2-select-row-compat` Worktreeで、LR2の行種別と実際のWMIX_HD素材を再確認した。

参照したLR2コードは `LR2_songmanage.cpp` のカスタムフォルダとコース行の分類、
`Scene02_Songselect.cpp` のバー・タイトル・ランプ・勝敗表示、
`LR2_skinobject.cpp` の選択行条件1..5、`LR2_configsave.cpp` のtitleflash既定値。

| BMZの行 | LR2の画像番号 | WMIX_HDでの見た目 |
|---|---|---|
| コース一覧 | 6 | 黄色のCOURSE |
| NEW COURSE | 7 | 黄色のCOURSE |
| 保存済みコース | 8 | 灰色 |
| RANDOM MIX | 9 | 黄色のCOURSE |
| 仮想フォルダの分類階層 | 1 | 青色のFOLDER |
| 仮想フォルダの抽出項目・検索履歴・難易度表 | 2 | 緑色のCUSTOM |
| NEW配下の抽出項目 | 3 | NEW SONG |
| 未所持譜面・未所持コース | 5 | 灰色（前の表示方針を維持） |

NEW COURSEはop 4、保存済みコースはop 3として評価し、op 5にはNEW COURSEを含めない。
抽出項目のランプ・曲数は実際に開くときと同じquery・limit・有効root・スコアルールで
既存の非同期workerから集計する。有効root変更時はキャッシュの世代を更新する。
分類階層の子孫ランプは合成しない。

譜面の初回登録日時をsnapshotへ渡し、既定24時間はBAR_TITLE 1を使う。
profileの `[select].new_title_hours = 0` で無効にできる。
ライバルのEXが1以上ある曲だけ自分・ライバルのランプを分け、それ以外は通常ランプを使う。
BAR_RIVALは自分とライバルのEX比較で勝ち・負け・同点・自分未プレイを選ぶ。
beatoraja形式の画像・タイトル番号は維持し、新しい行種別でも回帰テストで確認する。

### 確認範囲と残課題

- 外部スキンは編集していない。WMIX_HDをWorktree内のGit管理外へコピーし、個人のDB・configはコピーしていない。
- BAR_RANKはLR2のIRランキング一覧表示がBMZに無いため未対応。
- フォルダにはLR2の追加日時に相当する永続値が無く、BAR_TITLEは通常表示を維持する。
- 起動確認は独立した一時profile・DBで3フレームを表示するもの。全行の実画面での目視・操作確認は未実施。

### 今回の検証

- `cargo fmt --check`、`cargo check --workspace --locked`、
  `cargo clippy --workspace --all-targets --locked -- -D warnings` は成功。
- `cargo test --workspace --locked --no-fail-fast` は終了コード0。
  bmz-player 2428件、bmz-render 735件、bmz-skin 275件が成功。
  既定でignoredのテストは全体実行の成功件数に含めない。
- WMIX_HDのSelect・Result・CourseResultをdecodeする既存テストを素材ありで確認。
  通常LR2・Seraphic・ECBEの素材はWorktreeに無く、そのテストケースはskip。
- `cargo test -p bmz-player --locked --test startup_stack -- --ignored --test-threads=1` は
  合成LR2スキンとWMIX_HDの2件が成功。Windowsの実GPUで読み込み・初回描画・正常終了を確認。
- 全体テストの生ログはWorktreeの `.local/performance/lr2-row-tests-final.log` に保存。
  Git管理外のローカル資料であり、コミットには含めない。

## 2026-10-14 フォルダ表示とFAVORITE配置の追加指定

ユーザーの指定に従い、BMZの表示方針を同じWorktreeで更新した。

| 項目 | LR2画像番号 | WMIX_HDでの見た目 |
|---|---|---|
| NEWのルート・配下の階層・TODAY等の抽出項目 | 3 | NEW SONG |
| 設定フォルダ・その中の設定カテゴリ・戻る | 4 | 赤色のRIVAL |
| LAMP UPDATE〜LEVELの分類階層・抽出項目 | 2 | 緑色のCUSTOM |

NEWの `new_song` 指定をqueryの有無にかかわらず行種別へ反映する。
設定の画像4は外観の指定であり、ライバルのスコアや操作には結び付けない。
通常の曲フォルダは画像1、設定の「閉じる」は画像1を維持する。
beatoraja形式の画像番号は変えていない。

FAVORITEは登録がある場合にMY BESTの直後へ挿入する。
表示名ではなく `bmz-filter:my-best` で配置先を識別し、ソート後もフォルダの順序を維持する。
MY BESTが非表示またはカタログを読み込めない場合は、機能フォルダ群の末尾へ配置する。
お気に入りが無い場合はこれまで通りFAVORITEを表示しない。

ルート構築の回帰テストでNEWと機能フォルダの行種別、FAVORITEの有無、
MY BESTの改名・非表示、全8種類のソートを確認する。
描画の回帰テストでは設定ルート・設定カテゴリ・戻る・閉じると分類階層の画像番号を確認する。

追加指定後の `cargo fmt --check`、`cargo check --workspace --locked`、
`cargo clippy --workspace --all-targets --locked -- -D warnings`、
`cargo test --workspace --locked --no-fail-fast` はすべて成功。
全体テストのログは `.local/performance/lr2-folder-appearance-tests.log` に保存した。
Windows GPUの起動確認も合成LR2スキン・素材ありのWMIX_HDの2件が成功。
全項目の実画面での目視・操作確認は未実施。
