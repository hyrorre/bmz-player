# Starseeker ResultのMAX符号とLuaロード値

## 原因と修正

Starseeker Resultのscoreframe.luaはロード時の`main_state.number(154) == 0`で
数字画像を選択する。通常はy=292（先頭行の符号画像はマイナス）、満点ではy=165
（先頭行の符号画像はプラス）を使用する。
BMZはResultのロード用number一覧に154を供給しておらず、stubの0で満点用画像が
選択された。描画時には正しい不足点が入るため、1552ノート・EX SCORE 2969の
ケースで、本来のMAX -0135ではなくMAX +0135となった。

ResultSummaryからロード状態を生成する共通処理に154を追加した。
通常ResultとCourse Resultの両方に適用され、rendererの既存の計算を使うため、
ロード時と描画時の境界判定は一致する。154を非負で返す契約、数字画像の行順、
第三者スキンは変更しない。仕様は[skin.md](../../docs/skin.md)を参照。

ADFX02のローカルdevelop（b2b2693）とbmz（b1b0f1f）のStarseeker Resultには
画像を含め差分がなく、ブランチ切替では解決しない。現在のprofileのSelectはECFN。

## 検証

- ロード用154が不足点135、満点0、AAA到達直前1、到達時344となる回帰テストを追加。
- 各ケースでrendererの値と一致し、Luaロード状態へ引き継がれることを確認する。
- 現在のADFX02/Starseeker素材を実際にdecodeし、不足点135でy=292、満点でy=165を
  選ぶ追加テストが成功。素材が無い環境では明示的にskipする。
- fmt、bmz-player check / all-targets Clippyが成功。全テスト2064件成功・6件ignore。
  追加した実素材decodeテストも単独実行で成功し、追加後のClippyも成功。
  全テストの初回はsandboxのローカルbind拒否で17件失敗したが、制限外の再実行で解消。
- 実機でのResult表示確認は未実施。

## 2026-10-05追補: 現行Starseekerに合わせたテスト修正

本体の基点は`feat/select-detail-options` / `364c4d75795951712074e162e1b59b59f6e3648c`。
導入済みADFX02は`c74f7f9ea737d71f2586f066446b5a3e8bc410be`。開始時は両repoとも差分なし。
ECFN作業中に継続していたStarseekerテスト1件の失敗を調査・修正した。

当初の確認対象と異なり、現行Starseekerは`3b8417a`と`b9a120d`で
`rank_diff.lua`を共有するNEXT／NEAREST切り替えへ変更されている。
スコア枠は`../../rank_diff.lua`を読み込み、ref 71/74から差分を求めるruntime callbackを持つ。
旧テストには2つの問題があった。

1. `library_roots: &[]`になる互換decode helperを使い、スキンのentryディレクトリだけを
   許可していた。共通Luaがroot外として拒否され、スキン内の`pcall`が例外を捕捉した結果、
   `SCORE_FRAME`と`RANK_Diff_Exscore`が欠落した。通常アプリは設定済みスキンrootを渡している。
2. 現行の数値定義は`value_expr`でcallbackを参照するため、`ref_id == 154`という期待値も古かった。
   読込rootだけを直してもこちらで失敗する。

調査ではrootあり／なしで同じスキンを比較した。rootありではEX SCORE 2969で135・画像行292、
満点3104で0・画像行165となり、NEARESTのAAA到達時0とAAA+40の符号付き差分-40も確認した。
調査用コード・ログはGit管理外の`.local/starseeker*`に保存した。

旧ref 154の保証は、[app側テスト](../../crates/bmz-player/src/app/tests/result.rs)に
外部アセット不要の最小Lua fixtureとして残す。実際のResultSummaryから渡した値で、
MAX・未到達・AAA境界前後の画像行選択を確認する。

実スキンの検証は[skin loader側](../../crates/bmz-player/src/skin_loader/tests/starseeker_result.rs)へ移し、
通常アプリの共通document loaderと`AppPaths::skin_library_roots()`を使う。
Auto／Compat、NEXT／NEARESTについて、ロード時の数字画像と、同一VMでスコアを更新したときの
callback出力を確認する。単に古いassertionを削除するのではなく、定数化による退行も検出する。
この検証では数値定義とcallbackが対象のため、画像そのものの繰り返しdecodeを省く。
外部スキン未導入時の明示的なskipは維持するが、今回の検証では実アセットを読み込む。

本体のruntime・sandbox境界・スキンのLuaや画像は変更しない。

最終状態で`cargo fmt --check`、`cargo check -p bmz-player --locked`、
`cargo clippy -p bmz-player --all-targets --locked -- -D warnings`、`git diff --check`が成功。
旧ref 154関連2件と、現行Starseekerの実アセットを使う1件が単独実行で成功した。
実スキンは`starseeker`テーマのAuto／Compat × NEXT／NEAREST × 4種類のロード値、
各ロード後の4種類のスコア更新を検証した。素材欠落によるskipではない。

`cargo test -p bmz-player --locked`は**2,151成功・0失敗・19 ignored**。
ローカル通信を使う既存テストの待受制限を避け、全体テストはsandbox外で実行した。
ログは`.local/starseeker-fix-*.log`。実ウィンドウ・GPU描画・Windows/Linuxの確認は未実施で、
今回の変更はテストと記録のみ。

同日のmain取り込み時、旧ref 154の画像シート選択テストはmain側にも追加されていたため、
`result_lua_next_rank_matches_rendering_before_skin_load`へ統合した。
一時ファイルにはmain側の`ProfileTestDir`とappのdecode経路を使い、同じ4種類のスコアを維持する。
現行Starseekerの実アセット・共有Lua・runtime callbackの回帰テストは別途保持する。
