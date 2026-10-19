# 詳細オプションのスクロール補間を選曲バーへ統一

## 原因

詳細オプションは選曲バーとは別の線形補間を使い、追加入力では速度を引き継がず、
残り変位を±1列に切り詰めていた。アナログの複数項目移動でもカーソルだけが進み、
表示が入力直前の位置から飛んでいた。

## 修正

- 選曲の`SelectBarScroll`を再利用し、位置・速度が連続する三次Hermite補間と到着時の減速を共用。
- ホイールは選曲の低速設定、アナログは選曲と同じ移動数に応じた120ms / 30msを使用。
- 整数列分は表示窓、小数列分は描画変位へ反映し、複数項目移動や循環でも全移動量を維持。
- 表示窓と変位は同じ時刻で評価。項目配列を共有したままviewportを更新し、退出時は両方を保持。
- クリック判定に使ったsnapshotで行を解決し、列境界をまたいでも見えていた項目へ適用。

スキンの素材やLuaは変更していない。仕様は[詳細オプション](../../docs/select-detail-options.md)、
[操作](../../docs/controls.md)、[Skin API](../../docs/skin.md#bmz-select-detail-options-v1)に反映。

## 検証

- `cargo fmt --check`: 成功。
- `cargo check --workspace --locked`: 成功。
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: 成功。
- `cargo test --workspace --locked --no-fail-fast`: 全対象成功。bmz-playerは2582 passed / 38 ignored、bmz-renderは772 passed / 7 ignored。
  Luxezの全項目・候補と移動中のhit判定を含む既存の実スキンテストも成功。
- Windowsで`detail_scroll_input_routes_snapshot_cache_and_click_use_the_displayed_window`を`--ignored --exact`で単独実行: 1 passed。
  実際のホイール経路・アナログ移動経路の時間、viewport更新時の項目配列共有、旧snapshotの保持、行・候補・数値イベントの対象を確認。
- 補間の回帰テストで連続入力・逆転・複数列移動・循環と、選曲バーとの同時刻での位置一致を確認。

incremental cacheのhard linkからcopyへ切り替える環境warningは出たが、check / Clippy / testは終了コード0。
生ログはGit管理外の`.local/performance/2026-10-20-detail-option-scroll/`に保存。
実機のホイール・アナログスクラッチの操作感、GPU上の連続フレームの目視確認は未実施。
