# DETAIL OPTIONSのレビュー7件への対応

## 対象

`feat/select-detail-options`、開始時HEAD `8208c5c57d724fd8adf70b90b7f0ae8028294c60`。
`main`の`3ddd25453aca81557e7d0af61fdaf1eeb426d85f`は取り込み済みで、開始時の作業ツリーはclean。
レビュー対象は以前の`364c4d75`だが、7件とも現在の経路に残っていることを確認して修正した。

仕様: [詳細オプション](../../docs/select-detail-options.md)、[操作方法](../../docs/controls.md)。

## 修正と回帰確認

1. defaultのtext ID重複を解消。新パネルのtitle/countに固有IDを付け、
   実験設定ON/OFF・パネル非表示で曲名／フォルダー譜面数のDrawCommandを確認した。
   展開後の全text IDの一意性と、既存の詳細パネル描画・クリックも確認した。
2. 元譜面モードと設定編集スロットを別の関数へ分離。
   RANDOM MIXの候補抽出とsnapshotのsourceは元譜面、HS参照・編集・保存は変換後を使う。
   7K→9K、SP→DP、7K→6Kで元の7K候補が残ることと変換先を確認した。
3. セッションによる変換抑止をプレイ開始・選曲で共通化。
   AUTOPLAY BATTLE / G-BATTLE、DOUBLE OPTIONのBATTLE系、対戦対象ありではsourceの設定を使う。
   7KのSUDDEN・緑数字・判定調整を変更・シリアライズし、変換先設定が変わらないことを確認した。
   譜面変換・スコア分類・IR条件の変更はない。
4. パネル利用不可時は入力を消費せず、論理セッションを解除。
   Q/Wを含む`waveq`の押下・リピート・解放が後続処理へ渡り、モーダル終了後の解放で固定表示しないことを確認した。
   退出アニメーション中の入力遮断もモーダルには適用せず、遷移直後から入力先へ渡す。
5. コントローラー入力は既存14K resolverの後に7K resolverへフォールバック。
   独自Button13、14Kの2P、9K、異なるデバイス、ローカル奇偶を確認した。
6. プロファイル切替のdecodeでも本体予約optionを生成。
   切替・新規作成・コピーの有効化を実際に準備し、ON/OFFで対応宣言を確認した。
   空パスのdefault fallback、保存済み独自optionの保持、本体予約値の優先も確認した。
7. Escape解放を入力遮断より前で処理し、パネル遷移で終了タイマーを解除。
   終了判定でも現在のEscape保持とパネル非表示を確認する。
   通常／詳細・長押し／固定表示・キー解放・閉じた後のOSリピートによって古いタイマーが再開しないことと、
   通常選曲での1,200msの終了閾値を確認した。既存の入退場timerテストも維持した。

## 検証

- 各指摘の回帰テストを個別実行して成功。
- `cargo fmt --check`: 成功。
- `cargo check --workspace --locked`: 成功。
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: 成功。
- `cargo test --workspace --locked --no-fail-fast`: 3,697成功、失敗0、ignored 27。
  bmz-playerは2,173成功、ignored 19。子プロセスで再実行される1テストは重複集計しない。
  今回追加した9件の回帰テストも全て成功した。

旧`data/skins/Starseeker/play/play7.luaskin`がないため、1件は素材検査で早期returnした。
この1件は上記のtest harness上の成功件数に含まれるが、当該スキンの互換検証済み件数には数えない。
現行`data/skins/ADFX02/Starseeker/result/result.luaskin`を使うランク差分テストは実素材で成功した。

ローカルログは`.local/review-detail-*.log`（Git管理外）。
全テストはローカルHTTP待受を使うテストのためsandbox外で実行した。
GPUプレビューと実機のキー／コントローラー操作は今回未実施。
入力ルーティングの自動確認はwindowを作らない境界のテストであり、実機での通し操作とは区別する。

## 手動確認手順

1. defaultスキンで実験設定OFF/ONそれぞれの曲名とフォルダー譜面数を確認する。
2. 7Kフィルター・7K→9KでRANDOM MIXを開始し、元7K譜面が候補になり、変換表示も正しいことを確認する。
3. 7K→9Kを保存したままAUTOPLAY BATTLE/G-BATTLEへ切り替え、詳細のスコープが7Kであることを確認する。
   SUDDENや緑数字を変更して開始し、7K設定の反映と9K設定の保持を確認する。
4. 実験設定ONで検索に`waveq`を入力する。スキン内キーコンフィグでもQ/Wを割り当てる。
5. 7KのController KEY3だけをButton13へ変え、新詳細で値が変更できることを確認する。
   2台接続時は1P/2Pを切り分け、9Kでも独自割当を確認する。
6. 実験設定ON/OFFそれぞれで別プロファイルへ切り替え、新規／コピーの有効化でも操作方式が維持されることを確認する。
7. Escape押下→E1で開く→Escape解放の後に1.2秒以上待ち、終了しないことを確認する。
   詳細・固定表示・パネル閉じ直後にも繰り返す。通常選曲でEscapeを新しく1.2秒長押しした場合は終了することを確認する。
