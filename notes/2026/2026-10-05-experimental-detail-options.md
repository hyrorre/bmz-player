# DETAIL OPTIONSのexperimental化と2パネル操作

基点: `feat/select-detail-options` / `05b1872cb6a4c80919e348e72a64eda8560392be`。
開始時は親repo・対象submoduleとも差分なし。mz-selectの基点は`c7bccd16`、Luxez-Flatは`9f17aeeb`。
ユーザー承認済み計画に従い実装した。push・PR作成は行わない。

## 変更と判断

- app configの`select.experimental_detail_options`を追加。既定・既存configの欠落はOFF。
  本体設定の選曲ページで切り替え、選曲スキンを世代管理付きで再読込する。
- ONかつ対応スキンでは通常／詳細の2パネル。E1押下で即表示、300ms未満の解放で固定、
  300ms以上の解放で閉鎖、固定中の次のE1押下で閉鎖。表示中のE2押下だけで切り替える。
  再オープンは通常から。論理状態と物理holdを分け、プレイ等の操作を変更しない。
- 旧詳細のGAS方式、緑数字、BGA、判定表示自動調整、表示オフセット、判定アルゴリズムを統合し全21項目。
  先頭4項目と既存の安定IDは維持。数値はregistryの範囲でclamp、400ms後から60ms間隔で内部リピート。
  保存・モード別設定・選曲一時値・プリロード無効化は既存経路へ接続する。
- v1のAPI番号を維持し、`bmzDetailOptionsNumbers: true`を追加。数値を描画できない旧宣言だけで
  新操作を有効にしないため、対応を明示する。数値eventは19320..19337、行の予約フィールドを拡張する。
- Luaへ本体予約option `bmz_detail_options`を渡す。OFF時は部品がnilを返し旧Assistの開閉destinationを残す。
  ユーザーのスキンカスタマイズには保存しない。default JSONにも従来の7鍵Assist表示・クリックを補う。
- default、mz-select、Luxez-Flatに数値欄と増減ボタンを追加。後者2つの素材・フォント・角丸の切り出しは維持し、
  通常↔詳細の両方向で退出と登場を並行再生する。画像・フォントファイルの変更はない。
- 固定表示中は通常パネルも入力を専有する。背後の曲開始、旧詳細の同時押しショートカット、検索クリック、
  曲リストのwheelへ漏らさない。フォーカス・モーダル・入力再同期で誤った短押しや開き直しを作らない。

確定仕様と手動手順は[設計](../../docs/select-detail-options.md)、[操作](../../docs/controls.md)、
[Skin API](../../docs/skin.md#bmz-select-detail-options-v1)を参照。

## 検証

macOSで実施。ログはGit管理外の`.local/detail-options-*.log`。

- `cargo fmt --check`、`cargo check --workspace --locked`、
  `cargo clippy --workspace --all-targets --locked -- -D warnings`成功。
- `cargo test --workspace --locked --no-fail-fast`を実行。今回の機能テストは成功。
  bmz-playerの既存17テストはsandboxによるlocalhost HTTP/IPC待受拒否で失敗したため、
  sandbox外で`cargo test -p bmz-player --locked`を再実行し、2,149成功・10 ignored。
  bmz-renderの既存BMP素材テスト1件は並列時に別テストの画素を読み込んで失敗した。
  `cargo test -p bmz-render --locked -- --test-threads=1`では660成功・3 ignored。
  他のworkspace crate・doc testは初回から成功。無関係なテスト実装は変更していない。
- 最終の詳細オプション関連テストは41成功・4 ignored。
  設定の欠落時OFF／保存再読込、300ms境界、固定／holdでのE2切替、解放順序、resync、
  数値リピート停止・境界no-op・モード別保存、21項目、数値ref/option/event、
  3スキンのOFF/ONとクリック経路、通常↔詳細アニメーションを含む。
- Metal offscreenで3スキンの日英・960×540、1280×720、1024×768、1920×1080等を生成。
  mz-selectとLuxez-Flatは2560×1080のpillarboxも検証。
  数値欄・増減ボタン・説明・長い候補名の代表画像を目視した。
  最初のsandbox内GPU実行はアダプタを列挙できず、sandbox外では成功した。

## 未実施

実コントローラー、OS表示スケール、Windows/Linuxの実機は未実施。
通常のウィンドウでの連続操作、実際のプレイ開始と再起動を通した手動確認も未実施。
保存はTOML往復・モード分離、入力は状態遷移テスト、表示はoffscreenで確認した範囲であり、
実機確認済みとは扱わない。設計書末尾の手順で確認する。
