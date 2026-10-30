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

## 3. Selectのノーツ分布

- beatorajaの`LR2SelectSkinLoader`は`SRC_NOTECHART`の幅・高さ・delay等を使い、
  `DST_NOTECHART`のYを`src.height - y`へ変換する。BMZの`NOTECHART_1P`と同じ変換を
  Selectの命令名にも接続する。SRC/DST間に別種の部品があっても対象を保持する。
- SRCの寸法、2つのDSTキーフレーム、offsetの維持と、LITONE5にある2つのグラフを確認する。
  app経由の描画計画で位置・表示遅延と、解析未完了の別の曲へ切り替えた際の非表示を検証する。
  外部スキンへ変更は加えない。
- ログ: `.local/performance/litone5-drawing-notechart*.log`。
- 検証: fmt、bmz-skin check / all-targets Clippy / test（361成功・1 ignored）と
  app側のノーツ分布描画回帰テストが成功。実画面の比較は未実施。

## 4. 曲・コースバーのラベル

- `SRC/DST_BAR_LABEL`の0..4をsonglistのlabelスロットへ接続する。
  欠けたスロットを詰めず、画像とDSTの宣言位置を保持する。
- beatorajaの`BarRenderer`と同じく、宣言済みLN / CN / HCNと未定義LNのユーザー設定から
  最も強い種別を選ぶ。専用のCN / HCN画像・textureが無い場合はLN画像を使い、
  専用ラベルの表示条件が偽なだけの場合はfallbackしない。
  譜面特性の表示なので、BMZ独自のFORCE判定設定は宣言済みの表示種別を上書きしない。
- コースのentry previewに既存library情報を渡し、解決済みの構成譜面の特性を集約する。
  全曲を所持していないコースは描画対象にしない。DB/configの変更は無い。
- 合成CSVの0..4の対応・無効index・座標・宣言順、CN画像の選択、fallbackと非表示条件、
  コースの特性集約・未所持への切替をテストする。LITONE5の5つのラベル定義も読み込む。
- 検証: fmt、workspace check / all-targets Clippy / testが成功。
  bmz-player 2748成功・46 ignored。ログは`.local/performance/litone5-drawing-label-*.log`。

## 5. コーストロフィー

- `SRC/DST_BAR_TROPHY`の0..2を既存のcourse trophy描画へ接続する。
  無効indexを無視し、負の幅・高さはbeatorajaのSelect loaderと同じく正の矩形へ移す。
  同じ処理をBAR_LABELにも適用する。
- `GradeBar.getTrophy`はコース定義を末尾から調べるため、宣言順をsnapshotにも保持する。
  先に見つかった達成済みの名前が独自名ならメダルを表示しない点も維持する。
  定義順を持たない旧snapshotでは既存の銅・銀・金の最高達成を使う。
- SRC/DSTと宣言位置、負の寸法、達成銅・銀・金、独自名、未所持、曲バーでの非表示を確認する。
  メダル順と異なるコース定義順でもbeatorajaの優先順を使うテストを追加する。
- 検証: fmt、workspace check / all-targets Clippy / testが成功。
  ログ: `.local/performance/litone5-drawing-trophy-*.log`。

## 6. フォルダーのクリアランプ分布

- `SRC/DST_BAR_GRAPH`のtype 0を既存のフォルダー分布描画へ接続する。
  beatorajaの`LR2SelectSkinLoader`と`SkinDistributionGraph`の画像配列変換に合わせ、
  SRC格子を行順に11セルずつアニメーションフレームへ分ける。
  11列以外の格子も扱い、DSTのindexはbeatorajaと同じく単一グラフとして無視する。
- `SkinGraphDef`に内部変換用`lr2Distribution`を追加する。SRCの正のtimerを使い、
  OFF / 開始前は先頭フレームとする。0以下は未指定として画面の経過時間を使う。
  DSTの移動・条件・合成・補間とは独立に扱う。
  既存JSON / Luaの分布画像とダウンロードメーターの処理は維持する。
- ランプ10→0の順・曲数比率で配置し、空フォルダーやダウンロード中を含む通常の曲バーには出さない。
  type 1の28種類のランク分布は今回の対象外で、警告して以前のtype 0を流用しない。
- 合成CSVの格子・時刻・DST、22列×2行の画像と2つの時刻、SRC timer OFF / 開始前 / 負のID、比率、
  空フォルダー・通常曲をテストする。appの変換から描画までの接続も確認する。
  LITONE5 Selectの2つのノーツ分布、5ラベル、3トロフィー、1ランプ分布が読み込まれ、
  未対応命令の警告が無いことを検証する。
- 検証: fmt、workspace check / all-targets Clippy / test、Windows debug buildが成功。
  player 2,750件・render 978件・skin 365件が成功し、GPU等を要する既存のignoredテストは未実行。
  実アセットの全9定義を再確認し、Selectの変換警告は0件だった（素材欠如によるskipなし）。
  ログ: `.local/performance/litone5-drawing-final-{check,clippy,test,installed,build}.log`。

## 確認範囲と残る表示差

- 全9定義の読み込み成功は、全描画命令の対応や見た目の一致を意味しない。
  今回調べた4系統は対応済みで、配置済みSelectの既定設定で読み込み警告が無い。
  BAR_GRAPH type 1は未対応だが同スキンはtype 0を使う。
  Result / Course Resultのsource 21未定義や、Decideが参照する未配置の`Select/font/bar`も
  今回の読み込み停止とは別の課題。外部CSVや画像の書き換えは行っていない。
- beatorajaの実画面との比較、GPU画面の見た目・操作、macOS / Linuxは未確認。
