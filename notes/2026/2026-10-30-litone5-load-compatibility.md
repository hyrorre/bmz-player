# LITONE5の読み込み互換性

2026-10-30。対象はローカル配置の`data/skins/LITONE5`。第三者製CSV・画像等は変更しない。
現在の契約は[skin.md](../../docs/skin.md)、対応範囲は[skin-compatibility.md](../../docs/skin-compatibility.md)を参照。

## 1. 1件だけのoffset定義

修正コミット: `0a139130`。

- 実行ログでSelectの`offset: invalid type: map, expected a sequence`を確認した。
  同じSelectをCONTINUE素材の提供元にするDecide / Playでも解決失敗が波及していた。
- `#CUSTOMOFFSET`の変換自体は正しいが、Lua / LR2共通の数値正規化が`offset`という
  キーだけで1要素配列を畳み込み、定義オブジェクトの配列を壊していた。
  数値画像の`offset`定義配列も同じ影響を受ける。
- `.local/beatoraja/.../lr2/LR2SkinHeaderLoader.java`の`CUSTOMOFFSET`は各定義を配列に
  追加し、件数1でも維持する。BMZもオブジェクト・配列を含む配列は畳み込まない。
  `x={10}`等の数値の互換変換は維持する。
- 修正前に合成の数値正規化テストと実在Selectで失敗を再現し、修正後に成功を確認。
  定義0 / 1 / 2件、数値画像offset、通常の数値変換、CONTINUE提供元を回帰テストにする。
- 実在する全9定義（Select / Decide / Result / Course Result / AC7 / CS7 /
  HALFWIDE7 / AC9 / AC14）の型付き読み込みを確認。素材が無い環境ではskipするため、
  今回配置がある状態で実行したことと、合成テストの成功を分けて扱う。
- 生ログは`.local/performance/litone5-offset-{before,after}.log`、
  `litone5-definitions-{before,after}.log`。画面上の描画・操作は未確認。
- 検証: fmt、workspace check / all-targets Clippy / testが成功。
  bmz-playerは2745成功・46 ignored、bmz-skinは355成功・1 ignored、
  bmz-skin-documentは28成功。全体テストはWindowsのリンク作成を許可した環境で実行した。

## 2. タグ付きCUSTOMFILEと9鍵盤追加グラフ

- AC9は同じディレクトリの`*|1P|` / `*|2P|`を別のCUSTOMFILE項目として使う。
  BMZは最初の`*`までしか照合せず、同じ設定を両側に適用し、実パスにもタグを残していた。
  また保存したフォルダーをファイル限定の存在確認で拒否していた。
- beatorajaの`SkinLoader.getPath`は設定パス全体のprefixで照合し、選んだ名前へ置換して
  後続パスを付ける。タグは実際のファイル名に残らない。`SkinHeader.setSkinConfig`は
  Randomの候補にディレクトリも含める。
- タグ付きパスを項目別に照合し、タグだけを除去する。拡張子や子CSVのパスは維持する。
  ディレクトリの保存値も受理する。Randomは項目ごとにロード内で共有し、opaque依存として
  文書キャッシュの再利用を避ける。既存の保存値の相対パス・親移動禁止は維持する。
- 合成fixtureで既定値、basename / 相対パスの別々の選択、不在・親移動する保存値のfallback、
  Randomの同じ項目を使う2つのincludeでの一致を確認。修正前に失敗を再現した。
- 実在するAC9で1Pに判定分布、2PにFAST SLOW分布とBPM推移を選び、両方のCSVと
  judgegraph / bpmgraphの生成、警告なしを確認した。
- 既定のOFFフォルダーは配置されているがCSVが無い。正しいOFFパスへの非致命警告は残る。
  beatorajaのINCLUDEも存在しないファイルは読み飛ばす。外部スキンへ空CSVを追加しない。
- SelectとAC9はapp側の実際の画像decode経路でも確認した（それぞれ10 / 42 sources）。
  生ログは`.local/performance/litone5-tagged-{before,after}.log`、`litone5-app-assets.log`。
- 検証: fmt、workspace check / all-targets Clippy、Windows debug buildが成功。
  workspace testではbmz-skin 359成功・1 ignored、bmz-render 974成功・11 ignored、
  bmz-skin-document 28成功など、bmz-player以外の全crateが成功した。
  bmz-playerは書庫先読みテストだけが共有registry取得時に1回失敗し、単独再実行と
  bmz-player全体の並列再実行（2745成功・46 ignored）は成功した。
  別の既存テストがregistryをclearする経路は確認したが、干渉の原因は断定していない。
  今回の修正外のテストやregistryは変更していない。
  ログは`litone5-tagged-workspace-{check,clippy,test}.log`、`litone5-preload-rerun.log`、
  `litone5-player-rerun.log`、`litone5-windows-build.log`。

## 確認範囲と残る表示差

- 全9定義の読み込み成功は、全描画命令の対応や見た目の一致を意味しない。
  SelectのBAR_TROPHY / BAR_GRAPH / BAR_LABEL / NOTECHARTは未対応命令の警告が残る。
  Result / Course Resultのsource 21未定義や、Decideが参照する未配置の`Select/font/bar`も
  今回の読み込み停止とは別の課題。外部CSVや画像の書き換えは行っていない。
- beatorajaの実画面との比較、GPU画面の見た目・操作、macOS / Linuxは未確認。
