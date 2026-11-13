# v0.5.2のリリースで失敗したBokutachiのPrimary設定テスト

`c94508e7`の[Linux tarジョブ](https://github.com/hyrorre/bmz-player/actions/runs/43469318963/job/132037187621)では、同梱スキンのテスト3件が通過し、Bokutachiの送信キューのテスト1件が失敗した。
テスト名はPrimaryなしのケースを表すが、共通fixtureがBokutachiをPrimaryに設定していた。譜面別ランキングの追加後は、明示的に選んだBokutachiがPrimaryになる。

Primaryを空にした場合も送信キューを選べることと、BokutachiをPrimaryに選んだ場合は一括送信と一括取得が無効になることを検査する。仕様は[IRのPrimary設定](../../docs/ir.md#primary-providerとランキングの取得)を参照。

Cargo 1.100.0で失敗を再現し、修正後の一括操作・送信キューUIのテスト4件が成功した。

同じ実行の[macOS arm64ジョブ](https://github.com/hyrorre/bmz-player/actions/runs/43469318963/job/132037187736)は、GitHubがホストランナーを確保できず、ステップを開始する前にキャンセルされた。
