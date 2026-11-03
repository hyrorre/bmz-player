# HTTP取得した書庫の直接登録（#63）

## 依頼と方針

- HTTPで取得した書庫を展開せず、`data/songs/http/<取得名>.<拡張子>`へ直接保存する設定を追加します。
- 取得ごとの保存用フォルダや、選曲表示のための追加メタデータは作りません。
- 選曲階層は通常の書庫表示を利用し、展開した場合との完全一致は要件にしません。
- 直接再生とHTTP取得の対応形式をZIP / RAR / 7z / tar.gz / LZHへ揃えます。

現在の操作・保存条件は[譜面ダウンロード](../../docs/chart-downloads.md)、
形式ごとの制限は[書庫内譜面の直接再生](../../docs/archive-songs.md)を参照してください。

## 実装

- `bmz-song-resource`へ形式・拡張子・署名・表示名の共通定義を置きます。
  `.7zip`、`.tgz`、`.lha`も認識し、HTTP保存には正規の拡張子を使います。
- `song_archive`へtar.gzとLZHのreaderを追加し、HTTPの展開も同じreader・検査を通します。
  tar.gzは順読み、LZHは索引で選択したメンバーを読み、資源をディスクへ事前展開しません。
- 部分スキャンの対象に書庫ファイルを追加します。ファイル指定ではEverythingとディレクトリ探索を使いません。
  CLIとF5もこの経路を利用します。
- 書庫単体のスコープ判定・ハッシュ照合・更新時の削除範囲を元書庫単位で扱います。
  同じ親フォルダの別書庫は削除対象にしません。読めない書庫の既存登録は保持します。
- `downloads.http_keep_archive`は既定値falseです。ツールとライブラリ設定の両方で変更できます。
  キュー追加時に要求へコピーし、一括取得は確認画面からキューへ追加するときの値を使います。
- 保存方式をタスク統合とURL共有キャッシュのキーに含めます。
  登録失敗時の再スキャン、対象ハッシュ不一致時の再取得、同一書庫の重複保存防止、削除後の再取得を維持します。
- ライブラリDBのschema、譜面hash、スコア保存形式は変更しません。

## 検証

環境はWindows、Rust 1.99.1です。

- `cargo fmt --check`、workspaceのcheck、all-targets Clippy（`-D warnings`）が成功しました。
- `cargo test --workspace --locked --offline --no-fail-fast`が成功しました。
  `bmz-player`は2,820件成功、53件ignoreです。
- 最終調整後の`cargo test -p bmz-player --locked --offline song_ --lib --quiet`も112件成功しました。
- 5形式すべてについて、生成fixture／同梱のBMZ所有RAR fixtureで、直接読み取り、資源registry、展開、
  HTTP書庫保存、部分スキャン、要求ハッシュの登録確認を実行しました。外部素材の不在によるスキップはありません。
- LZHはLH0 / LH5、ヘッダーレベル0〜3、CP932の`0x5c`、拡張ディレクトリ名、CRC、リンク、パスと上限を確認しました。
  tar.gzはGNU long name、PAXのパス、ルートディレクトリ、CRC、危険なパス、リンク、サイズとヘッダー上限を確認しました。
- 保存方式の混在、共有書庫、DB登録の再試行、同じ書庫の再取得、書庫削除後の再取得、破損時の清掃を確認しました。
- Windows実行ファイルをbuildし、生成したZIP / tar.gz / LZHからAutoplayへ直接入るsmokeが成功しました。
  各形式で音源1件のdecode、Playへの遷移、3フレーム描画、正常終了をログで確認しました。
  設定・DBは`.local/validation/issue63/run-*`へ分離しています。

ビルドキャッシュのhard linkを作れずコピーする環境由来の警告があります。RustソースのClippy警告はありません。
通常のexe直起動はローカルのDLL探索パスが不足したため、smokeにはCargoがDLL探索パスを構成する`cargo run`を使いました。
smokeでは`gilrs-core`のWindows WGI入力スレッドに`HRESULT(0x80070057)`のpanicが記録されました。
アプリは継続し、3形式ともPlay描画と正常終了まで進みました。入力側の例外は今回の変更範囲では修正していません。

実サービスからの取得、実際の古いBMSのLZH書庫、設定UIの手動操作、macOS / Linuxでのbuild・実行は未実施です。
ローカルログ・生成データはGitへ含めません。
