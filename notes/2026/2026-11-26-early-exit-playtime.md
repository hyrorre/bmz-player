# 途中退出時の累計プレイ時間

120秒の合成譜面を10.75秒で手動中断し、保存時にプロファイルの累計プレイ時間が120秒増えることを確認した。[Issue #169](https://github.com/hyrorre/bmz-player/issues/169)

結果のスナップショットに累計への加算秒数を持たせる。手動中断・途中FAILED・採点確定前の早期退出では、gameplayが確定した停止時刻を使う。完走は譜面の長さを使う。[スコア保存の仕様](../../docs/score-persistence.md#単曲の保存判定)

macOS arm64の回帰テストで、保存時までに時計が13秒へ進む条件でも、通常プレイ、実効アシスト、コース各曲の手動中断がいずれも10秒を加算することを確認した。採点確定後の非同期保存では完走として120秒を加算した。Windows実機での操作確認は未実施。

`cargo fmt --check`、`cargo check -p bmz-player --locked`、`cargo clippy -p bmz-player --all-targets --locked -- -D warnings`が通過した。全テストは模擬サーバーの起動を許可し、`cargo test -p bmz-player --locked -- --test-threads=6`で3,498件成功、69件無視だった。
