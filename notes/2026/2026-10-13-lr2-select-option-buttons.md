# LR2選曲スキンのオプションbuttonを直接切り替えにする

2026-10-13に、LR2 CSV選曲スキン（type 5）のオプションbuttonをLR2本来の挙動へ寄せた。
基点は `a14d691a`。現在の仕様は [スキン仕様](../../docs/skin.md#lr2のselect--result) と
[操作](../../docs/controls.md#lr2選曲スキン) を参照する。

## 背景

従来はLR2 buttonの現在値がLR2の候補に無い場合（EX-HARD、R-RANDOM等）や、
BMZに同じ意味の設定が無いbutton（FX、AUTO SCRATCH、BATTLE等）を押した場合に、
egui の詳細設定画面を開いていた。プレイ中の選曲操作でウィンドウが割り込むため、
LR2にある設定はbuttonで直接切り替え、BMZ独自の設定はBMZ設定画面からだけ変更する方針にした。

## LR2の挙動の確認

`.local/OpenLR2/LR2/LR2_skinobject.cpp` の `SetObjectValue_Button` と `ButtonByInput` を確認した。

- 値の変更は左クリックだけ。`SRC_BUTTON` のop4が1なら＋のみ、2なら−のみ、0なら左右半分で増減する。
- 範囲外は `LoopInRange` 相当で反対端へ回る。
- 10 / 11 / 12（難易度・キー・ソート）はbutton側では値を変えず、
  `Scene02_Songselect.cpp` のフィルター処理で常に次へ進む。
- 各値の並びはOpenLR2の文字列設定・play処理と、既存の `lr2_scene::button_index` の対応から決めた。
  EFFECTは `Scene04_Play.cpp` の可視範囲から OFF / HIDDEN / SUDDEN / HID+SUD と確認した。
  TARGETは `SetTarget` の NO TARGET / MY BEST / AAA / AA / A / % / IR TOP / IR NEXT / IR AVERAGE。

## 実装

- `crates/bmz-player/src/app/select_support/lr2_buttons.rs` に、button IDから操作への対応、
  LR2順の候補、方向の決定、BMZ独自値からの入り方を純粋関数として分離した。
- `select_flow/lr2.rs` はこの対応に従って値を切り替える。BMZ設定画面を開く分岐を削除した。
  BMZ独自の現在値は、次へなら候補の先頭、前へなら末尾へ移す。
- `select_flow/pointer.rs` でLR2 button（内部ID 90000..90999）は左クリックだけを受け付ける。
  beatoraja互換のbuttonは従来どおり右クリックで逆方向に動く。
- 表示側は `bmz-render` の `lr2_scene::button_index` に 50 / 75 / 77 を追加し、
  `bmz-skin-document` の `lr2_arrange_index` に H-RANDOM / ALL-SCR（LR2の4 / 5）を追加した。
  77用に `SkinDrawState::lr2_select_target_index` を追加した。50 / 75 / 77はResultでは表示しない。
- E1保持中のKey6（LR2のAUTO SCRATCH）は、BMZ設定画面を開かず何もしない。

| button | 設定 | 切り替え |
|---|---|---|
| 1 | 基本オプションパネル | 固定表示の開閉（従来どおり） |
| 10 / 91..96 | 難易度フィルター | 10は常に次へ。91..96は直接選択 |
| 11 | キーフィルター | ALL / 7K / 5K / 14K / 10K / 9K（常に次へ） |
| 12 | ソート | LEVEL / TITLE / CLEAR / SCORE（常に次へ） |
| 13..17 / 19 | キーコンフィグ・スキン設定・プレイ・オート・README・リプレイ | 従来どおり |
| 40 | ゲージ | NORMAL / HARD / HAZARD / EASY |
| 42 / 43 | 1P / 2P配置 | NORMAL / MIRROR / RANDOM / S-RANDOM / H-RANDOM / ALL-SCR |
| 50 | EFFECT | OFF / HIDDEN / SUDDEN / HID+SUD |
| 54 | DP FLIP | OFF / FLIP |
| 55 | HS-FIX | OFF / MAX / MIN / MAIN |
| 57 | ハイスピード | 押した側で上下 |
| 72 | BGA | OFF / ON / AUTOPLAY |
| 74 | 判定タイミング | ±1ms |
| 75 | 判定タイミング自動調整 | OFF / ON |
| 77 | TARGET | NO TARGET / AAA / AA / A / IR TOP / IR NEXT |

BMZに同じ意味の設定が無いため、buttonでは変更しないもの:
パネル2..8、FX/EQ/PITCH（20..33）、2Pゲージ（41）、AUTO SCRATCH（44 / 45）、shutter（46）、
2P EFFECT（51）、BATTLE（56）、2P HS（58）、SCOREGRAPH（70）、GHOST（71）、BGA SIZE（73）、
既定ターゲット率（76）、画面・色・VSYNC・リプレイ保存（80..83）、favorite tag（90）、TAG RESET（18）。
LR2のP-ATTACK / G-ATTACKゲージ、HS-FIXの3 / 4、TARGETのMY BEST / % / IR AVERAGEもBMZに無いため候補から外した。
BMZ独自の値（A-EASY、EX-HARD、R-RANDOM、SPIRAL、START BPM、BATTLE、RANK_NEXT等）はBMZ設定画面で選ぶ。

## 検証

- 自動テスト: `app::tests::select::lr2_buttons` にbutton対応・方向・各候補の循環とBMZ独自値からの入り方、
  `bmz-render` の `lr2_select_option_buttons_show_cycled_profile_values` に表示indexを追加した。
- `cargo fmt --check`、`cargo check --workspace --locked`、
  `cargo clippy --workspace --all-targets --locked -- -D warnings` は成功。
- `cargo test --workspace --locked --no-fail-fast` は、未初期化のsubmodule（mz-select / Luxez-Flat）を要求する
  `skin_loader::tests::{detail_options, luxe_detail_options, mz_detail_options}` の14件だけが `path.exists()` で失敗した。
  今回の変更とは無関係で、それ以外は成功。
- 実機確認は未実施。外部LR2選曲スキンでのクリック位置・画像frameの表示、Key6の無反応は手動で確認する必要がある。
