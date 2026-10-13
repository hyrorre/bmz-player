# 選曲バーの連続移動補間

## 背景と根拠

- BMZは選択index変更時に1行分の補間を再開始し、途中の移動残量を捨てていた。
  スクラッチの複数行入力もループで上書きし、ページ移動も1行分だけ補間していた。
- LR2の`Scene02_Songselect.cpp`は現在の`listTopbar`を`oldBar`へ引き継ぎ、
  `ChangeValueByTime(..., 0)`で新しい`nowBar`へ線形補間する。
  長押しは移動終了20ms前から次へ接続し、BAR_FLASHは中央の未補間destinationを原点にする。
- beatorajaの`BarRenderer.java`は隣接スロット間の補間と入力用duration/angleを共有する。
  BMZの従来処理はそれを参考にしていたが、再入力・複数行入力を連続座標で保持していなかった。

## 実装

- `SelectBarScroll`で符号付き残量・開始時計・所要時間を保持する。
  再入力ではその時点の残量に実移動行数を加え、位置を連続させる。
- キー・ホイール・長押し・スクラッチ・G-BATTLEで共通の補間を使う。
  スクラッチの複数行分は1回の更新にまとめ、Page Up/Downは7行を補間する。
- 長押しの接続は最大20ms前。短い設定では所要時間の半分を上限とする。
- 選択index周囲の論理行は参照・操作用に維持し、遅れている描画位置周囲の行を追加で供給する。
  大きい移動残量でも描画用の行数を増やさず、現在の25行のウィンドウを維持する。
- 描画とクリックで整数スロット移動・小数の座標/幅/高さの補間を共用し、ランプ等の子部品も同じ原点へ配置する。
  中央flashはスロットに固定し、行境界で発光が飛ばないようにする。
- 選択変更タイマー（プレビュー・skin timer）と補間時計を分離する。
  Home/End・直接クリック・スライダー・一覧切替の既存リセット経路で補間を破棄する。
- 外部スキン、runtime config、DBは変更しない。

## 検証

- `cargo fmt --check`、`cargo check --workspace --locked`、
  `cargo clippy --workspace --all-targets --locked -- -D warnings`成功。
- `cargo test --workspace --locked --no-fail-fast`成功。既存のファイル操作・localhost模擬サーバーを
  含むため、実行制限外で検証した。生ログはGit管理外の`.local/select-continuous-scroll-tests.log`。
- 残量引き継ぎ、連打、反転、7行移動、循環、長押し、リセット、短い時間設定、
  60/120/144Hz相当の経過時刻、f64からf32へ変換する際の整数境界丸めを回帰テストで確認。
- LR2/JSON形式の非等間隔・異なるサイズのdestinationで、再入力前後の同じ行の矩形、
  ランプの追従、クリック領域、中央flash固定、複数行遅れ時の論理選択保持を確認。
- WMIX_HDの実CSVが存在する環境で、1/7/36/100行の追加入力後も中央バーの描画とクリック位置が
  一致することを確認した。テクスチャID・サイズは模擬しており、GPU描画の確認ではない。
- ビルドには増分キャッシュのhard link失敗によるcopy fallbackの環境警告が出た。
  コードのwarningを抑制する変更は行っていない。
- Windows実画面、実キー・マウス・コントローラー操作、実モニター60/120/144Hzの録画比較、
  macOS/Linux実機は未確認。経過時刻の自動テストを表示FPSの計測結果として扱わない。

仕様と操作: [skin.md](../../docs/skin.md)、[controls.md](../../docs/controls.md)。
