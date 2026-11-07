# EXTRA NOTEの配置改善（#61）

開始時点: `c8209e227b0233f84532638fe7c8a976a22e1341`。

## 方針と実装

LR2の音別レーン傾向とLR2oraja ED-rianの前後検査を参考に、BMZの配置仕様を実装した。
地雷・不可視ノーツ・LNを保持し、最終配置へ音付きBGMを移す。
DEPTHは同時刻の追加上限とし、BPM連動の前後間隔、保持を含む同時押し、
元ノーツ密度に応じた毎秒追加数の制限を併用する。DPの制限は左右独立。
生成は配置乱数やインストール先に依存せず、相手の対戦譜面へ追加しない。

保存方針はユーザー指定により、実際に追加されたときだけAssistとする。
追加ゼロは通常保存・IR候補、実効時はclear-only保存とする。
履歴の再送でも追加ゼロをOFFと同じ扱いにするため、保存用assist maskのEXTRA NOTE bitは実効値を使う。
既存データの変更やDB migrationは不要。

現在の契約は[EXTRA NOTEの配置仕様](../../docs/extra-note.md)に記載。

## 検証

Windowsで次の検証が成功した。

- `cargo fmt --check`
- `cargo check -p bmz-player --locked`
- `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`
- `cargo test -p bmz-player --locked --no-fail-fast`: libは2953成功・0失敗・56 ignored。
  startup integrationは0成功・0失敗・2 ignored、bin / doc-testsは対象0件。

追加した17件は合成譜面・in-memory DBで実行し、素材不足による早期returnはない。
音イベントの保存、LN・地雷・不可視ノーツの保持、全対応鍵盤数、DEPTH、
スクラッチ、BPM境界、同時押し・密度、DP、配置済みノーツの保持、
G-BATTLEの相手表示・採点、実効有無によるランプ・数値・IR・履歴maskを検証した。

初回の全体テストはsandboxがlocalhost接続を拒否（os error 10013）し、
模擬HTTP / OBSテストが失敗・待機したため終了した。同じIRテストを制限外で確認した後、
全体を制限外で再実行し、上記の成功を確認した。
生ログはGit管理外の`.local/extra-note-test-unsandboxed.log`、
`.local/extra-note-check.log`、`.local/extra-note-clippy.log`に保存。

実機プレイとmacOS / Linuxは未確認。
