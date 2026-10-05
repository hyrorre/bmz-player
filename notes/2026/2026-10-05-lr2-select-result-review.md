# LR2 Select / Resultレビューの修正

対象ブランチ: `codex/lr2-select-result-compatibility`（レビュー時点 `5f9749e2`）。
現在の契約は [スキン仕様](../../docs/skin.md#lr2のselect--result) を参照。
Windowsで、外部スキンは変更せずBMZ側を修正した。

## 曲名の予約列

`DST_BAR_TITLE` のop1..3相当は予約列であり、OpenLR2の
`AddDrawingBuffer_TextXY`も表示条件として評価しない。
通常destinationへの変換前に予約列を無効化し、外側の`#IF`条件を残した。
回帰テストは予約列に1/2/3が入った場合と複数DSTの結合を確認する。

検証:

- `cargo fmt --check`、`bmz-skin`のcheck / all-targets Clippy / testが成功（245件）。
- 実アセットをappのdecodeとSelect描画評価へ渡して確認。
  WMIX_HD / ECBE / 3R / EndlessCirculation / PLATINUMのサンプル曲名は
  各1件から8件へ復帰。LR2 / Seraphic / RED_BELTは従来の件数を維持。
- 自動テストの外部アセット確認は作業領域に素材が無くスキップされるため、
  上記の実アセット確認はmain側の素材を読み込む別プローブで実施。
  ローカルログはmain側の`.local/lr2-fix1-repros.log`（共有されない）。
- GPU上の画面表示・実操作は未確認。

## 3R Resultのロード条件

レビューでは30/31をプレイ側と記述したが、参照実装の条件はBGA SizeのNormal/Extend。
3Rはこの条件をResultの左右レイアウト選択へ流用していた。
BMZのResultにはPlayスキンと共有するBGA Size設定がないため、ロード状態に
30=true / 31=falseを渡してNormal側を選ぶ。通常・コースResultの先読みも同じ状態を使う。

検証:

- 先読み・確定スコアの両状態、7K / 14Kでincludeが読み込まれる回帰テストを追加。
- `BMZ_TEST_LR2_SKIN_ROOT`でmain側の実素材を指定したappテストでは、
  3R Resultが66 destinations / 5 decoded sourcesとなった。
- fmt、bmz-player check / all-targets Clippy / test成功（2116件、ignore 6件）。
  最初の全テストは同梱素材の参照先不一致で3件失敗し、`BMZ_RESOURCE_DIR`を
  作業領域の`data`へ指定して解消。`BMZ_DATA_DIR`は隔離した`.local/lr2-test-data`を使用。
- ログはmain側の`.local/lr2-fix2-test-resources.log`。GPU画面・実操作は未確認。

## 確定ランクによる素材選択

Resultのロード状態へ300..308を渡し、確定したEX SCORE / 総ノート数から
描画と同じランク条件を計算する。前回ベスト320..327や相手側310..318とは分離する。
OpenLR2は`LoadSceneG`後に`FlipScore`を呼ぶため、ロード時条件は反転前の1Pを使う。

検証:

- AAA〜Fの境界、0点、0ノート、前回ベスト、`FLIPRESULT`を回帰テストで確認。
- cacheは先読み→AAA→AA→低ランク→AAAの順で正しい素材を選ぶ。
- app生成状態でRED_BELTの実素材を読込み、`BG/Result/AAA/RANDOM/w.png`と
  `BG/Result/AA/RANDOM/w.png`への切替を確認。
- fmt、bmz-player check / all-targets Clippy / test成功（2120件、ignore 6件）。
  素材・隔離データの環境設定は前項と同じ。
- ログはmain側の`.local/lr2-fix3-focused.log`と`.local/lr2-fix3-test.log`。
  GPU画面・実操作は未確認。

## 別テーマのinclude

`LR2files/Theme/<theme>/...`を解決するとき、テーマ名を除去する前に
同じライブラリ内の指定テーマを確認する。従来のリネーム済み自テーマへのfallbackは維持。
テーマ指定側にファイルがあれば自テーマ側の同名ファイルより優先する。

- 回帰テストで同名衝突、入れ子の相対include、読み込んだCSVの依存追跡を確認。
- 実RED_BELTの`OA_DX+/setting/yellow_gauge_setting.csv`と
  `ghost_battle_setting.csv`を読み込めること、include欠落警告がないことを確認。
- fmt、bmz-skin check / all-targets Clippy / test成功（247件）。
  実素材は`BMZ_TEST_LR2_SKIN_ROOT`で指定。ログはmain側の`.local/lr2-fix4-test.log`。

## 画像名末尾の余分な引用符

3RのCSVには`RED.png",,`のような定義があり、従来のparserは途中の引用符を
開始引用符として扱って後続カンマまで画像名へ取り込んでいた。
開始引用符はフィールド先頭だけで認識し、末尾の余分な引用符は既存のパス正規化で除去する。
正常に引用されたカンマ入りファイル名と、末尾コメントの扱いは維持する。

- synthetic CSVと実3RMain / 3RBTの定義で回帰テストを実施。
- appの画像decodeでも、両スキンの判定線（144×6）、ビーム（122×482）、
  キーフラッシュ（79×65）、発光（665×216）の4画像を確認。
- ローカルログはmain側の`.local/lr2-fix5-focused.log`、`.local/lr2-fix5-decode.log`。

## 最終検証と残る制約

- `cargo fmt --check`成功。
- `cargo check --workspace --locked`、
  `cargo clippy --workspace --all-targets --locked -- -D warnings`、
  `cargo test --workspace --locked --no-fail-fast`がすべて成功。
  ログはmain側の`.local/lr2-final-{check,clippy,test}.log`。
- テストの素材参照先と隔離データは前述の環境変数で指定。
  `BMZ_TEST_LR2_SKIN_ROOT`は今回追加した実素材テストの参照先のみを変更する。
  その他の既存外部スキンテストには素材不足による早期returnがある。
- 実機のGPU表示・入力操作、macOS / Linuxでの実行は未確認。
- 3R Selectの`reg.png` / `Shutter.png` / `parts.tga`、EndlessCirculation
  SE-Selectの`parts.tga`は指定先に実ファイルが無い。外部素材の追加・改変は行っていない。
  未対応命令・装飾等の範囲は [互換状況](../../docs/skin-compatibility.md) と仕様を参照。

## mainへの統合

`main`の`9eeca32e`へ、修正済みブランチの`8071b934`をマージした。
選曲イベントと描画状態の競合は、mainのDETAIL OPTIONSとLR2互換処理の両方を維持して解消。
記録の索引も両側の項目を残した。

LR2のパネル切替呼び出しをmainの新しい引数へ追従させ、パネル変更時に
終了キーの長押し状態を解除する。既存のパネル遷移テストも検証対象とする。

共有ビルド生成物には別worktreeの型情報が残っていたため、関連crateの生成物を
再作成した。mainに存在する`DisplayJudgementEvent::display_time`の認識不一致は
これで解消し、当該ソースの変更は不要だった。

統合後の検証:

- `cargo fmt --check`、`cargo check --workspace --locked`、
  `cargo clippy --workspace --all-targets --locked -- -D warnings`、
  `cargo test --workspace --locked --no-fail-fast`がすべて成功。
- bmz-playerは2186件成功・19件ignore、bmz-renderは679件成功・4件ignore、
  bmz-skinは249件成功。GPU依存等のignoreテストは実行していない。
- `BMZ_RESOURCE_DIR`はmainの`data`、`BMZ_TEST_LR2_SKIN_ROOT`はその`skins`を指定。
  `BMZ_DATA_DIR`は`.local/lr2-main-merge-test-data`へ隔離した。
  今回はmainの外部素材を参照し、追加LR2スキンの実素材読み込みテストも実行できた。
- ログは`.local/lr2-main-merge-{check,clippy,test}.log`。
  GPU画面・実操作とmacOS / Linuxでの実行は引き続き未確認。
