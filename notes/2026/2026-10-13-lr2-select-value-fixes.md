# LR2選曲スキンの値のずれと条件の修正

## 背景

手元のLR2選曲スキン8本（LR2標準、Seraphic、ECBE、RED BELT、EndlessCirculation SE、PLATINUM、3R、W-mix HD）で
使われている要素を走査し、BMZの対応状況と照合した。
[LR2専用のnumber / option参照](2026-10-13-lr2-number-option-refs.md) と同じ仕組みで直せるもののうち、
LR2の定義が明確で、必要な値がすでに描画状態にあるものを修正した。

## 変更

- number（Select / Decide）
  - 12: 判定タイミング。BMZはパネル3以外でソートの番号を返していた。
  - 45..49: 各難易度のレベル。BMZは全欄に選択中のレベルを出していた。選択中の譜面の難易度の欄だけ表示する。
  - 70 / 73: 自己ベストのLR2式スコアとレート。
  - 210..219: IRのクリア人数と率をLR2の5分類で返す。beatorajaの11分類の値を出していた。
- number（全画面）: EQ / FX / PITCH（50..56、60..66）は非表示にした。54 / 55にDPオプション・HS-FIXの番号が出ていた。
- number（Result）: 212 / 214 / 216を拡張LR2のFAST / SLOW / コンボブレイク数として扱う（Playの扱いと同じ）。
- text（Select / Decide）: 190..199（ランダムコースの設定）は空文字にした。beatorajaのランダム関連の文字列が出ていた。
- option: Select / Decideの20（パネルなし）を評価する。未処理で常に偽だった。
  `#IF` のoption 50 / 51は読み込み時に固定せず描画時に評価する。選曲の読み込み時は常にオフライン扱いになっていた。
- bargraph 140..145: Selectでは選択中のスコアを譜面のノーツ数で割る。経過ノーツ数が0のため常に0だった。
  beatorajaのJSON / Luaスキンの選曲画面にも同じ修正が効く。

## 対応しないもの

[skin.md](../../docs/skin.md#lr2のnumber--option) の未対応一覧に記載した。
値の供給が必要なもの（レベルバー、同じフォルダ内の他難易度、ゴースト進捗値、ライバルのIR順位等）と、
BMZに同じ機能が無いもの（EQ / FX / PITCH、パネル2..7、スキン設定・コース作成・ヘルプの埋め込み、
README / MOUSECURSOR、BAR_RANK / BAR_RIVAL）がある。

## 検証

- bmz-skinに変換の回帰テストを追加した。既存の「選曲の210はPlayのFAST/SLOWにしない」テストは、
  IRのクリア分類になることを確認する形に更新した。
- bmz-renderに評価の回帰テストを追加した（判定タイミング、難易度欄、EQ、自己ベスト、IRの5分類、パネルなし、判定率グラフ）。
- `cargo fmt --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、
  `cargo test --workspace --locked --no-fail-fast` は成功。
- GPUでの実画面確認とIR接続時の確認は未実施。
