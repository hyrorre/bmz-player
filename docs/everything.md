# Everything 連携

Windows では Everything 1.5 以降のインデックスを使って、曲スキャンのファイル探索を高速化できます。既定値は OFF です。

## 有効化

アプリの設定画面で「スキャン設定」→「曲探索に Everything を使用（Windows）」を有効にします。`data/config.toml` では次の設定に対応します。

```toml
[scan]
use_everything = true
```

Everything のデータベースが未ロード、IPC が利用不可、ファイルサイズまたは更新日時が未インデックス、検索が失敗・タイムアウトした場合は、そのルートだけ通常のファイルシステム探索へ自動的にフォールバックします。「シンボリックリンクを辿る」が OFF の場合も、通常探索との意味を変えないためフォールバックします。

Everything 経由でも通常探索と同じく、BMS系拡張子、更新日時、ファイルサイズ、同一フォルダの `.txt` 有無、再帰設定、ドットで始まるファイル / フォルダの除外を扱います。

## ON/OFF 計測

設定を保存せず、一回の CLI スキャンだけ探索方法を上書きできます。

```powershell
cargo run -p bmz-player -- songs load --everything
cargo run -p bmz-player -- songs load --no-everything
```

出力の `Timing` で全体時間と探索時間、`Discovery backends` で実際に使われたルート数とフォールバック数を確認します。

## 計測記録

条件と実測値は [2026-08-17の作業記録](../notes/2026/2026-08-17-everything-performance.md) を参照してください。
