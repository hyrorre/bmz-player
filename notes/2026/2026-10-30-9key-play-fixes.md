# 9KEYSの選曲カーソルとPeacefulPlay表示の修正

## 背景と修正範囲

`96ac57dd`を基点に、9KEYSで報告された4件の不具合を調べて修正しました。
PeacefulPlay（beatoraja 9key用、ver 1.3.1、`data/skins/PeacefulPlay/`）は改変・再配布不可のため、
スキンは編集せず、beatoraja（`.local/beatoraja/`）の挙動を基準にBMZ側で直しました。
現在の仕様は[skin.md](../../docs/skin.md#playの見逃しノーツと判定領域)と
[skin-compatibility.md](../../docs/skin-compatibility.md)を参照してください。

| 症状 | 原因 | commit |
|---|---|---|
| 9KEYSコントローラーで選曲カーソルが止まらない | ゲームパッドのrelease経路が長押し移動を解除しない | `06381683` |
| 判定文字が左と中央にしか出ない | 判定領域を全体レーン番号と16レーンで計算 | `7a46939f` |
| KEY LOGGERのノートが出ない | 押下時刻と経過時間の時計が違う | `4cf11f39` |
| 見逃しノーツの消え方が違う | PMS見逃しPOORの開始時刻・落下式・消滅条件の差 | `15838fbc` |

## 選曲カーソルの長押し移動

v0.5.0の9K選曲操作（`01ee28f0`）で、KEY4/KEY6とKEY1/KEY9の押下が長押し移動を始めるようになりました。
解除は一致するreleaseイベントでしか行わず、キーボード経路は解除しますが、
ゲームパッドのrelease分岐（`crates/bmz-player/src/app/select_flow/gamepad.rs`）は設定フォルダ内でしか解除していませんでした。
7KEYSは既定のアナログ皿が長押し経路を通らないため表面化していませんでした。
デジタル皿の7KとG-BATTLE一覧のコントローラー操作でも同じ問題がありました。

beatorajaの`BarRenderer`は毎フレーム`isNonAnalogPressed`で現在のキー状態を読みます。
BMZもstart/E2のholdと同じく物理状態を正とし、入力イベントごとのhold同期と毎フレームのリピートで、
開始した入力がどのデバイスでも押されていなければ止めるようにしました。
bounce filter・modal・プロファイル切替の早期returnで落ちたreleaseでも止まります。
ゲームパッドの押下snapshotには合成アナログ軸のボタンも含まれるため、デジタル皿のholdは誤って止まりません。

## 判定領域

PeacefulPlayの9key版は判定を3つ（index 0/1/2、timer 46/47/247）持ちます。
beatorajaの`JudgeManager.updateMicro`は`lane / (lanelength / judgenow.length)`で領域を選び、
9KEYSはKEY1-3が左、KEY4-6が中央、KEY7-9が右です。
BMZは全体のレーン番号（Key1..Key9 = 1..9）と`LANE_COUNT`(16)から`idx*3/16`を計算していたため、
KEY1-5が左、KEY6-9が中央になり、領域2は点きませんでした。
14K/10Kは左右が8で分かれるため偶然一致していました。

`lane_judge_region`をキーモード対応にし、beatorajaのレーン順（`beatoraja_note_index`）と
キーモードのレーン数で計算します。範囲外になる組み合わせは最後の領域へ寄せます。

## KEY LOGGER

KEY LOGGERのノートは、decode時にLua関数timerを`bmz:keylogger_event:{lane}:{slot}`へ置き換えて描きます。
`KeyLoggerRuntime`は押下時刻をchart timeで保存し、経過時間はplay scene開始からの経過時間との差で求めていました。
2つの時計はREADY前の待ち（PeacefulPlayの`loadend`/`playstart`で3秒以上）だけずれるため、
押した瞬間にアニメーションの終端（最大1300ms）を越えていました。
既存テストは両方の引数に同じ時計の値を渡していたため、この不一致を検出できませんでした。
押下時刻をµsで保持し、chattering警告と同じく`ingest`のchart timeとの差で求めるようにしました。

## PMSの見逃しノーツ

beatorajaの`LaneRenderer`は、skinの`dst2`がある場合だけPMS見逃しPOORを描きます。

- 判定時刻を過ぎた未判定・BAD・POORのノーツは、STOP終了＋`|MS late|`（PMS 175ms）まで判定ライン上に留まります。
- その後はハイスピードによらず、1小節で判定ラインからレーン上端までの長さ（`hu - hl`）ぶん落ちます。
  落下中のSTOPでは止まります。
- `dst2`を`-region.height`〜判定ラインに収め、ノーツがそれより下へ出たら描きません。
- 落下中は`markprocessednote`によらず通常画像です。`dst2`があると、早押しのPG/GR/GDはその場で消えます。

PeacefulPlayは`dst2`をレーン下端（判定ライン）に置くため、ノーツは判定ライン上で約175ms止まり、落下を始めた時点で消えます。
修正前のBMZは、判定時刻でいったん消え、POOR確定（183ms）後に判定ラインへ戻り、
「1小節で判定ラインからdst2へ線形移動」する式（PeacefulPlayでは移動量0）で1小節止まって見えていました。

`VisibleNote::pms_miss_fall`に落下開始からの小節数を持たせ、snapshotではK9の過去Tapを`note_retention`によらず残します。
消える位置はrendererが`dst2`で判断し、snapshotでは8小節を越えたノーツだけを除きます。
LR2横スクロールの落下先Xも、1小節で判定位置から右端までの長さぶん進め、落下先を越えたら消すようにしました。
この横方向の規則はBMZの補完動作です。

## 検証

- 各commitで`cargo fmt --check`と対象crateの`clippy --all-targets --locked -- -D warnings`に成功しました。
- `cargo test -p bmz-render --locked`は971件成功（全修正後）。
- `cargo test -p bmz-player --locked`は2,734件成功、1件失敗でした。失敗した
  `viewer_ipc::tests::play_stop_and_quit_requests_round_trip_to_persistent_listener`は2秒の受信待ちが
  全体実行の負荷で超えたもので、単独実行では成功しました。修正前の全体実行でも成功しています。
- Windows専用の`key9_gamepad_release_stops_select_hold_move`（ignored）は、修正前はrelease後も
  `Some(Previous)`のまま失敗し、修正後に成功しました。bounce 20msとG-BATTLE一覧も確認しています。

  ```powershell
  cargo test -p bmz-player --locked --lib key9_gamepad_release_stops_select_hold_move -- --ignored
  ```

- `cargo test -p bmz-skin --locked`では、PeacefulPlayの`dst2`が9レーンの下端と一致することを確認しました。
  同じテスト後半の`gauge-lead-glow-*`件数（216件）は、ローカルのPeacefulPlay 1.3.1に該当objectが無く、
  修正前のHEADでも失敗します。今回の修正とは別の、スキン版の差によるテスト期待値のずれです。
- 全修正後に`cargo check --workspace --locked`と
  `cargo clippy --workspace --all-targets --locked -- -D warnings`に成功しました。
  `cargo test --workspace --locked --no-fail-fast`の失敗は、上記のPeacefulPlay `gauge-lead-glow-*`の1件だけでした
  （bmz-player 2,735件、bmz-render 971件成功）。ログは`.local/9key-fixes/`に置き、共有しません。
- PeacefulPlayを使った実プレイの目視確認と、9KEYSコントローラー実機での確認は未実施です。

## 追記: 7 to 9版の見逃しノーツとKEY LOGGER列（2026-10-30）

上の修正で残した、PeacefulPlayの7 to 9版（`play7_9lane.luaskin`、7KEYS譜面を9レーン外観で遊ぶ版）の2件を直しました。

### 見逃しノーツ（`1b141063`）

beatorajaのPMS見逃しPOOR描画は、キーモードではなくskinの`dst2`で有効になります。
BMZのsnapshotは9KEYSのときだけ過去ノーツを残していたため、7 to 9版では落下を描いていませんでした。
`GameSession::missed_note_fall`にプレイスキンが`note.dst2`を持つかを保持し、
9KEYSまたはこのフラグがあるときに過去の未判定・BAD・POORのTapを残すようにしました。
フラグは`install_active_play`（通常・コース・Practiceの開始）と動画出力で、読み込んだプレイスキンから設定します。

### KEY LOGGER列

7 to 9版は、レーン割り当て（`割り当て assign`）に応じて皿と鍵盤を表示列へ並べます。
スキンのtrackerは表示列ごとに`main_state.timer(100 + skin offset)`を読みますが、
BMZは7KEYSのレーン順（皿が先頭）で列を数えていました。既定の「SC1 KEY2-8」だけが偶然一致し、
他の5種類では別の列に表示されていました。

`keylogger-button-on-<列>`のdraw callbackは、その列のkey-on timerだけを読みます。
ロード時にこのcallbackが読むtimerを記録し、documentの`keyloggerLane`として保持します。
`KeyLoggerRuntime`はこの対応で入力レーンを列へ割り当て、対応が無いスキンでは従来のレーン順を使います。
実スキンから求めた対応は、9key版が列N→timer 100+N、7 to 9版の「SC1 KEY3-9」が
列1→100（皿）・列3〜9→101〜107、「SC9 KEY1-7」が列1〜7→101〜107・列9→100で、
スキンの`lane_map`と一致しました。

### 追記分の検証

- `cargo check --workspace --locked`と`cargo clippy --workspace --all-targets --locked -- -D warnings`に成功しました。
- `cargo test --workspace --locked --no-fail-fast`の失敗は、既存の`gauge-lead-glow-*`の1件だけでした
  （bmz-player 2,736件、bmz-render 972件、bmz-skin 340件、bmz-gameplay 255件成功）。
- 追加したテスト:
  - 7KEYSで`missed_note_fall`があるときだけ見逃しノーツを残して落とす（snapshot）。
  - 「SC1 KEY3-9」の対応で、皿・KEY1・KEY7の押下と判定数が列1・3・9に入る（runtime）。
  - 実スキンの9key版と7 to 9版3種類で、`dst2`と列の対応を確認する（decode）。
- 7 to 9版を使った実プレイの目視確認は未実施です。

### 追記時点の残課題

- `peaceful_play_integral_property_ops_are_selectable_when_available`の`gauge-lead-glow-*`の期待値を、
  現在のPeacefulPlayの版に合わせて見直す必要があります。
  このテストは途中で失敗するため、7 to 9版の確認は別のテストに分けました。

## 追記: コミットレビューと現行ゲージのテスト（2026-10-30）

`96ac57dd..3b0441d9` の8コミットをレビューしました。
今回変更したのはゲージのテスト期待値だけで、次の2点は未修正のレビュー指摘です。

- 内蔵9KEYスキンは `pms_miss_fall` を扱わず、負の `note.y` を判定ラインへ丸めます。
  snapshotの保持上限が1小節から8小節へ延びたため、見逃しノーツが落下せず、
  BPM120では約16秒残ります。0秒のPOORを0.675秒と4.175秒で描画すると、
  落下量が0.25と2.0でも、矩形はどちらも `y=0.806, height=0.018` でした。
  内蔵描画にも落下・消滅の処理が必要です。
- `missed_note_fall` のある7KEYSスキンでは、`visible_lane_notes` の下限がなくなり、
  既に表示期間を過ぎたBAD/POORも毎フレーム再走査します。
  各ノーツでtiming segment全体の走査と落下時間の積分を行い、8小節の打ち切りもその後です。
  表示期間の下限による候補の絞り込みと、積分結果の再利用が必要です。
  合成7KEYS譜面（10,000 Tap、500回のBPM変更、10%がPOOR、cache再利用、各10回）の
  debugビルドで、snapshot作成の中央値は曲10%地点で0.592ms、90%地点で25.534msでした。
  フラグOFFではそれぞれ0.020ms、0.061msです。これはCPU処理時間の確認であり、
  releaseビルドの実プレイFPSやGPU/present待ちを測った値ではありません。
  調査用の一時テストは削除しました。

現行PeacefulPlayの `play9main.lua` は、旧版の `gauge-lead-glow-*` destinationではなく、
`gauge-main` の36枚のnode画像で先端の発光を表します。
古い216件の期待値と7 to 9版の旧destinationへの期待値を削除し、独立したテストで次を確認します。

- 9key版・7 to 9版ともに24粒、36枚のnode画像、1500ms周期を保持する。
- 発光ONは `type=3` と発光画像、OFFは `type=1, range=0` と通常画像を使う。
- ゲージ種別ごとの画像切り出しと、ボーダー上下の先端画像が正しく解決される。
- `gauge-main` のdestinationは1つで、旧版の `gauge-lead-glow-*` は生成されない。

`cargo fmt --check`、`cargo check -p bmz-skin --locked`、
`cargo clippy -p bmz-skin --all-targets --locked -- -D warnings` に成功しました。
`cargo test -p bmz-skin --locked` は342件成功、1件ignoredです。
ローカルのPeacefulPlay 1.3.1を実際に読み込んで確認しており、素材未配置によるskipではありません。
`cargo test --workspace --locked --no-fail-fast --quiet` は、bmz-playerのリンク作成テストで
Windowsの権限エラーが発生し、bmz-playerのテストプロセスも終了コード3で途中終了しました。
ほかのworkspace targetは成功しました（bmz-render 972件、bmz-gameplay 255件成功）。
必要な実行権限で `cargo test -p bmz-player --locked --lib --quiet` を再実行し、
2,736件成功、45件ignoredを確認しました。全体をまとめた再実行はしていません。
Windows専用の `key9_gamepad_release_stops_select_hold_move` も `--ignored` で単独実行し、成功しました。
ログは `.local/peaceful-review-tests.log` と `.local/peaceful-review-player-tests.log` に置き、共有しません。
実プレイの目視、物理コントローラー、macOS/Linuxでの確認は未実施です。
