# BMS `#TEXT` / `#SONG` / `#TEXT00` のスキン表示

実施日: 2026-10-16。ブランチ `feat/chart-text-skin` の3コミットで実装した。
現在の仕様は [skin.md のBMZ Chart Text Refs](../../docs/skin.md#bmz-chart-text-refs) を参照する。

## 調査結果

- 参照資料: [BMS command memo](https://hitkey.nekokan.dyndns.info/cmdsJP.htm#TEXTXX)。
  `#SONG` は `#TEXT` の旧式で、同じIDでは `#TEXT` が優先される。`#TEXT00` はnanasiでミス時に
  表示されるが、対象判定と表示時間の記載は無い。空文字の定義はその行を消した場合と同じ
  結果になるとされるため、未定義IDの参照も空文字イベントとした。
- beatoraja（`.local/beatoraja`）と jbms-parser は `#TEXT` / チャンネル99を扱わない。
  互換仕様は無く、ID・名前・timerはBMZ独自拡張として `20100..20199` に置いた。
  beatorajaのcustom timer `10000..19999`、BMZの `19000` 番台、LR2 text bridge `95000..` とは重ならない。
- bms-rs 1.0の `TextProcessor` は `#TEXT` と `#SONG` を同じ表へ後勝ちで入れ、
  `text_events` を時刻キーのmapで保持する。また未定義IDで `?` により行の残りを処理しない。
  優先順位、同時刻イベント、未定義IDの扱いを満たせないため、BGAチャンネルと同じく
  `#RANDOM` 解決後の原文から定義とメッセージを読む方式にした
  （`crates/bmz-chart/src/import/bms_rs_adapter/text.rs`）。入力bytesと譜面ハッシュは変えない。
- 既存の `chart_text_at_time()` は線形走査だったため二分探索に変更し、直近イベントを返す
  `chart_text_event_at_time()` を追加した。`RenderSnapshot` は毎フレーム複製されるため、
  `ChartTextEvent.text` と `RenderSnapshot.chart_text` を `Arc<str>` にして文字列複製を避けた。
- 表示状態はgameplayスレッドのsnapshot構築と、window側の `PlayfieldProjection::project` の
  両方で譜面時刻から求める。gameplay側で追加した処理は判定時の直近ミス時刻の更新だけ。

## 判断

- 共通方針の「スキンに実装が無い場合は左上の汎用メッセージ」と、「スキン定義が無ければ
  自動表示しない」「`play_plan_does_not_inject_bms_text` を維持」は同時に満たせない。
  当初は既定OFFのプロフィール設定 `play.chart_text_fallback` で両立させたが、同日の指示で
  設定を削除し常に有効にした。スキンの描画計画には追加せず、左上のオーバーレイ欄へ出す。
  参照の有無はtext部品の `ref` / `value` の静的確認で、Luaの関数内の参照は判別できない。
- `#TEXT00` のミス表示はBMZ暫定ポリシー: スコアに数えるPOOR・空POOR、表示時間は
  ミスレイヤー表示時間。BADは「ミス」とするか資料で判断できないため含めなかった。
- 20100は最終的に表示する文字列（ミス表示中は `#TEXT00`）、timer 20102はチャンネル99だけで
  再始動する。ミス表示用にoption 20104とtimer 20105を別に追加した。

## 検証

自動テストは各コミットの本文を参照。実機（GPU描画・実際のBMS）での表示確認は未実施。
