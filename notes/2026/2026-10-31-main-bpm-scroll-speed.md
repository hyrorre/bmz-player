# MAIN BPMのSCROLL / SPEED集計とHSへの適用

実施日: 2026-10-31。対象ブランチ: `codex/main-bpm-scroll-speed`。
集計条件の導入commit: `01804ee6`。現在の仕様は[HS-FIX](../../docs/hs.md#hs-fix)を参照。

## 集計条件

- TapとLN始点だけを数え、LN始点の重複をNoteIdで除く。
- ノーツ位置のSCROLLが正の場合だけ集計する。負値の絶対値は取らず、0も除外する。
- BPM、SCROLL、実効SPEEDと積が有限かつ正の場合に、`BPM × SCROLL × SPEED`の出現回数を数える。
- CONSTANT有効時の実効SPEEDは1.0。PracticeではCONSTANTを無効にする。
- 極端な譜面ではSTART BPM等を選択する運用とし、MAIN候補の上下限は設けない。

## レビューで確認した問題

BPM 120、目標緑数字300、カバーなしの譜面で、セッション生成と描画snapshotの表示時間を確認した。

| 条件 | 修正前 | 修正後 |
|---|---|---|
| 全域SCROLL 2、SPEED 1 | MAIN 240、HS 1、緑600 | MAIN 240、HS 2、緑300 |
| 全域SPEED 2、CONSTANT有効 | MAIN 240、HS 2、緑600 | MAIN 120、HS 4、緑300 |

前者はMAINに含めた倍率を初期HSの逆算で再度掛けていた。
MAIN基準でカバー変更時にHSを求め直す経路も同じ影響を受けていた。
後者は集計時にCONSTANTの実効SPEEDを使っていなかった。

## 修正

- 初期HSと曲開始前の再計算、MAINを固定基準にしたカバー変更では、MAINに含めたSCROLL / SPEEDを追加で掛けない。
- 曲開始後に現在BPMから求め直す経路では、現在位置のSCROLL / 実効SPEEDを使う。
- セッション生成時は、Practiceを含めた実際のCONSTANT状態を決めてからMAINを求める。
- 設定UIでCONSTANTを切り替えた場合もMAINを再集計する。LR2では表示HSを保ちながら新しい基準値で実効HSを求め直す。

## 検証対象

`crates/bmz-player/src/app/tests/play/cases_06.rs`に次の回帰ケースを追加した。

- SCROLL 2、SPEED 2、両方2について、Normal / FloatingとCONSTANT有効・無効で初期HSと緑数字を確認。
- SCROLL 2のノーツ区間からSCROLL 3区間へ移った後、HS自動調整の有効・無効で固定基準と現在BPM基準を確認。
- 曲開始前・開始後のCONSTANT切替でMAINとHSを更新し、LR2の表示HSを維持することを確認。
- PracticeでCONSTANTを要求してもSPEEDを含むMAINが使われることを確認。

Windows上の自動検証を対象とする。GUIでの手動プレイ、物理入力、macOS / Linuxでの実行は未実施。

## 検証結果

- `cargo fmt --check`: 成功。
- `cargo check -p bmz-player --locked`: 成功。
- `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`: 成功。
- `cargo test -p bmz-player --locked -- --quiet`: 2761 passed / 0 failed / 47 ignored。別途startup_stackの2件は既定どおりignored。

最初のテスト実行は、実行環境の制限でローカル模擬サーバーへの接続がOSエラー10013、
Windowsリンク操作がAccess deniedとなったため中断した。
ローカル通信とリンク操作を許可した環境で全体を再実行し、上記の成功結果を得た。
ビルドにはインクリメンタルキャッシュのハードリンク失敗時にコピーへ切り替える環境由来の警告が出ていた。
