# Bokutachiの文字列曲IDによる送信失敗

2026-11-13（JST）。Windowsの送信キューに残った7Kの4件は、
譜面照合の応答を読み取る段階で `invalid type: string … expected u64` になりました。
インポート要求を送る前の失敗で、再送に使うプレイ結果はキューに残っています。

[Tachiの譜面型](https://github.com/zkldi/Tachi/blob/adabf2c3aa8d53cde0eeab01f33de9824274d70a/typescript/common/src/types/documents.ts#L367-L386)
に合わせて `song.id` を文字列として扱い、旧形式の数値も読み取ります。
BMSはSHA256、PMSは照合した曲IDを使って登録します。
修正版での個別再送は[譜面の照合](../../docs/bokutachi.md#譜面の照合)を参照してください。

Windowsで `cargo fmt --check`、`cargo check -p bmz-player --locked`、
`cargo clippy -p bmz-player --all-targets --locked -- -D warnings`、
`cargo test -p bmz-player --locked` が通りました。テストは3,197件成功、89件はignoredです。
模擬APIで、失敗済みの7Kジョブの個別再送から登録完了までと、
PMSの譜面照合と送信で同じ文字列IDを使うことを確認しました。本番への再送は実施していません。
