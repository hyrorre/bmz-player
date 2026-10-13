# LR2選曲のDP表示・ホバー・スコアバー

## 調査と変更

WMIX_HD選曲スキンで、STYLEが14KEYSでも2PのLANE OPTIONとDP FLIPが表示されず、
STYLEへマウスを置くと発光が左へずれる。SCORE DATAのF〜AAAバーも空になる。
第三者スキンのCSV・画像を変更せず、BMZ側の変換と条件評価を修正した。

- LR2 option 10 / 11 / 12を専用namespaceへ変換し、選曲STYLEからDP条件を評価する。
  LR2参照実装の`GetOptionFlag_dst`と同じく、選択譜面の鍵盤数からは判定しない。
  BMZの14 / 10鍵フィルターで10と12がtrue、未対応のBATTLEは11がfalse。
- パネル条件20..29も描画時に評価する。`SRC_ONMOUSE`のpanel指定を局所マウス範囲と
  一緒に保持し、描画条件へ追加する。panel=0は共通、指定ありは該当パネルでだけ表示する。
  LR2の`AddDrawingBuffer_OnMouse`に合わせ、DSTの条件とも併用する。
  WMIX_HDではSTYLEがx=415、正しい発光がx=401。誤って重なったSCOREパネル用の
  発光はx=373であり、座標補正ではなくパネル制限が必要だった。
- スコアバーのoption 572（コース作成中ではない）をtrue、571をfalseとして供給する。
  BMZにコース編集画面が無いため、この条件は固定。BARGRAPH 47→147の変換とスコア率
  計算は既に動作していた。EX SCORE 3115 / 3400なら91.62%の幅になる。

## 検証

- `cargo fmt --check`、`cargo check --workspace --locked`、
  `cargo clippy --workspace --all-targets --locked -- -D warnings`成功。
- 変換・条件評価の回帰テストを追加。STYLEのDP条件、`#IF`の条件保持、
  `SRC_ONMOUSE`のパネル条件と局所範囲を扱う。
- `bmz-player`の追加2テスト成功。素材不要のCSVと実在するWMIX_HDのCSVで、
  14K→7K→10K→ALL→14Kのクリック領域、PLAY / SCORE / 閉じた状態の発光、
  0% / 50% / 91.62% / 100%のバー幅、フォルダでの非表示を確認した。
  テクスチャは描画項目確認用の仮IDであり、GPUや実画面での表示確認とは区別する。
- `cargo test --workspace --locked --no-fail-fast`成功。`bmz-render`は727件、
  `bmz-skin`は272件成功。追加した変換・描画条件・実CSVのテストも実行された。
  制限下の初回実行は、既存のリプレイ保存でアクセス拒否となり、ローカルHTTPサーバーを
  使う既存テストが待機したままになったため終了。承認された制限なしの実行で全体成功を
  確認した。ログは`.local/lr2-select-conditions-tests-host.log`（Git管理外）。
- Windows上の実画面・実マウス操作は未実施。Rustのincremental cacheがhard linkを
  作れずcopyへ切り替わる警告があるが、check / Clippyは終了コード0。
