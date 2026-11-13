# Bokutachiの譜面別ランキングとPrimary設定

選曲と単曲リザルトの全体ランキングにBokutachiを追加しました。
既存のSHA256照合を使い、Tachiの譜面別PB一覧と本人のPBを参照します。
順位と参加人数はTachiの値を使います。送信条件やノーツ数照合はスコア送信時に適用します。

既存プロファイルのPrimaryは保持し、IR設定で明示的にBokutachiを選ぶと切り替わります。
選曲とリザルトでPMSの入力機器が異なる場合は、それぞれの部門を取得します。
結果画面では登録完了を確認してから順位を更新します。

詳細は[Bokutachiの譜面別ランキング](../../docs/bokutachi.md#譜面別ランキング)、
APIは[Tachiの譜面別PB一覧](https://github.com/zkldi/Tachi/blob/adabf2c3aa8d53cde0eeab01f33de9824274d70a/typescript/server/src/server/router/api/v1/games/router.ts#L466-L486)を参照してください。

11月14日（JST）にWindowsで `cargo fmt --check`、`cargo check -p bmz-player --locked`、
`cargo clippy -p bmz-player --all-targets --locked -- -D warnings` を確認しました。
テストの実行と本番API・画面での動作確認は未実施です。
