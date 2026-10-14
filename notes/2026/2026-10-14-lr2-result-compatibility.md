# LR2 ResultのCLEAR TYPE・左右切替・TARGET・DURATION

2026-10-14〜15。WMIX HDのResultで報告された6項目を、実CSV・画像とLR2参照ソースを基に修正した。
Playの透過色・SRC/DST・フレーム欠落については
[Playの修正記録](2026-10-14-lr2-play-compatibility.md)を参照。
第三者製スキン、利用者の設定・DBは変更していない。

## 原因と変更

- CLEAR TYPEの画像は4列で、汎用buttonの行選択では常に先頭のCLEARを表示していた。
  `image.lr2Button`で区別し、状態番号を横・縦共通のセル番号として扱う。
  FULLCOMBOの追加画像に使うResult op 100..105も、更新後の自己ベストのLR2ランプとして評価する。
- 自己ベスト側の画像とbitmap fontの`CONTINUE`を通常のファイル名として扱っていた。
  同じ番号のPlay素材について、実際に解決した絶対パスと透過色をResultへ渡す。
  Play読込時に同じScoreKeyのベストクリアも渡し、WMIXの画像23を正しく選ぶ。
  素材の引継ぎ元はResult入場時に固定し、IR更新の再読込では変更しない。
  ステージResultからCourse Resultへ進む場合はステージResultを使う。
  引継ぎ素材が変われば文書cacheを無効化し、存在しない画像は透明とする。
- Left側CSVの余分な`#ENDIF`が呼出元の分岐を閉じ、Right側CSVも読んでいた。
  IFのスタックをCSVファイルごとに独立させる。SETOPTIONとSRC/DSTの状態は共有する。
  呼出元の描画時条件はinclude全体に引き継ぎ、子ファイルのENDIFで失われないようにする。
  解像度推定と本変換に同じ規則を適用する。
- Result ref 154はLR2ではEXから次の整数境界を引く値。
  EX 2048 / 理論値2076はMAX −28、MAX到達時は0とする。
  AAAのランク番号は従来どおりで、JSON/Luaの正の必要点数も維持する。
- Resultの自分と反対側のref 121を未取得として隠していた。
  SPではターゲットの最終EX・率・差・ノーツ数・取得済み最大コンボを渡す。
  左右反転に追従し、判定数・ゲージ等の未取得値は補完しない。
- Result ref 163/164にPlayの残り時間0を渡していた。
  LR2 Resultだけを専用参照へ変換し、終了した譜面・コースの長さの分・秒を表示する。

引継ぎ状態はヒープへ置き、既存のWinitAppのスタックサイズ制約を維持する。

## 検証

CSVごとの分岐、SETOPTIONの共有、画像・フォントの番号と透過色、引継ぎパスとcache、
Result / Course Resultの入場と再読込、4×1 / 1×6 / 2×3のbutton、TARGETの反転、
MAX差の整数境界と到達時0、分・秒の回帰テストを追加した。
WMIXの実素材テストではLeft / Rightの片側だけを読むことと、
NOPLAY / HARD / FULLCOMBOに対応したPlay画像23を確認する。

### 自動検証

- `cargo fmt --check` 成功。
- `cargo check --workspace --locked --offline` 成功。
- `CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets --locked --offline -- -D warnings` 成功。
  Windowsのincremental cacheでhard linkが拒否される環境のため、Clippyはincrementalを無効にした。
- `cargo test --workspace --locked --offline --no-fail-fast` 成功。
  主な件数はbmz-player 2441、bmz-render 742、bmz-skin 291、bmz-skin-document 22。
  Windowsの一時ファイル・localhostを利用する全体テストはホストで実行した。
- 6系統9個のResult / Course Resultを保存済みカスタム設定で再変換し、変換エラー・警告は0件。

### 実素材のGPU描画

Windows / wgpuの1280×720オフスクリーン描画を使った。
PlayをベストFULLCOMBOの状態でdecodeし、その素材をResultへ引き継いだ。
ResultにはEX 2048、理論値2076、ベストEX 2073、ターゲット1846、長さ131秒、
HAZARD・FULLCOMBOの状態を注入した。
Left / Rightの画像を保存して確認し、自己ベストと今回のF-COMBO、MAX −28、
TARGET 1846と差202、DURATION 2:11、指定側への配置を確認した。

実BMSの演奏からResultへの操作、全カスタム設定、実コース遷移、macOS / Linuxは未確認。
対戦相手の実履歴やSCORECHART index 1/2は今回の対応に含まない。
生ログ・状態スナップショット・GPU画像は`.local/lr2-play-audit/`に保存し、コミットしない。

現在の契約は[スキン仕様](../../docs/skin.md#lr2のselect--result)と
[対応状況](../../docs/skin-compatibility.md)へ反映した。

## 2026-10-15追記: WMIXの難易度画像

Black Wings [Another]のResultでANOTHERが欠落する報告を追跡した。
登録済みmetadataと実BMSのimport結果はどちらもdifficulty=4であり、
元ファイルの`#difficulty 04`はimport時に数値4へ正規化されていた。
原因はResult読込状態にop 150..155を渡していなかったこと。
WMIXはこの条件で難易度画像のSRCを選ぶため、全分岐が無効になっていた。

描画時に使う共通の難易度変換をResult読込でも使い、150..155を毎回上書きする。
通常／コースResultの画像分岐とdecode cacheの回帰テストを追加し、
数値・名前の別表現、ANOTHER→INSANE→未定義の切替を確認する。
WMIX実CSVではLeft / RightそれぞれのANOTHER（atlas y=496）とINSANE（y=454）を確認する。
実素材テストがskipする環境はGPU確認済みと見なさない。

先のGPU確認は描画状態から全optionを注入しており、アプリの条件供給漏れを検出できなかった。
今回のGPU確認は`WinitApp::result_lua_runtime_state`と`scene_snapshot`を使い、
実アプリと同じ供給経路で画像を作る。
テストは[Result難易度の回帰テスト](../../crates/bmz-player/src/app/tests/result_difficulty.rs)に置く。

修正前は追加した3テストがすべて失敗し、WMIXの難易度画像0枚を再現した。
修正後は3テストが成功し、WMIX実素材の8条件（左右×数値／名前×ANOTHER／INSANE）を
skipせず確認した。通常／コースResultのcacheは、同じ難易度の数値→名前ではhitし、
ANOTHER→INSANE→未定義では再decodeすることを確認した。

Windows / wgpuでignoredのGPUテストも明示実行して成功。
合成した終了結果を使い、Left / Right × ANOTHER / INSANEの1280×720画像4枚を保存し、
下部の難易度ラベルを目視確認した。ゲームパッドの初期化はテストで無効にした。
画像は`.local/lr2-play-audit/difficulty-fixed/`、全体検証ログは同auditディレクトリに保存する。
実BMSの演奏からResultへ入る操作、全カスタム設定、macOS / Linuxは未確認。

今回の全体検証はfmt、workspace check、all-targets Clippy（`CARGO_INCREMENTAL=0`、
`-D warnings`）、workspace test（`--no-fail-fast`）がすべて成功した。
Cargoの検証には`--locked --offline`を付けた。主な成功件数はbmz-player 2456、
bmz-render 747、bmz-skin 291、bmz-skin-document 22。
