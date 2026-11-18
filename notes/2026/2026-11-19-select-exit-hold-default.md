# 選曲画面のESC長押し終了の既定値

選曲ルートからESC長押しで終了するまでの既定値を1200msから1000msへ変更した。
新規プロファイルと、`select.exit_hold_ms`が未指定のプロファイルに適用する。保存済みの値は読み込み時に引き継ぐ。

[操作](../../docs/controls.md)、[設定](../../docs/settings.md)、[選曲スキンの終了進捗](../../docs/skin.md#bmz-exit-hold-refs)を更新した。

Windowsで`cargo fmt --check`、`cargo check -p bmz-player --locked`、`cargo clippy -p bmz-player --all-targets --locked -- -D warnings`が通過した。
`cargo test -p bmz-player --locked`は通常のWindows権限で実行し、3328件通過、96件ignoreだった。実ウィンドウでの操作確認は実施していない。
