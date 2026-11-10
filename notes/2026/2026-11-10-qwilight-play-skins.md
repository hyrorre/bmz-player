# Qwilightプレイスキンの試験対応

- 作業日: 2026-11-10
- 基点: `1140fb89`、ブランチ: `feat/qwilight-play-skins`
- 対象: Playのみ。設定・DB・外部スキンは変更しない。

## 参照と方式

インストール先は`G:\SteamLibrary\steamapps\common\Qwilight`。
本体のProductVersionは`1.17.14+e9ef433d56a72aeba8a00a42cb5294d2c61a2ab8`。
同じrevisionの[公開ソース](https://vcs.taehui.net/root/qwilight/-/tree/e9ef433d56a72aeba8a00a42cb5294d2c61a2ab8)
のUI定義・ComputedState・DrawingSystem・LongNoteを参照して、ファイル契約をRustで独立実装した。
原作のC#コードと外部スキン素材はGit管理対象に含めない。

YAMLとLuaによる配置評価を`bmz-skin`、ZIP読み取りを`bmz-skin-assets`に置き、
既存SkinDocumentへ変換してappの素材デコード・GPU描画へ渡す。
分割PNGのatlas化とLNの独立したcap寸法のみ共有契約を拡張した。
スコア等はBMZの値で、Qwilightのゲームルールは移植しない。

実素材で見つかった差として、数値と`then`間の空白省略、DP lane mapの展開、
`!`を含む素材名、画像フレームの省略時の引き継ぎ、LN上下端・bodyの重なりを扱った。
曲アーカイブregistryが`!/`を所有するため、スキンZIPは`!skin/`の仮想名に分離した。

## 検証

`SavesDir (beta)/UI`のDefault、CR_Core、CR_DX、CR_Fancy、CR_Mania、CR_minimal、
CR_natural、CR_simple、QMAX_Humanoid、QMAX_templateを対象に、
4K / 5K / 6K / 7K / 8K / 9K / 10K / 14Kの計80ケースで定義と素材パスを確認。
Default / CR_DXの7KとCR_simpleの14Kは実画像をデコードし、WindowsのGPUオフスクリーンで描画した。
プレビューは`.local/qwilight-previews/`に置く（Git管理外）。

再現コマンド（PowerShell）:

```powershell
$env:BMZ_QWILIGHT_SKIN_ROOT='G:\SteamLibrary\steamapps\common\Qwilight\SavesDir (beta)\UI'
cargo test -p bmz-skin --locked installed_qwilight_play_skins -- --ignored --nocapture
$env:BMZ_QWILIGHT_PREVIEW_DIR=(Join-Path (Get-Location) '.local/qwilight-previews')
cargo test -p bmz-player --lib --locked qwilight_installed_play_skins_gpu -- --ignored --nocapture
```

通常テストでは自作YAML / Lua / PNG / ZIPによって、設定変更、レーン順序、DP、
atlasの画素、LNの形状とアニメーション、ゲージ、カタログ、root境界を確認する。
LuaのOSアクセス遮断と、保護呼び出しで命令制限を回避できないことも確認する。

最終状態で以下を実行し、すべて成功した。Windowsの既存incremental cacheの
hardlink警告を避けるため、検証時は`CARGO_INCREMENTAL=0`を指定した。

```text
cargo fmt --all --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked --no-fail-fast
```

`bmz-player`は3,003件、`bmz-render`は1,016件、`bmz-skin`は416件、
`bmz-skin-assets`は17件が成功。外部素材・GPU等が必要なignoredテストを
通常テストの成功件数へ含めない。生ログは`.local/qwilight-workspace-final.log`に置く。

手動プレイ・実ウィンドウ・実入力・音声・macOS / Linuxでの実行は未確認。
全オプション組合せや原作との画素一致は確認していない。
未対応機能は[現在の仕様](../../docs/skin.md#qwilightのプレイスキン)に記載する。

## 追記: 判定割当てとHS・達成率

初回実装`a7f946bc`ではBMZのPOORをfailed、空POORをPOORへ割り当てていた。
ユーザーの指定により判定画像と回数欄の両方を入れ替え、POOR → POOR、空POOR → failedに変更した。
PGREAT / GREAT / GOOD / BADの対応は変更しない。

`multiplier`へ表示HS（310 / 311）、`point`へプレイ中EXスコアレート（102 / 103）を追加。
既存の整数・小数参照を使用し、どちらも小数点以下2桁とする。EXスコアレートの分母は
処理済みノーツ数の2倍で、開始前は100.00%。元のQwilightの採点値は使用しない。
数字・小数点・単位の素材を再利用し、整数部の桁数に応じた既存draw条件で配置を切り替える。
rendererのschemaやruntime VMは追加しない。
共通の小数点素材`Bin/..png`は、拡張子を取り除いた名前`.`をそのまま扱う。
filesystem用の`Path::with_extension`ではこの名前が変化するため、ZIP内の素材名は
文字列として拡張子を除去するように修正した。

合成素材の描画テストで、判定素材0〜5とゲージMAX用素材、POOR・空POORの回数参照、
HSの丸め・表示倍率優先、達成率の開始前値・切り捨て、ゼロ埋め、1〜3桁の整数部、
左・中央・右寄せ、DPの両側配置を確認した。実スキンのGPUプレビューにはHS 2.30と
EXスコア1234 / 処理済み700ノーツ = 88.14%を設定し、3種類とも数字・小数点・単位を確認した。
追加の回帰テストと10種類×8キー数の読み込みも成功。ログは
`.local/qwilight-hud-targeted.log`、画像は`.local/qwilight-hud-previews/`に置く（Git管理外）。
追記後もfmtとworkspace全体のcheck・all-targets Clippy（warning禁止）・testはすべて成功。
`bmz-player`は3,004件成功・70件ignored。全体ログは`.local/qwilight-hud-workspace.log`。

## 追記: スキンカスタマイズ（位置・サイズ、条件付き表示、言語別選択肢、レーンカバー）

基点は`d6e5a001`。Qwilight本体の`UIConfigure.cs`・`ComputingGUIConfigureWindow.xaml`・`LanguageUtility.cs`・
`PaintPipelineID.cs`・`ModifyVeilDrawingViewModel.cs`と、既存参照の`UI.cs`・`ComputedState.cs`・`DrawingSystem.cs`・
`DefaultCompute.cs`（いずれも`e9ef433d`）を確認した。参照コピーは`.local/qwilight-reference/`に置く（Git管理外）。

### 調査結果

- Qwilightは`mainPosition`等の調整値をComputedStateでLuaのグローバル変数へ渡す。設定画面の各スライダーは±1000 pxで、
  スキンの`setMainPosition`等が`true`のときだけ有効になる。レーン位置・ノーツ幅・高さ・ボム範囲はキー数別、
  判定線・コンボ・判定・判定ビジュアライザーは全キー数共通に保存される。判定線は`600 + 値`、V2は`1 + 値 / 600`、V3は値。
- 値の正は下方向。実スキン（Default / CR系 / QMAX）はPosition1へ加算して使う。CR_Maniaだけが
  `setMainPosition`・`setNoteLength`を`false`にしている。
- `configure-available-*`は読み込み時のLua（保存値の`configures`）で評価され、隠れた項目はプレイ時のLua
  （ComputedState）で先頭の選択肢として扱われる。素材名と`lambda`・`format`は読み込み時の値を使う。
  実スキンではCR_minimal・CR_natural・QMAX_Humanoid・QMAX_templateが、ノーツ色の項目をトグルで隠す。
- 言語別の選択肢は`configure-{i}-{LCID}`。QwilightのLCIDは日本語1041・韓国語1042・その他1033だけで、
  保存値は表示中の言語の文字列になる。実スキンではDefaultだけが韓国語の選択肢を持つ。
- Veil（レーンカバー）はNote（7）の直後のVeilDrawing（50）で、1Pレーン幅・縦横比維持・下端をカバー高さに合わせて描く。
  利用者が追加した画像を優先し、なければ`_GetVeil()`の`Drawing/`画像（`[a, b]`なら乱択）、なければ透明。
  カバー高さ・終了時に下ろす設定・プレイ中の開閉はQwilight本体の設定。CR_DX同梱の`Cover_DX.png`はZIP内の`Veil 0.png`と同一。

### 実装

- 調整値は既存のPlay枠オフセット（名前保存・履歴・リセット）として宣言し、読み込み時に`LuaLoadRuntimeState.offset_values`から
  Luaへ渡す。BMZのオフセットに合わせてyは上が正とし、Qwilightへは符号を反転する。IDは描画時offsetの範囲外の1001〜1008。
  変更はPlay枠の再読み込み要求となり、署名の差で再decodeされる。専用の保存先は追加していない。
- 共通オフセットは、変換後のnotes destinationへ30、判定画像とコンボ数へ32を付けて効かせる。
  対象要素がないJudge Detail・Bar Lineは、Qwilightのときだけ設定UIから外す。
- 設定UIの定義はQwilightに限りスロットのオプションでLuaを評価して作る。カタログ走査は従来どおりLuaを実行しない。
  定義cacheのkeyにオプションを含め、同じパスで16件を超えたら古い選択を捨てる。
  `lambda`・`format`・素材名と、`point`・`frame`・`alt-*`で`configures`を切り替える。
- 表示名は`SkinDocument.propertyLabels`（UI言語コード別の文字列）を追加して渡し、保存名は従来の項目名・既定の選択肢名のまま。
  中国語はQwilight同様に英語（1033、なければ指定なし）を表示する。
- レーンカバーはSUDDEN+のslider（type 4、range=レーン高さ）として描く。画像サイズはヘッダーだけ読んで縦横比を求める。
  BMZのオプション「Lane cover image」でSkin / Random / 追加画像を選び、追加画像は`<data>/skins/qwilight-veil/`へコピーする。
  Qwilightのカバー高さ・開閉はBMZのSUDDEN+の量と操作で代える。

### 検証

自動テスト（通常実行）:

- bmz-skin: 調整値のフラグ・符号・±1000制限とヘッダー宣言、`configure-available-*`の表示切替と
  素材名／`point`の値の切り分け、Luaエラー時の表示維持、言語別の表示名と個数不一致の除外、画像ヘッダー（PNG / JPEG / BMP / GIF / WebP）。
- bmz-player: 自作ZIPでのレーンカバー（スキン同梱・追加画像・削除済み画像のfallback、縦横比、SUDDEN+ 0 / 0.5の描画位置、
  ノーツ直後の描画順、DPの両側）、Notes / Judge offsetのID付与、保存した調整値のTOML往復→再読込署名の変化→
  7K / 14Kの配置、スロットのオプションによる設定項目の増減と共通offsetの整理、追加画像のコピー・同名回避・削除の範囲、表示言語のフォールバック。

実素材（`G:\SteamLibrary\steamapps\common\Qwilight\SavesDir (beta)\UI`、10種類）:

- 10種類×8キー数を既定値と、宣言された全調整を非既定値にした状態で読み込み、調整でレーンの位置・高さが変わることを確認。
  条件付き表示はCR_minimal 16中10、CR_natural 15中9、QMAX_Humanoid 16中10、QMAX_template 14中8を既定で表示。
  CR_Maniaはレーン位置・ノーツ幅を出さない。同梱レーンカバーはCR_Core以外の9種類で検出。
- GPUオフスクリーン（Windows）でCR_DX 7K・Default 14Kを、既定／調整値／SUDDEN+ 0.4／追加画像の4条件で描画。
  調整でレーン・判定線・コンボが移動し、カバーは上側だけに描かれ、追加画像（CR_naturalのCover_Red.pngを一時rootへコピー）が
  同梱画像と置き換わることを画素差と目視で確認した。Qwilightのインストール先には書き込んでいない。
  画像は`.local/qwilight-customize-previews/`に置く（Git管理外）。

```powershell
$env:BMZ_QWILIGHT_SKIN_ROOT='G:\SteamLibrary\steamapps\common\Qwilight\SavesDir (beta)\UI'
cargo test -p bmz-skin --locked installed_qwilight_play_skins -- --ignored --nocapture
$env:BMZ_QWILIGHT_PREVIEW_DIR=(Join-Path (Get-Location) '.local/qwilight-customize-previews')
cargo test -p bmz-player --lib --locked qwilight_installed -- --ignored --nocapture --test-threads=1
```

fmtとworkspace全体のcheck・all-targets Clippy（warning禁止）・test（全33バイナリ）が成功。
`bmz-player`は3,009件成功・71件ignored、`bmz-render`は1,016件、`bmz-skin`は420件成功。
その後の`bmz-player`の定義cache修正・文言修正の後に、fmt・`bmz-player`のall-targets Clippy・testを再実行して成功。
ログは`.local/qwilight-customize-workspace.log`と`.local/qwilight-customize-player-final.log`（Git管理外）。

実ウィンドウでの設定画面の操作（ファイル選択ダイアログによる画像追加を含む）、手動プレイ、実入力、
macOS / Linuxでの実行は未確認。全オプションの組合せと原作との画素一致も確認していない。
