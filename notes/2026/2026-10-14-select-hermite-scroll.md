# 選曲バーの速度を引き継ぐHermite補間

## 背景

- [連続移動補間](2026-10-14-select-continuous-scroll.md)で位置の連続性を保ったが、
  線形補間は再入力時に速度が変わり、到着時に急に停止していた。
- LR2の選曲バーは線形補間を使用する。今回のカーブはBMZの表示改善として実装する。
- 基点は`f78f0a26`。専用worktreeで変更し、元checkoutの作業中の変更を保持する。

## 実装と判断

- `SelectBarScroll`に残量の速度（rows/second）を追加する。
  再入力時には同じ時計で位置と速度を評価し、新しい目標へ接続する。
- 残量`r`、その微分`v`、時間`T`、正規化時間`u=t/T`として、
  `r(u)=(1-u)^2 * (r*(1+2u) + v*T*u)`を使用する。
  微分は`(1-u) * (-6*r*u + v*T*(1-3u)) / T`。
  初期位置・速度を保ち、終点の残量と速度を厳密に0にする。加速度の連続性は保証しない。
- 静止からの単発移動はsmoothstep相当となる。行ごとではなく全残量へカーブを適用する。
- 目標へ向かう初期速度が大きい場合は`T <= 3*abs(r)/abs(v)`へ短縮して行き過ぎを防ぐ。
  速度をclampすると再入力時に不連続になるため、時間だけを変える。
- 反転・残量0の場合は短く減速してから戻る。
  Hermiteの初期接線項`u*(1-u)^2`の最大値が`4/27`なので、
  `T <= 0.25*27/(4*abs(v))`として現在位置と目標の区間外への移動量を約1/4行以内に抑える。
  Durationのnanosecond丸めがあるため数学的な厳密上限には丸め誤差を含む。
- 時間0は即時移動、フォルダ変更等のresetは速度も破棄する。
  長押しの入力周期・論理選択・整数行と小数行の分解・描画とクリックの共用経路は維持する。
- 新しい設定やスキンの変更は加えない。描画上の整数pixel丸めや、スキン配置の折れ曲がりは残る。

## 検証

- `cargo fmt --check`、`cargo check -p bmz-player --locked`、
  `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`成功。
- 補間の回帰10件で再入力時の位置・速度、停止、反転時の約1/4行の制限、近い目標への単調移動、
  残量0の制動、reset、時間0、ページ移動・循環、長押し、60/120/144Hz相当の経過時刻を確認。
- `cargo test -p bmz-render --locked select_scroll`の既存回帰3件成功。
  非等間隔配置での描画・ランプ・クリック領域、論理選択と表示行の分離、中央flash固定を確認。
- 元checkoutのWMIX_HDをGit管理外のworktreeへコピーし、実CSVの既存回帰が実行されたことを確認。
  描画・クリック矩形の評価であり、GPU描画の確認ではない。
- 初回のビルドは元checkoutの`target`を`--target-dir`で共有した。
  初回のcrate全体テストはworktreeの未初期化submoduleと、共有ビルド先からのresource解決により17件失敗。
  同梱スキン3つを元checkoutのローカルGitコピーから指定commitで初期化し、
  テストプロセスの`BMZ_RESOURCE_DIR`と`BMZ_DATA_DIR`をworktreeの`data`へ指定した。
  元のDB/configはコピーしない。初回ログは`.local/select-hermite-player-tests.log`。
- 共有ビルド先での再検証は、並行作業の異なる`SelectSnapshot`型を参照してcompileに失敗した。
  worktreeのソースには存在しないfieldの初期化を要求するエラーで、ソース修正では対処しない。
  worktree専用の`target`へ外部依存のキャッシュだけをコピーし、BMZ各crateの成果物を除外して再構築する。
  以降のcheck / Clippy / testは専用targetと上記の環境変数で再検証する。
- 専用targetでfmt / check / all-targets Clippy成功。
  `cargo test -p bmz-player --locked`成功（lib: 2426 passed / 22 ignored、native起動: 2 ignored）。
  WMIXの実CSV回帰も実行された。最終ログは`.local/select-hermite-player-tests-isolated.log`。
- 専用targetで描画回帰3件も成功（`.local/select-hermite-render-tests-isolated.log`）。
  `cargo build -p bmz-player --locked`成功。実行ファイルは`target/debug/bmz-player.exe`。
  ビルドログは`.local/select-hermite-build.log`。
- Windows実画面での操作感、実キー・マウス・コントローラー入力、60/120/144Hzの録画比較、
  macOS/Linux実機は未確認。時刻の自動テストは表示FPSの計測結果ではない。

仕様と操作: [skin.md](../../docs/skin.md)、[controls.md](../../docs/controls.md)。
