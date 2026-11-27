# 選曲・ゲージ更新・IRランキングの計測

比較元は `60fe45a0`。macOS の同一マシンで、各項目の変更前後に同じ入力を使った。

## 選曲バナー

GPUに32枚保持できる設定で、新しいバナーを1枚表示したときの退避枚数は32枚から1枚になった。
`full_gpu_cache_retires_only_one_banner_for_one_upload` でアップロード要求を実行し、
解放待ちのキューを直接数えた。変更後は件数上限の256枚に達した場合も退避は1枚。
解放前は次のアップロードを待つ。実GPUのフレーム時間は計測対象外。

## IRライバルの選択

一時SQLiteに2万件のスコアを置き、通信を無効にして、未選択からライバル1人を選んだ。
debugテストで11回実行した中央値。選曲側の最初の呼び出しは46.545msから0.016ms、
全スコアが利用可能になるまでは46.545msから47.709msだった。
再現コマンドは `cargo test -p bmz-player --lib measure_rival_selection -- --ignored --nocapture`。

## HCNのゲージ更新

同じレーンに時間順に並ぶHCNが1万本あり、先頭を保持している譜面を使用した。
チャート索引の構築を計測から除き、100µs間隔のゲージ更新1,000回を11回反復した。
releaseビルドの中央値は4.490msから0.046msに短縮した。FPSや譜面全体のプレイ時間は測っていない。
再現コマンドは `cargo test -p bmz-gameplay --release --lib measure_hold_gauge_updates -- --ignored --nocapture`。
