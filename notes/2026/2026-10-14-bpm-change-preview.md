# BPM変化予告の拡張refとantique実装

## 依頼と範囲

プレイスキンでBPM変化を予告表示するため、BMZ拡張のref / option / timerを追加する。
機能のON/OFFと予告開始時間はスキン側のオプションで決め、同梱fork `data/skins/mz-select` の
antiqueに実験的に実装した。改変許諾は [antique Ambient背景](2026-09-30-antique-ambient.md) で確認済み。

## 設計判断

- 本体は「次のBPM変化までの残り時間・変化前後のBPM」と「直前の変化からの経過時間」だけを
  snapshotへ載せる。予告開始時間はskin documentの `bmzBpmChangePreview.leadMs` で宣言し、
  rendererが予告option `19064` とtimer `19068` を計算する。
- 時間はユーザー指定により実時間（再生速度を反映）とした。譜面時間で進む他のプレイtimerとは
  1.0倍速以外で進み方が異なる。
- 変化時刻は `GameSession` と同じ `TimingMap::from_chart_timing_events` の区間から求め、
  `ref=160` の現在BPMが切り替わる時刻と一致させる。STOPと同じ値の再指定は除き、
  同一時刻の複数指定は最後の値で比べる。判定はBPM表示と同じレーン時刻（表示オフセット適用後）。
- 16分間隔などで連続して変わる譜面も、次の1回だけを対象にする。
  連続変化を区間にまとめる案は検討したが、ユーザー判断で採用しない。
- snapshotの値はms単位のi32とした。`RenderSnapshot`はapp側に複数inlineで保持され、
  `WinitApp`のサイズ上限テスト（64KiB未満）が基準commitで上限の直前にある。
  µs単位のi64では上限を80 bytes、ms単位でも48 bytes超えたため、決定画面の間だけ
  snapshot全体を持つ`pending_decide`を`Box`化した（`lr2_skin_settings`等と同じ扱い）。
- ID: number `19060..19063`、option `19064..19067`、timer `19068` / `19069`。
  仕様は [skin.md](../../docs/skin.md#bmz-bpm-change-preview-refs)。

## antique

- 「BPM変化予告 (BMZ)」: OFF（既定）／TYPE A（FAST/SLOWより上）／TYPE B（判定文字の下）／
  TYPE C（レーン外・判定文字の横）／TYPE D（レーン外・判定線の横）。
- 「BPM変化予告タイミング」: 500～4000ms、既定2000ms。OFFでは `bmzBpmChangePreview` を出力しない。
- 当初は「153 >> 156」を同梱フォントのtext 3要素で表示した（下の追記で変更）。
  上昇は青 (80,170,255)、下降は赤 (255,80,80)。timer 19068で120msのフェードイン。
- TYPE C/Dはレーン中心が画面左半分なら右側、右半分なら左側に置く。
  グラフ等に重なるため、レーン外だけ黒の下地 (`panel`, alpha 0xe0) を敷く。
  TYPE A/B/Cは判定文字と同じくlift・判定位置offset (3, 32)、TYPE Dはlift (3) に追従する。
- 初回描画でtextの `x` を左端として配置し、文字が左へずれた。beatoraja互換でtextの `x` は
  alignの基準点（右揃え=右端、中央=中心）なので、配置を修正した。

操作説明は [controls.md](../../docs/controls.md)。

## 検証

検証画像とログは `.local/performance/bpm-change-preview/` に保存（Git管理外）。

- 本体: 変化点抽出（STOP・同値・同一時刻の取り消し）、`TimingMap::bpm_at_time` との一致、
  実時間換算（50% / 200%）、snapshot構築と描画スレッドのprojectionでレーン時刻を使うことをテスト。
- renderer: 宣言の有無、`leadMs` 既定値とclamp、予告開始境界、timer値、変化なし時の非表示、
  Lua `main_state.number/option/timer` をテスト。
- antique: OFF既定、TYPE A～D × 予告時間3種の宣言・色・配置、右寄りレイアウト3種の左側配置をテスト。
- GPU（ignoredテストを明示実行）: 実際のantique素材・フォントで6条件を描画し、
  予告前は色付き文字がなく、上昇は青・下降は赤の文字が配置範囲に描かれることを確認。画像も目視確認。

## 2026-10-14 追記: 画像数字・»・レーン側への寄せ

ユーザー指定により、antiqueの表示を次のとおり変更した。本体の実行コードは変更していない。

- 数字をFAST/SLOW数と同じ `image/number_common_s.png` の画像数字に変更した。
  上昇はFAST行 (y=26、(204,204,255))、下降はSLOW行 (y=39、(255,204,204)) を2倍 (1桁26×26px) で使う。
  10分割の数字部分だけを使い、上位桁は空白にする。
- 矢印を「»」に変更した。フォントのtextで、色は各数字行に合わせる。
- TYPE C/Dでは、レーン側の数字を左詰め（レーン右側）／右詰め（レーン左側）にしてプレイ枠の8px外に寄せる。
  「»」と反対側の数字は、レーン側の数字の桁数 (1～4) ごとに `draw: "number(...) >= 100 && ..."` で
  位置を切り替える。TYPE A/Bは「»」をレーン中央に固定する。
- 下地はレーン側から4桁+4桁分の固定幅とした。

検証:

- 読込テストで数字画像の行・align・桁数別drawの位置、右寄りレイアウトでの右詰めを確認。
- GPUテストで、レーン右側では左端、レーン左側では右端の色付き画素がプレイ枠から8px以内にあることを
  2桁・3桁・4桁で確認。画像は `.local/performance/bpm-change-preview/v2/` に保存し目視確認した。