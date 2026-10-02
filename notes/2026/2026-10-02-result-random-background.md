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
