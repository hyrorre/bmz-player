# IRの入力境界とレート制限

対象は`fd94c209`。Linux環境でIRクライアントの秘密情報保存とWebの受信処理を確認した。
仕様は[ir.md](../../docs/ir.md)に反映した。

- サーバー由来のprovider keyがファイル名に展開され、`../`で保存先を逸脱できた。
  token・device key共通のストア境界で、読み書き・削除・OSストア移行の前にキーを検証する。
  refresh応答によるprovider key変更も拒否する。
- 単曲スコアはEXと判定数の一致、ノーツ数に対するEX・コンボ・処理済みノーツの上限を検証する。
  途中終了・EMPTY POOR・独立した最小BP/CBを許容し、取り込みスコアにも同じ検証を適用する。
  コースも最大EXに対するEX・コンボ上限を確認する。不正な入力はHTTP 400とする。
- IPレート制限はCloudflare実行コンテキスト下のCF-Connecting-IP、またはNodeのソケットを使う。
  利用者が指定したX-Forwarded-Forで別の制限枠へ移ることを防ぐ。

`bun run test:ir`は183件成功。変更ファイルのPrettierと`bun x vue-tsc --noEmit`も成功。
Rustはfmtと、変更したcredentials/secret_store実ソースをincludeする独立harnessで7件成功。
harnessのconfig・一時ディレクトリ型は最小stubで、OS keyringの実機連携を検証したものではない。
`cargo check -p bmz-player --locked`はlibpipewire-0.3等の開発ライブラリ不足で停止したため、
プレイヤー全体のcheck完了・Clippy・testは未確認。Linux環境にはライブラリのインストール権限がない。
Cloudflareへのデプロイ・本番アクセスは実施していない。
