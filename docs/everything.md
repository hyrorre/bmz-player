# Everything 連携

WindowsではEverything 1.5以降のインデックスを使って、曲スキャンのファイル探索を高速化できます。既定値はOFFです。

## 有効化

アプリの設定画面で「スキャン設定」→「曲探索に Everything を使用（Windows）」を有効にします。`data/config.toml`では次の設定に対応します。

```toml
[scan]
use_everything = true
```

検索に応答するのは、同じWindowsセッションで起動しているEverything本体です。Everythingをサービスとしてインストールしていても、本体が起動していなければ利用できません。

Everythingのデータベースが未ロード、IPCが利用不可、ファイルサイズまたは更新日時が未インデックス、検索が失敗・タイムアウトした場合は、そのルートだけ通常のファイルシステム探索へ自動的にフォールバックします。「シンボリックリンクを辿る」がOFFの場合も、通常探索との意味を変えないためフォールバックします。
フォールバックしたときは、スキャンの完了時に対象のフォルダ数と最初の理由を画面に通知し、CLIでは`Everything unavailable:`の行に表示します。
Everythingが1件も返さなかったルート（空のルートやインデックス外の場所）は通常探索で読みますが、通知しません。

Everything 1.5 alphaのようにインスタンス名（`1.5a`など）付きで起動したEverythingには接続できません。
利用しているIPCクレート（everything-ipc 0.1.4）がインスタンス名付きのウィンドウクラス名を正しく組み立てないためです。

Everything経由でも通常探索と同じく、BMS系拡張子、更新日時、ファイルサイズ、同一フォルダの`.txt`有無、再帰設定、ドットで始まるファイル / フォルダの除外を扱います。

## ON/OFF 計測

設定を保存せず、一回のCLIスキャンだけ探索方法を上書きできます。

```powershell
cargo run -p bmz-player -- songs load --everything
cargo run -p bmz-player -- songs load --no-everything
```

出力の`Timing`で全体時間と探索時間、`Discovery backends`で実際に使われたルート数とフォールバック数を確認します。

## 計測記録

条件と実測値は[2026-08-17の作業記録](../notes/2026/2026-08-17-everything-performance.md)を参照してください。
