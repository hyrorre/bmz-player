# リザルト更新時のLuaランダム背景維持

## 対象と原因

- 対象: `v0.4.3..753f81cb` のレビューで確認した、IR更新時の背景再抽選。
- `skin_config.get_path()` で `Random` を解決し、構築時にIR数値も参照するLua Result skinで発生する。
- 従来はLua実行後の `source.path` をキーに画像を固定していた。Luaが別画像を選ぶと前回のパスと一致せず、固定処理が効かなかった。
- 最小スキンと公開ローダーで、修正前は固定情報を渡した2回目の更新で別画像へ切り替わることを確認した。

## 修正

- `bmz-skin` でランダムな `get_path` の結果を、正規化したパターンと呼び出し順で記録する。
- `bmz-player` は同じResultの更新要求にその選択を渡し、Luaの構築中から同じパスを返す。
- 適用済みResultの選択は、IR依存の分岐が一時的に消えても保持する。新しいパターン・呼び出しの選択は、その更新を適用した時点で追加する。
- 通常読み込み・次のResultでは選択をリセットする。プロファイルに `Random` の具体的な選択先を書き戻さない。
- 明示したファイル選択は固定情報より優先する。再利用するパスにも既存のsandbox検証を適用し、Randomを含むdocumentは引き続きcacheへ保存しない。
- 第三者製スキンの変更はない。現在の仕様は [skin.md](../../docs/skin.md) を参照。

## 検証

Windows上で以下を実施した。

- `cargo fmt --all -- --check`: 成功。
- `cargo check --workspace --locked --offline`: 成功。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`: 成功。
- `cargo test --workspace --locked --offline --no-fail-fast`: `bmz-player --lib` の3件が失敗し、ほかのtargetは成功。`bmz-player --lib` は2100成功・3失敗・6ignore、`bmz-skin` は230成功、`bmz-render` は655成功・3ignore。
- 追加した4件は成功。IR数値を16回変更しても背景とパス依存の構築値が変わらないこと、次のResultで再抽選すること、選択の呼び出し順・追加呼び出し、明示選択の優先、sandbox拒否、IR依存の分岐が消えた後の選択保持を確認した。

全体テストの失敗は以下の通り。今回の修正へ無関係な変更を混ぜないため、そのまま残した。

- `skin_loader::tests::cache::result_refresh_pins_resolved_wildcard_source`: decode後のsourceが空でindexエラー。Windowsのverbatim絶対パスを固定情報へ渡す既存ケース。修正前の2026-10-01ビルド済みtest binaryでも同じ失敗を再現。
- `skin_loader::tests::paths::wildcard_source_with_context_falls_back_to_default_file_stem`: 通常パスと `\\?\` prefix付きパスの比較不一致。上と同じ修正前binaryでも再現。
- `app::tests::result::starseeker_result_selects_next_rank_sheet_from_summary_when_available`: ローカルのStarseeker素材で `RANK_Diff_Exscore` が生成されず失敗。手元の該当定義はテストが期待する `ref = 154` ではなく `get_rank_info()` を呼ぶvalue callback。上の修正前binaryにはこのテストが存在せず、変更前binaryでの再実行による確認はできていない。

外部スキンでのIR通信・動画連続再生の実機確認は未実施。

## 2026-10-02追記: 残った3件のテスト修正

上記の3件を `b23e4867` 時点のコードで詳しく調査し、テストの入力・前提を修正した。

- パス2件は、macOSの `/var` と `/private/var` の相違を解消した `0b968135` の `fs::canonicalize` 追加がWindowsで `\\?\` prefixを付けることが原因だった。
  BMZの `SkinPathContext` はcanonicalize後にこのprefixを外すため、固定情報に渡すパスと、期待値を比較するパス表現が揃っていなかった。
  固定情報は `SkinPathContext::resolve_file` の結果を使い、wildcardの比較は解決結果もcanonicalizeする。macOS向けの正規化は維持した。
- StarseekerはローカルのADFX02 `8cc5bb1` で確認した。ランク差分の共通化後はResultより上の `rank_diff.lua` を読み、`main_state.number(71/74)` を使うcallbackで差分を返す。
  以前のテストは許可rootをResultディレクトリに限定しており、この読み込みが拒否され、`pcall` 内のスコアフレーム構築が中断していた。加えて `ref=154` 固定という期待も現在の定義と一致していなかった。
- Starseekerの実素材テストをskin loader側へ移し、アプリと同じ `AppPaths::skin_library_roots()` を渡す。
  描画時のproviderでcallbackを評価し、NEXT / NEARESTの差分、MAX用の数値画像位置、読み込み後のスコア変化への追従を確認する。旧版の `ref=154` 定義にも対応する。
- アプリ側の `ref=154` 回帰確認は最小Lua fixtureに分離した。外部素材なしでも、実際のResult summaryからロード時に正しい数値画像を選ぶことを検証する。
  既存のgrade diff本体の修正は維持し、本番コードと第三者製スキンへの変更は行っていない。

Windowsで対象3件と `ref=154` のテスト計4件、fmt、`bmz-player` のcheck / all-targets Clippyは成功。
`cargo test --workspace --locked --offline --no-fail-fast` も成功し、全targetで失敗0件。
`bmz-player --lib` は2103成功・6ignore、`bmz-skin` は230成功、`bmz-render` は655成功・3ignoreとなった。
Starseekerは素材ありで実行し、1552 notes・EX SCORE 2800のNEXT=304 / NEAREST=-40、MAXの0を確認した。
macOS / Linuxでの再実行と画面の目視確認は未実施。
