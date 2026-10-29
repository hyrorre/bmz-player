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

## 残課題

- 9KEYS以外のキーモードで`dst2`を持つスキン（PeacefulPlayの7 to 9版を7KEYSで使う場合など）では、
  見逃しノーツの落下を描きません。snapshotがK9だけで過去ノーツを残すためです。
- PeacefulPlayの7 to 9版のKEY LOGGERは、BMZが7KEYSのレーン順（皿を含む）で列を数えます。
  スキンは表示レーン順で数えるため、既定以外のレーン割り当てでは別の列に表示されます。
- `peaceful_play_integral_property_ops_are_selectable_when_available`の`gauge-lead-glow-*`の期待値を、
  現在のPeacefulPlayの版に合わせて見直す必要があります。
