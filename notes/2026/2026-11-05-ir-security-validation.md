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

## 2026-11-06: リプレイ2件とLR2 backfill

PR #74の`d2b2d2db`に対する追加修正。

- 同時PUTで検証が読んだblobを上書きでき、DBのverifiedと保存内容が食い違った。
  URL発行時の世代IDとPUTごとの保存先を分け、既存blobを上書きしない。
  URL発行・アップロード確定・検証結果の反映で、読み取った世代と現在のDB値を照合する。
  競合は409となり、再試行ではURLを取得し直す。schema migrationは不要。
- Content-Lengthのない本文を全量buffer化してから4MiB上限を確認していた。
  NitroのCloudflare adapterもhandlerより前に全量を読み込むため、Worker entryで先に制限する。
  Nodeのアップロードhandlerもbackpressureと読み取り中止に対応する。小さなchunkの配列も
  無制限に保持せず、上限付きの単一bufferへコピーする。
- LR2のPOORは空POORを含む。LR2だけPOORをノーツ数合計から除外し、他の検証は維持する。
  既存の取り込みfixtureと同じnotes=128 / PG100 / GR22 / GD3 / BD2 / PR10 / EX222を
  受理し、他の取り込み元・EX不一致・PG〜BDだけで超過する値は引き続き拒否する。

IRテストは195件成功（競合とstream/Worker入口の11件を含む）。
変更ファイルのPrettier、vue-tsc、Cloudflare用Nuxt production buildを確認した。
開発用はNitroのNode entryを維持し、`/login`のSSRがHTTP 200を返すことも確認した。
ローカルのWrangler設定ディレクトリは書き込み可能な一時領域へ指定した。
本番へのデプロイ・アクセスは実施していない。Rustクライアントは今回は変更しておらず、
前回記載のプレイヤー全体のビルド制約は残る。
