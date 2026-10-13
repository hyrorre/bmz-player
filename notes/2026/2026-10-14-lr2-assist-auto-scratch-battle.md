# LR2選曲のASSIST（AUTO SCRATCH）とBATTLE

LR2選曲スキンのASSIST（button 44 / 45）とBATTLE（button 56）をBMZの設定へ接続した。
ASSISTのためにAUTO SCRATCHをプロフィール設定として新設した。

## 参照した仕様

- LR2（`.local/OpenLR2`）
  - `config.play.battle` は 0 OFF / 1 BATTLE / 2 D-BATTLE / 3 SP TO DP / 4 G-BATTLE。
  - 表記は `LR2files/Config/optionstr.csv` の `#BATTLE,OFF,BATTLE,D-BATTLE,SP-TO-DP,G-BATTLE,...` と、
    `#ASSIST,OFF,AUTOSCR.`。
  - AUTO SCRATCH（`p1_assist`）はスコアを保存せず、クリアをイージー扱いにする（`isForceEasy`）。
    ただし `battle == 2`（D-BATTLE）のSP譜面ではイージー扱いにしない（`LR2_bmsload.cpp`）。
  - D-BATTLEはスクラッチを自動演奏する。SP TO DPはスコアを通常保存する。
  - 選曲のKey3は、キーフィルターがDOUBLE系ならFLIP、それ以外はBATTLEを切り替える（`Scene02_Songselect.cpp`）。
  - option 10は「DPフィルター、またはbattle 2 / 3」、12は「DPフィルター、またはbattle 1..3」。
- LR2oraja Endless Dream（`.local/lr2oraja-endlessdream`、`BMSPlayer.java`）
  - AUTO SCRATCHは全譜面オプションの適用後に掛ける。配置でスクラッチへ移ったノーツも対象にするため。
  - 通常プレイだけが対象で、リプレイ再生とゴーストバトルは対象外。皿のないモードは対象外。
  - スクラッチノーツが無い譜面ではアシスト扱いにしない。それ以外はASSIST EASY・スコア保存なし。

## 決めた適用ルール

- AUTO SCRATCH（`play.assist.auto_scratch`）
  - F1のASSIST一覧、プロフィール画面、LR2 button 44 / 45から切り替える。1P / 2Pの区別は持たない。
  - 実効アシスト `Assist`（clear-only、リプレイ・IRなし）。自分側のスクラッチレーンに採点対象ノーツが
    ある場合だけ。リプレイ再生では自動演奏しない。
  - DP OPTIONのBATTLEと併用すると、プレイ開始時にBATTLE ASとして扱う（専用bucketへ通常保存）。
    切り替え時もBATTLE ↔ BATTLE ASを連動させる。
  - 旧profileの `assist = "AutoScratch"` は `auto_scratch = true` として読み込む（以前は無視していた）。
- BATTLE
  - BMZに単独の値は持たず、SESSION MODE（G-BATTLE）> DP OPTION（BATTLE系 → D-BATTLE）>
    KEY CONVERSION（SP TO DP）の順で表示値を決める。この優先順は、バトルがKEY CONVERSIONより
    優先されるプレイ開始時の扱いと同じ。
  - 選ぶと他の値に当たる設定を解除する。PRACTICE / AUTOPLAY / FLIP / 7K TO 9K・6KはLR2に無い値なので、
    置き換える場合を除いて維持する。
  - LR2の2人対戦BATTLE（値1）は選べない。
  - スコア保存はBMZの既存規則のまま（D-BATTLEは専用bucket、SP-TO-DPは保存しない、G-BATTLEは通常bucket）。
    SP-TO-DPはLR2では通常保存するが、譜面が変わるためBMZの扱いを維持した。
  - 譜面のキー数で使えない値は、表示は設定値のままにし、開始時に無効化する（従来どおり）。

## 描画側

- `SelectSnapshot` に `auto_scratch` と `key_mode_conversion` を追加し、`SkinDrawState` の
  `select_auto_scratch` / `lr2_select_battle_index` へ渡す。
- option 54..57を固定値から外し、選曲では設定値、プレイでは実際に自動演奏するスクラッチレーン
  （`auto_note_lanes`）で判定する。
- text 67 / 68（ASSIST）と69（BATTLE）を追加。D-BATTLE中はFLIP（button 54 / text 70）をOFFと表示する。
- option 62..66（このまま取れる最高ランプ）は、AUTO SCRATCHをD-BATTLE以外でEASY、SP-TO-DPを0とする。

## 検証

- `cargo fmt --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、
  `cargo test --workspace --locked --no-fail-fast` が成功。
- 追加したテスト: 設定の連動と取り消し、旧profileの読み込み、譜面変換後のアシスト判定（スクラッチ有無、
  BATTLE併用）、自動演奏レーン、LR2 BATTLEの読み書き、LR2のbutton / option / text / ランプ判定。
- 実機でのLR2スキン操作・プレイは未確認。
- 途中で `bmz-render` の変更がcargoに検出されず、古いビルドのまま `bmz-player` が型エラーになった。
  変更ファイルの更新時刻を更新して再ビルドすると解消した（コード側の問題ではない）。
