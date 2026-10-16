# HTTP/IPFS譜面ダウンロードの管理・一括取得

開始日: 2026-10-16。基点はmain `cb5f9037`、ブランチは `feat/chart-download-manager`。
現在の操作と契約は[譜面ダウンロード](../../docs/chart-downloads.md)、[操作方法](../../docs/controls.md)を参照。

## 現行実装の調査と判断

BMZの `song_download.rs` は1バッチを逐次取得し、appの `pending_chart_download` が存在すると
追加要求を拒否していた。アーカイブ保存後は方式別ルート全体をスキャンし、要求した譜面の
ハッシュとスキャン結果を結び付けていなかった。
未所持行のSHA256キーが所持済みのChartIdキーへ変わり、復元が以前のインデックスへ戻る問題もあった。

参照コピー `.local/lr2oraja-endlessdream` で `HttpDownloadProcessor`、`DownloadTask`、
3つのソース、`HttpDownloadSource`、`DownloadTaskMenu`、`ContextMenuBar`、`MusicSelectCommand` を確認した。
EDは固定5ワーカーとURL重複判定、表/レベルのMD5一括照合、メモリ内の管理UIを持つ。
参照したEDの完了状態は展開終了であり、対象ハッシュのDB登録までを表すものではなかった。

BMZでは既存の転送・安全な展開処理を再利用し、app所有のキューと固定上限のworkerを用いた。
HTTPは初期3・実効1〜5、IPFSは1、展開は共通1。
DB書き込みと選曲モデル反映は既存のSelect保守ゲート・FIFOスキャンキューへ統合した。
Play/Result中も転送は継続するが、新しい登録スキャンと完了通知の反映は次のSelectまで保留する。

## 選曲復元と登録

通常の所持済み行ではChartIdを維持し、同じファイルの別コピーを区別する。
未所持→所持済みの切替だけSHA256/MD5で橋渡しし、同じハッシュの複数参照では元の位置に近い行を選ぶ。
単一取得には要求ID、タスクID、対象ハッシュ、フォルダ履歴、開始時の選曲キー・位置・操作世代を紐付ける。
選曲・フォルダ・フィルタ・プロファイル変更後は自動移動しない。一括取得は復元要求を作らない。

Completedは取得先の部分スキャンと対象ハッシュのDB登録確認後だけ設定する。
同名の別譜面を成功とせず、複数対象を共有するアーカイブは対象ごとに照合する。
フィルタで空になっても、ダウンロード後の更新ではフォルダとフィルタを維持する。

## 一括取得・設定・UI

F1 → 譜面ダウンロードから表/レベルの件数を確認して確定する。既存スキンに新イベントやキーを要求しない。
所持済み照合は既存の一括DB検索を使い、比較時だけハッシュを正規化して元の表記を保持する。
ブラウザによる手動取得は自動キューに混ぜない。
管理UIは状態別の一覧、容量不明時のバイト表示、再試行、個別・待機中一括キャンセルを持つ。
一覧は表示行のみ描画し、短い間隔で集計を更新する。キューの全レコードを毎フレーム複製しない。

HTTPプリセットはEDのURL定義を候補として採用した。2026-10-16に未登録ハッシュへの応答を確認し、
Gingerは404、Konmaiは404と取得不可JSON、Wriggleは検証環境でTLS接続失敗だった。
実譜面取得の成功やサービス継続は保証しない。既存カスタムURLとHTTP無効設定を保持する。
SHA256のみの取得は、そのハッシュを明示したCustomテンプレートに限定する。

## セルフレビューで追加した修正

- 後着の共有タスクを待機/実行中スキャンへ合流し、同じディレクトリの再走査を避ける。
- 登録確認も共有ディレクトリごとに行い、対象の照合と集計にはハッシュ集合を使う。
- 大量投入の重複検索は索引を用い、実行中タスクとスキャン待ちだけをpollする。
- 展開待ちをキャンセル可能なsemaphoreに変更し、終了時にはworkerの一時ファイル清掃を待つ。
- 排他的に作成した専用workディレクトリだけを削除し、他タスクや既存曲フォルダを削除しない。
- gzipの末尾まで検証し、共有キャッシュの所有者交代時には別所有者の項目を消さない。
- HTTPエラー表示からURLの認証情報とクエリを除去する。
- 対象譜面がない場合の共有キャッシュを無効化し、再試行で取得元の修正を取り込めるようにする。

## 検証

Windows上のローカルHTTP fixtureと一時DBを利用する。実サービスに依存する自動テストは追加しない。
サンドボックスでlocalhost接続が `os error 10013` になったため、通信を含む試験は通常Windows権限で再検証した。
fixtureのacceptソケットもWindowsでは非ブロッキング設定を継承するため、handlerでは明示的に解除した。

対象試験でHTTP並列数1/3/5、追加投入、共有取得、所有者キャンセル時の引継ぎ、待機/実行中キャンセル、
再試行、正常/異常JSON、404/500、リダイレクト、Content-Length不明、容量制限、不正7z、
パストラバーサル、gzip checksum、一時領域清掃、部分スキャン・DB照合、選曲復元を確認する。
1,000件のUI仮想一覧、10,000件のキュー投入とworker生成抑制もテストする。
これらは性能の実測比較や実プレイの負荷検証を意味しない。

検証結果:

- `cargo fmt --all -- --check`: 成功。
- `cargo check --workspace --locked`: 成功。
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: 成功。
- `cargo test --workspace --locked --no-fail-fast`: 成功（既定のignore対象を除く）。
- レビュー後の最終 `cargo test -p bmz-player --locked`: ライブラリ2,547件成功、32件ignore、失敗0件。
  ネイティブ起動試験2件は既定のignore。別途42件のダウンロード試験を実行し、42件成功・ignoreなし。
- i18nの9試験、1,000件のUI仮想一覧、10,000件のキュー投入試験も成功。

既存のignoreはGPU/ネイティブwindow/実音声機器/外部スキン素材/個別データ/手動計測を要求するもの。
今回追加したダウンロード試験を素材不足による早期returnやignoreで成功扱いにはしていない。
ビルド時のincremental cacheハードリンク失敗はコピーへのfallbackで、ソースの警告抑制は追加していない。
生ログはGit管理外の
`.local/performance/chart-download-manager/` に保持する（この環境以外には共有されない）。

未実施: 実画面でのF1/選曲/プレイ遷移操作、実IPFSノード、実HTTPサービスからの譜面取得、
macOS/Linux実機、プレイ中のCPU/音声/GPU負荷の比較測定。
履歴の永続化・再起動後の再開・自動再試行は今回の対象外。

## 2026-10-16 追記: Wriggle無効化とKonmaiの実応答

Wriggleは利用できないというユーザーの指摘に合わせ、プリセットの選択と既定URLを無効化した。
保存済みenumは引き続き読み込み、明示URLとHTTP有効/無効は変更しない。
URLが空なら既存のIPFS・手動URL選択へ戻る。設定往復・HTTP選択・IPFS優先の回帰試験を追加した。

Konmaiの未登録ハッシュ応答だけでは稼働確認が不足していたため、公開Satelliteの段位定義にある
MD5 `9ee5524e4ed8a3e7033b4de1fedc35fd`、`9498378a847b94b949d11a566a86d3c0`、
`4023045d9134dfc47e953bc430e5ef07` の3件を読み取り確認した。
すべてHTTP 200、`result: success`、`data.song_url` を返し、BMZの既存JSON解析形式と一致した。
最後の「Life is PIANO [Another]」の取得URLはHEADでHTTP 200、7zのContent-Type、
106,431,803バイトを返した。Range `bytes=0-31` もHTTP 206で応答し、
先頭署名 `37 7A BC AF 27 1C` を確認した。実サービスの全取得・展開・DB登録は行っていない。
生ログの先頭32バイトと応答ヘッダーは `.local/performance/chart-download-manager/konmai-prefix*` に保持する。

追記後の検証: `cargo fmt --all -- --check`、`cargo check -p bmz-player --locked`、
`cargo clippy -p bmz-player --all-targets --locked -- -D warnings` が成功。
`cargo test -p bmz-player --locked` はライブラリ2,549件成功・32件ignore・失敗0件。
ネイティブ起動試験2件は既定のignore。設定と取得方式選択の追加2試験、翻訳キー整合性も成功。
ログは同じローカルディレクトリの `check-sources.log`、`clippy-sources.log`、`test-sources.log` に保持する。

## 2026-10-16 追記: レビュー対応と再試行の重複展開

レビューで、対象譜面を含まないアーカイブを再試行するたびに、同じ内容を別フォルダへ展開する問題を確認した。
`retry_of_missing_chart_refetches_an_updated_archive_without_deleting_old_songs` が示すとおり、
再取得は取得元のアーカイブ更新に追従するための設計であり、保存済みフォルダの再スキャンだけには変更しない。

LR2oraja Endless Dream（`.local/lr2oraja-endlessdream` 3c1a4129）の `HttpDownloadProcessor` と比較した。
- 取得URLは投入時に1回だけ解決し、再試行は同じURLを再実行する。メタデータAPIは再照会しない。
- 再試行はダウンロード失敗のError状態だけが対象。展開失敗はDownloaded状態のまま残り、再試行できない。
- 展開先はダウンロードフォルダ直下で、アーカイブ内のパスへ上書きする。重複フォルダはできないが、既存ファイルを上書きする。
- 対象譜面がDBへ登録されたかの確認はなく、展開できれば完了扱い。
- 同じ取得URLのタスクは、完了・失敗後も含めて再投入を拒否する。

BMZは既存フォルダを上書きしない方針を維持し、成功結果にアーカイブ全体のSHA256（IPFSは取得順に連結）を保持する。
対象なしの再試行で同じSHA256を取得した場合は展開せず `ArchiveUnchanged` で失敗にする。
前回の展開フォルダが無い場合は比較しない。取得元が更新された場合だけ新しいフォルダへ展開する。

同時に、取消要求が未反映のタスクへの統合禁止、取消と完了が重なった転送の登録継続、
登録中のキャンセル無効化、共有アーカイブ待機のNotify化、失敗トーストの集約、
`:` を含むアーカイブ名の拒否のWindows限定化、identity照合の共通化を行った。

検証: `cargo fmt --check`、`cargo clippy -p bmz-player --all-targets --locked -- -D warnings` が成功。
`cargo test -p bmz-player --locked` はライブラリ2,554件成功・32件ignore・失敗0件。
未変更アーカイブの再試行と、展開フォルダ削除後の再展開を統合試験で確認した。
実HTTPサービスでの再試行、実画面操作は未実施。
