# IR Webの譜面詳細から外部サービスへのリンク

- 譜面詳細のハッシュの下に、rianIRと参照例の外部7サービスへのリンクを追加した。各リンクは譜面のSHA-256またはMD5を使い、別タブで開く。MD5を使う3件は有効なMD5を持つ譜面に表示する。
- Chromeとテスト用D1で、MD5の有無によるリンクの表示、リンク先URL、新しいタブの属性、幅390pxでの折り返しを確認した。
- 公開サンプルでrianIR、Mocha、Bokutachi IR、Stellaverse IR、BMS Score ViewerのHTTP応答を確認した。Kaleid IRはサンプルで404となり、同サービスに登録済みの譜面のSHA-256では詳細への転送を確認した。BMS-IRとMinIRはアクセス確認時に403を返した。
- Prettier、型検査、翻訳テスト7件、IRテスト253件が通過した。

関連仕様は[IRのWeb表示](../../docs/ir.md#webの表示)を参照。
