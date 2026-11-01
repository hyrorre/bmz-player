# スキンの譜面情報供給とLuaフレーム処理

対象: [#57](https://github.com/hyrorre/bmz-player/issues/57)、[#58](https://github.com/hyrorre/bmz-player/issues/58)、[#59](https://github.com/hyrorre/bmz-player/issues/59)。
仕様は[skin.md](../../docs/skin.md)、対応範囲は[skin-compatibility.md](../../docs/skin-compatibility.md)を参照します。

## #59: Playの譜面長

- number 1163 / 1164がResult以外ではSelectの曲長しか見ず、Playでは0になっていた。
- Play / Decideの描画状態へ`RenderSnapshot.duration`から譜面長を渡す。ノーツ表示時間の`total_duration_ms`とは区別する。
- 通常開始・先読み・コース次曲・オフライン出力のPlayロード状態へも渡す。第三者スキンのソースは変更しない。
- ローカルにはSocialSkin（曲長表示・ログ）、LITONE9（Playの合計時間）、Hub / mz-select antique（区間ログ）、ModernChic（曲長に依存する数値・グラフ）がある。
- 回帰テストは分秒境界、開始前から終了後までの時刻変更、別曲での再ロード、負の長さを確認する。
- Windows: `cargo fmt --check`、workspace check / all-targets Clippy成功。workspace testはbmz-player以外が成功。bmz-playerの素材不足23件は同梱submodule初期化後に全libテストを再実行し、2790 passed / 50 ignoredだった。
- 実機での見た目とmacOS / Linuxは未確認。
