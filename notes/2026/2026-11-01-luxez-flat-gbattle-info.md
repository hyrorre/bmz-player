# Luxez-FlatのG-BATTLE相手情報とSP/DP配置切り替え

## 依頼と配置

- SPは選択済みのRIVAL DATA統合案、DPは左下の詳細パネル案を実装する。
- SPの413×150pxには配置オプション、鍵盤順、CHART REPLICATIONの値を表示する。
- 10K/14Kは左下412×284pxのIR DATA / SCORE DETAIL本文と切り替える。
  1P/2Pのオプションと鍵盤を上下に並べ、DP OPTIONとCHART REPLICATIONを下に置く。
  右のRIVAL DATA / SCORE DATAと上部のライバル選択を維持する。
- 相手名、重複する項目名、HARD等のゲージ名は追加しない。
- 追加指摘に合わせ、鍵盤はResultと同じ等倍25×35px・27px間隔（隙間2px）にする。

現在の契約は[skin.md](../../docs/skin.md#g-battle一覧の相手情報)、対応範囲は
[skin-compatibility.md](../../docs/skin-compatibility.md)を参照する。

## 素材と実装

`data/skins/Luxez-Flat/select_skinparts/default_battleinfo/parts.lua`を追加した。
Luxez-Flat側の対象commitは`ffe2745`。
20200..20229の既存相手情報APIとtext 86を利用し、自分の設定で欠損を補完しない。

- 枠はdefault_scoredata/rival_data_resize.pngとdefault_modebotton/score_detail_panel.png。
  未使用の縦帯と外周をLuaで組み合わせ、元の透過・縦グラデーション・発光・角丸を保持する。
  SPのRIVAL DATA見出しは元画像のまま使う。
- オプション名はdefault_stateplayoption/stateoption_resize.pngとrandom_bmz.pngの文字領域。
  元の余白を切り、文字を拡縮しない。
- 1..7の鍵盤はresult/png/random_pattern.pngの25×35pxを流用する。
  8/9/Sは同じ画像の無地帯・外周とfont_subを組み合わせる。
- DP見出しは既存VL-Gothic、補助文字は既存font_sub_small。
  NO DATA / LOADING / NOT FIXEDはnative NO PLAYのRGB(127,128,128)と同じ色にする。
- imagesetのvalueにIntegerPropertyを渡す。画像インデックス用refとは別の名前空間であり、
  数値refをrefへ直接渡すと全鍵盤が先頭の画像になるため、描画テストで鍵盤のUVも確認する。

load.luaは正常に読み込んだ場合だけ、所有モジュールと元の配置範囲で特定した本文を切り替える。
DPではモーダル用のtimer・loop・mouseRectを持つdestinationを対象外にする。
既存drawを先に実行し、その結果へ表示条件を足す。状態を更新するcallbackを失わず、
部品の無効・欠落・失敗時やBMZ以外では元の本文を残す。
PNGの加工、新規フォントの同梱、本体の相手情報API変更は行っていない。

## 検証

実際のサブモジュール素材を使う回帰テストを
`crates/bmz-player/src/skin_loader/tests/luxe_battle_info.rs`へ追加した。
Auto / Compatの両方でSP↔DP、DP FLIPの側内番号、読み込み状態、灰色、元パネルの復帰、
鍵盤画像の番号・等倍サイズ・間隔を確認した。9Kの8/9とRANDOM-EXのSも確認した。

Windowsのoffscreen GPU描画で完成画面を確認した。プレビューには固定したサンプル情報と
既存のスキン外観設定だけを使い、実プロフィール・スコアDB・IRデータを書き換えない。
画像と作業用ツールはGit管理外の`.local/notes/2026-11-01-gbattle-layout/luxez-implemented/`、
`.local/lr2-play-audit/`に置く。実IRでの手動選曲操作、macOS/Linuxの実機確認は未実施。

- `cargo fmt --check`: 成功。
- `cargo check -p bmz-player --locked`: 成功。
- `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`: 成功。
- `cargo test -p bmz-player --locked`: 2,781件成功、48件ignore。別のstartup smoke 2件はignore。
  既存のローカルHTTPテストも実行した。詳細ログは上記ローカル画像ディレクトリのplayer-tests.log。
- モーダル除外条件を追加後、Luxez関連を再実行し10件成功・3件ignore。完成画面も再描画した。

## 枠の継ぎ目の修正（2026-11-01追記）

SPのRIVAL DATAとDPのG-BATTLE INFOで、本体の左右の縁が細い1本線になり、
下の角では縁の内側の発光が下端19pxにだけ急に現れて、色の段差が出ていた。

- 元画像の左右には、外側の影、2pxの明るい縁（x=9〜10）、その内側の約25pxの発光がある。
  旧版は左右10pxだけを残し、その内側から中央の1列を伸ばしていたため、発光が本体で消えていた。
- 左右36px（影と縁11px、発光25px）を等倍で描き、平坦な中央の1列だけを伸ばす。
  下端も左右の角36pxを等倍にし、中央だけを伸ばす。縦方向はどの部品も等倍のままにする。
- 左の発光はSCORE/BREAK/LAMPの印（DPは判定ラベルの緑の印）の光で色付いている。
  文字のない右の発光を負の幅で左右反転して置き、外側の縁11pxは元の左側をそのまま使う。
- DPの見出しは緑の点の光がx=32で切れていたため、元のタイトル文字の直前のx=36まで等倍にした。
  光は元のタイトル文字の下まで続くため、x=36に緑で約10段階の差が残る。

Luxez-Flat側の対象commitは`3b14175`。詳細オプションの枠（`196a6ad`、`5930317`）と同じく、
縁の発光を伸ばさず中央だけを伸ばす。

### 検証

- 回帰テスト`luxe_battle_info_frames_keep_the_native_edge_glow_continuous`を追加した。
  描画コマンドから枠の各画素がどの元画素を描くかを組み直し、各画素を1回だけ描くこと、
  縁・右の発光・下の角・見出し左端が元画像と同じ画素であること、左の発光が右の鏡像であることを確認する。
  修正前のparts.luaでは縁の2px目（10, 38）が中央の色になって失敗することを確認した。
- ImageMagickで同じ組み方を再現し、元画像と異なるのはラベル・印の光・区切り線だけであることを確認した。
- Windowsのoffscreen GPU描画でSP/DPを比較し、発光が下の角までつながることを確認した。
  一覧を閉じた画面は、修正前と比べて右上の時計以外に差がない。
  画像とログはGit管理外の`.local/notes/2026-11-01-gbattle-layout/frame-before/`、`frame-fixed/`に置く。
- `cargo fmt --check`、`cargo check -p bmz-player --locked`、
  `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`: 成功。
- `cargo test -p bmz-player --locked`: 2,782件成功、48件ignore。別のstartup smoke 2件はignore。
- 通常起動での目視、macOS/Linuxの実機確認は未実施。
