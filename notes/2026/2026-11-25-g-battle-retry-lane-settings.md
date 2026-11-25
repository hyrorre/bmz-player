# G-BATTLEのクイックリトライ時のレーン設定（#166）

`ccdf67be`では、通常開始時に設定対象のキーモードを渡していました。リトライ専用の読み込みでは指定が抜けていたため、7鍵のG-BATTLEで相手側を追加した14鍵の内部譜面に対応する設定が選ばれていました。[Issue #166](https://github.com/hyrorre/bmz-player/issues/166)

通常開始とリトライで、読み込みを開始する前に同じ方法で設定対象のキーモードとHS-FIXを指定するようにしました。同じ配置でキャッシュ済みの譜面を使う場合と、別配置で譜面を再読み込みする場合の両方で適用します。[プレイ設定](../../docs/settings.md#選曲)

macOS arm64で、7鍵用と14鍵用で異なるレーンカバー・リフト・緑数字・HS-FIXを設定し、両方の読み込み経路で7鍵用の値がセッションに反映されることを回帰テストで確認しました。`cargo fmt --check`、`cargo check -p bmz-player --locked`、`cargo clippy -p bmz-player --all-targets --locked -- -D warnings`、`cargo test -p bmz-player --locked`が通過しました。テストは3,498件成功、69件はignore指定です。Windowsでの手動プレイは未確認です。
