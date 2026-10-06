# 演奏終了・ミス表示・見逃しキー音の任意設定

## 対象と根拠

2026-10-06の追加依頼により、[直近4か月の互換修正](2026-10-06-upstream-compatibility-fixes.md)に続いて、
LR2orajaED-rianの以下の動作を独立したprofile設定として取り込む。
既存profileの動作を保持するため、3項目とも初期値はOFFとする。

| 設定 | 参照commit | 動作 |
|---|---|---|
| 定義音の配置終了までResultを待機 | `aa4ce430` / `529a1b77` | 最終ノーツに加え、不可視ノーツ・BGM・通常BGAの最終配置時刻を終了基準に含め、その5秒後まで待つ |
| GOOD以上でミスレイヤーを解除 | `9734a3db` | PGREAT / GREAT / GOODで現在のミスレイヤー表示を解除し、後続BAD / POORで再表示する |
| 見逃しPOORでキー音を再生 | `a9007a89` | 入力せず見逃した通常ノーツ・LN始端および放置したCN / HCN終端のキー音を鳴らす |

上流参照は `.local/lr2oraja-endlessdream/core/src/`。
Result待機はPCMの自然終了を監視する機能ではなく、最終配置時刻と譜面時刻で5秒の余裕時間を基準とする。
Poor専用BGAや音声データ自体の長さは待機基準に追加しない。
BMZの既存終了処理と同じ時計を使うため、rian FreqTrainerの実時間の余裕時間とは区別する。

## 境界

- Practiceの区間終了は延長しない。通常プレイ・Replay・Autoplay・Courseでは設定を適用する。
- 手動終了と途中FAILEDは既存の共通終了処理を維持する。途中FAILEDの音声停止を迂回しない。
- 見逃しキー音は採点イベント全般を対象としない。BAD済みノーツ、早離しPOOR、空POOR、地雷には追加発音しない。
  CN / HCN始端を見逃したことで同時採点される終端も二重発音しない。
- 表示・発音・終了待機の設定であり、判定、スコア保存条件、IRのscore identityは変更しない。
- GOOD解除の通常session経路では入力オフセット補正後の採点時刻と表示時刻を区別する。
- HCN始端の見逃し発音は、その更新内で消音しない。次回更新では通常のHCNミュートに戻し、
  押し直しで復帰する。直前の別HCNの消音状態が残る場合も、新しいvoiceへ次回の消音を適用する。

継続仕様は [操作・設定](../../docs/controls.md)、[演奏runtime](../../docs/gameplay-runtime.md)、
[判定仕様](../../docs/rule.md) を参照。

## 検証

Windowsで以下を確認した。生ログは `.local/validation/2026-10-06-optional-play-behaviors/` に保存し、Git管理しない。

| 確認 | 結果 |
|---|---|
| `cargo fmt --check` | 成功 |
| `cargo check --workspace --locked` | 成功 |
| `cargo clippy -p bmz-gameplay --all-targets --locked -- -D warnings` | 成功 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 既存の `crates/bmz-player/src/input/capture.rs:111` の `clippy::let_unit_value` で失敗。今回の差分には含めない |
| `cargo test --workspace --locked --no-fail-fast` | 3,861 passed / 0 failed / 30 ignored。子プロセスで再実行した1件の内側出力は二重計上しない |
| `git diff --check --ignore-submodules=all` | 成功 |

直接回帰は設定の独立性・保存復元、ノーツ/BGM/BGA別の終了基準、Practice除外、
GOOD解除と正負入力オフセット・判定順序、LN/CN/HCNの見逃し、BAD済みノーツと早離しの除外、
自動キー音との重複抑制、Viewer seekと表示専用レーンの無音を扱う。
通常/Replayの見逃し発音、HCN初回発音→次回消音→再押下復帰、途中FAILEDによる停止、
長いPCMが残っていても設定したイベント基準で正常終了することを、実際のaudio mixer出力で確認した。

これは自動テストでの確認であり、出力デバイスでの聴取、上流アプリと同じ楽曲による目視比較、
macOS/Linuxでの実行は未実施。ignoredの外部素材/GPU等の検証を今回実行済みとは扱わない。
