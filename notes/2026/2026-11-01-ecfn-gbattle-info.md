# ECFNのG-BATTLE相手情報

別途導入するADFX02のECFN（`data/skins/ADFX02/ECFN/select/`）へ、G-BATTLE一覧で選択中の相手の
配置オプション・鍵盤順・CHART REPLICATIONを表示する`G-BATTLE DATA`窓を追加した。
配置は検討時の`.local/notes/2026-11-01-gbattle-layout/refined/ecfn-refined.png`、
見た目はPLAY DATA / IR DATA窓の作法に合わせるというユーザー指定による。
mz-select / Luxez-Flatと同じAPI（20200..20229、text 86）を使い、本体の変更はない。

現在の仕様は[skin.md](../../docs/skin.md#g-battle一覧の相手情報)、対応範囲は
[skin-compatibility.md](../../docs/skin-compatibility.md)、ECFN側の説明は
`ECFN/select/BMZ_EXTENSIONS.md`を参照する。ADFX02側の対象commitは`bmz`ブランチの`9c5ba51`（この時点で未push）。

## 配置と既存窓の作法

- 範囲はグラフ下の空き（x=441、top=696、715×108px）。右端1156はTYPE-AのIR DATA窓に揃え、
  既存の窓は隠さない。描画順は`score_window`の直前で、オプションパネル等の下になる。
- 枠は`system_.png`のSCORE DATA窓（0,613,584×183）を組み直す。見出し帯27px、
  内枠上端の陰影6px、下端13pxと左右の縁（見出し帯12px、本文14px）は等倍で描き、
  中央は文字のない列x=300を伸ばす。縦方向は滑らかなグラデーション（137px）だけを62pxへ縮める。
- 窓の焼き込み文字はECFN_readme.txtのとおり源暎エムゴで、`SCORE DATA`の見出しは
  GenEiMGothic2-Heavy 13px＋黒縁とほぼ一致した。見出しは13px（字高9px、左端13px）、
  本文はPGREAT等と同じ字高12pxになる17px、左端16pxで描く。
- 列の区切りはPLAY DATA内の線と同じ1px・RGB 124・不透明度225。不透明な白の数字画素を着色する。
- 未取得等の表示は、数字画像の11番目にある先頭の0（同じ白を不透明度128にした画像）にならい、
  同じ文字を50%の不透明度で描く。ECFNのNO PLAYは灰色ではなく金属調の画像のため使わない。

## 完成イメージからの変更

- RIVAL OPTION / LANE ORDER / MIRRORの項目名、相手名、HARDは表示しない。
  mz-select / Luxez-Flatで決めた方針と同じで、ゲージはAPIにもない。
  完成イメージのMIRROR OFFは検討用の架空の表示であり、実データがない。
- 鍵盤は完成イメージの拡大（43×40px）をやめ、StarseekerのResultと同じ等倍31×29px・28px間隔にした。
  9鍵が収まる幅で区切り線を置き、3列目に余白がある。
- 10K/14Kは1P/2Pの行を上下に分け、右の列の上段にDP OPTION、下段にCHART REPLICATIONを置く。

## 素材

- 鍵盤は`../../Starseeker/result/Theme/starseeker/jud_detail.png`の1〜7（x=1+(n-1)*31、y=926）。
  ADFX02リポジトリ（GPL-3.0）にはECFN（`25df962`）とStarseekerのResult画像（`1beaf86`）を
  どちらもECFNの作者ric_aoiが追加している。同じパッケージ内の参照であり、画像のコピーや加工はしない。
  ECFN_readme.txtの「画像ソースを流用して他プラットフォーム作品に使用する」禁止には当たらないと判断した。
- 8/9/Sは、元の鍵盤番号が奇数なら1、偶数なら2の鍵盤から、数字のない左右5pxと中央1列を組み合わせる。
  8/9はECFNの数字画像（Starseekerの数字と同系統）、Sは源暎エムゴ18px・縁1pxで重ね、
  Starseekerの数字に合わせてRGB 218で描く。皿は5K/7K/DPでn+1番の鍵盤として扱う（Luxez-Flatと同じ）。

## 実装上の注意

- BMZのLua runtime callbackは`value` / `text` / `graph` / `slider`の値に対応し、`imageset.value`の
  関数は推論できる形だけを扱う。任意の関数では値が付かず先頭画像になるため、`imageset.value`には
  鍵盤番号の数値ref（20210..）を渡し、元の鍵盤番号0..9に対応する画像の列を並べる。
  表示条件は`draw`関数で判定する。
- `bmz_battle_info.lua`は基本拡張・詳細パネルと同じく、元の表を変更せずに複製して構築し、
  `score_window`が見つからない場合や失敗時は`nil`を返す。`select.lua`は`pcall`で読み込む。
- 検討時のプレビュー（Codexの`gbattle_design.rs`）は`fs::canonicalize`のパスと、
  library rootを渡さない`decode_beatoraja_skin_with_options_and_runtime_state`を使っていた。
  そのため`../_font`のTTFとECFNのbitmap fontが読み込めず、完成イメージの一部の文字は細い代替字体だった。
  今回のプレビューは`decode_beatoraja_skin_request`へ`data/skins`をlibrary rootとして渡し、
  実アプリと同じく`../_font`と`../../Starseeker`を解決する。

## 検証

- 明示実行テスト`ecfn_battle_info.rs`を追加した（ADFX02は別途導入のため`#[ignore]`）。
  詳細パネルOFF/ONの両方で、閉じた画面への影響なし、SPの3列、DP FLIPの上下2行と側内番号、
  9Kの8/9、RANDOM-EXのS、未取得・読み込み中・可変配置・DP未取得の50%表示、
  枠の各画素を1回だけ描くことと見出し帯・縁が等倍であることを確認する。
  `score_window`直前への挿入と、モジュールの欠落・失敗・BMZ以外で表示しないこと、
  基本拡張が独立して残ることも確認する。
- `cargo test -p bmz-player --locked ecfn -- --ignored`: 新規2件を含む14件成功、1件失敗。
  失敗した`ecfn_extensions_mode_and_force_preserve_button_hits`（上部のALLが描かれない）は、
  今回のモジュールを外しても再現する既存の問題で、今回の変更とは別。
  同日、キーフィルター表示名の変更に追従していなかったテストと判明し、修正後は15件成功した
  （[ECFN基本拡張の記録](2026-10-05-ecfn-bmz-extensions.md)）。
- Windows offscreen GPUでSP、DP（14K FLIP / 10K）、9K、RANDOM-EX、未取得、読み込み中、可変配置、
  DP未取得、一覧を閉じた画面を描画し、既存の窓と並べて目視確認した。画像はGit管理外の
  `.local/notes/2026-11-01-gbattle-layout/ecfn-implemented/`、ツールは`.local/lr2-play-audit/`に置く。
- `cargo fmt --check`、`cargo check -p bmz-player --locked`、
  `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`: 成功。
- `cargo test -p bmz-player --locked`: 2,782件成功、50件ignore（新規の明示実行テスト2件を含む）。
  別のstartup smoke 2件はignore。
- 実IRでの操作、通常起動での目視、macOS/Linuxは未確認。

## 9Kの鍵盤8/9（2026-11-02追記）

9Kで8/9に重ねたECFNの数字（18px）が、Starseekerの鍵盤の数字より1px大きく高かった。
縁も紺色（RGB 10,20,31）の細い線で、鍵盤の数字の黒い約2pxの縁と違って見えた。

- 数字画像の下に黒く着色した同じ画像を8方向へ1pxずらして重ね、黒い縁を付ける。
- PLAY DATAと同じく縮めて補間し（13px、鍵盤の左上から9px右・8px下）、縁込みの上端・下端を
  鍵盤の数字（暗い画素で7〜21行目）に揃えた。15px・14pxでは縁込みで1px大きくなった。
- 8/9が出るのはSPの8K/9Kだけ（7KやDPの8番は皿のS）なので、数字の描画先はそのモードだけに作る。
- 明示実行テストを更新し、Windows offscreen GPUで9Kを描画して確認。

### 縁の太さと二重の下端（2026-11-02追記）

黒く着色した複製を1px外側に重ねた版は、縁が太くなりすぎ、数値の下端が二重に見えた。
ECFNの数字画像はもともと約2pxの紺色（RGB 10,20,31）の縁を持ち、下端にはさらに1行ずらした影の行がある。
外側に黒を足すと縁が約3pxになり、下では元の縁・影の行・1px下の黒い複製が重なっていた。

- 黒い複製をやめ、数字画像を1回だけ17pxで描く。縁込みの上端・下端は鍵盤の数字（暗い画素で7〜21行目）と一致する。
- 灰色がかって見えた主な原因は、18pxから17pxへの補間で細い縁がにじむことだったため、補間しない。
  縁はRGB 9,17,26の約2pxで、鍵盤の数字（RGB 6,6,6、約2〜3px）とほぼ同じ見え方になる。
- 縁だけを黒くする方法として乗算（`blend=4`）も検討した。数字画像の透明な画素に紺や暗い灰色のRGBが
  残っているため、まわりの鍵盤が暗く汚れるので採用しなかった。
- 13px＋黒縁、15px・16px・17px（補間あり/なし）・18pxを描画して比べ、明示実行テストを更新した。
