# F3で譜面ファイルを選択表示

## 変更

選曲画面のF3および同じ処理を呼ぶスキンイベント212で、DBから利用可能な譜面ファイルを解決し、そのファイルを選択してフォルダを開く。
同一譜面のコピーがある場合は既存のsource解決を利用する。譜面ファイル不在時と通常のフォルダ行は従来のフォルダ表示に戻す。
ハッシュコピーの修飾キー操作は維持する。操作仕様は [controls.md](../../docs/controls.md) を参照。

- Windows: `explorer /select,`。スラッシュと拡張パス形式をshell向けに変換する。
- macOS: `open -R`。
- Linux: 別スレッドから `dbus-send` で `org.freedesktop.FileManager1.ShowItems` を呼ぶ。応答待ちは2秒、ツール不在や失敗時は `xdg-open` で親フォルダを開く。file URIのカンマは配列引数の区切りにならないようエスケープする。

## 検証

Windows環境で `cargo check -p bmz-player --locked` と `cargo clippy -p bmz-player --all-targets --locked -- -D warnings` が成功。
元ファイル、別コピーへの切り替え、全コピー不在を再現する回帰テストが成功。Windowsパス変換のテストには日本語、空白、カンマ、拡張UNCパスを含む。

`cargo fmt --check`、`git diff --check` と `cargo test -p bmz-player --locked` が成功（2,077 passed、6 ignored、0 failed）。ログはGit管理外の `.local/f3-reveal-test.log`。
各OSでファイルマネージャーの選択状態を目視する実機確認は未実施。macOS/LinuxのビルドもこのWindows環境では未実施。
