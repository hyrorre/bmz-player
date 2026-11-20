# リリース後バウンス除去の上限を100 msに拡張（#142）

[Issue #142](https://github.com/hyrorre/bmz-player/issues/142) では、ゴムスイッチのコントローラーで、上限の20 msでも多重入力が起きると報告されました。
同じコントローラーをbeatorajaで使うと、最小入力間隔50 msで多重入力を抑えられたとのことです。

beatoraja 0.8.9では、同じボタンの押下と解放の両方に最小入力間隔が適用され、設定上限は100 msです。
[設定上限](https://github.com/exch-bms2/beatoraja/blob/0.8.9/src/bms/player/beatoraja/PlayModeConfig.java#L16-L19)と[コントローラーの入力処理](https://github.com/exch-bms2/beatoraja/blob/0.8.9/src/bms/player/beatoraja/input/BMControllerInputProcessor.java#L135-L173)を確認しました。
BMZ Playerでは、解放直後の再押下を除去します。
キーボードとコントローラーの共通の設定上限を100 msに引き上げました。
現在の設定範囲は[設定画面の仕様](../../docs/settings.md#リリース後バウンス除去)を参照してください。

## 検証

- 設定メニューの値を扱うテストで、両方の設定が99 msから100 msに上がり、さらに増やしても100 msに留まることを確認しました。プレイ開始時に50 msと100 msがデバイス別の閾値になることもテストしました。
- Windowsで `cargo fmt --check`、`cargo check -p bmz-player --locked`、`cargo clippy -p bmz-player --all-targets --locked -- -D warnings` が通過しました。
- `cargo test -p bmz-player --locked -- -q` は、ローカル通信とフォルダーリンクを使える状態で再実行し、3,367件通過、失敗0件、既定で対象外のテストは96件でした。
- 報告にあったゴムスイッチのコントローラーでの実機確認は未実施です。
