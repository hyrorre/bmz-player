# E2+鍵盤による緑数字の操作の向き

実施日: 2026-11-11

## 背景

beatorajaのバージョンによって、プレイ中のSELECT（BMZのE2）+ 鍵盤で緑数字（duration）を上げる鍵盤と下げる鍵盤が逆になっている、という報告があった。
参照ソース `.local/beatoraja`（exch-bms2/beatoraja master、`83574048`、VERSION `beatoraja 0.8.9`）で事実を確認し、BMZで向きを選べるようにした。

## beatorajaでの変化

`src/bms/player/beatoraja/play/ControlInputProcessor.java` を比較した。

| 時点 | SELECT + 鍵盤（7K） | START + 鍵盤（HS） | SELECT + 皿 |
| --- | --- | --- | --- |
| `ec3d3169`（2023-11-12、VERSIONを0.8.7へ更新） | 白鍵（1/3/5/7）でduration +1、黒鍵（2/4/6）で -1 | 白鍵で下げる、黒鍵で上げる | 上で +、下で - |
| `c98b9e85`（2024-08-20）以降、現在の0.8.9 | 白鍵で -1、黒鍵で +1 | 変わらない | 変わらない |

- 0.8.7では、キーモードごとに `processSelect7key` 等の関数があり、SELECT時はSTART時と逆の鍵盤の組で増減していた。
- `5c77038a` / `c98b9e85`（どちらも2024-08-20、"ControlInputProcessor : refactor"）で、
  キーモードごとの関数を、START時と共通の `keybinds` 配列（白鍵 `-1`、黒鍵 `1`）にまとめた。
  SELECT時も同じ配列で `setDuration(getDuration() ∓ 1)` するため、SELECT + 鍵盤の向きだけが逆になった。
  他の5K / 9K / 24Kのキーモードも同じ配列で処理する。
- `8852eb5b`（2026-06-11）で、VERSIONは0.8.7から0.8.9へ変わった。履歴に0.8.8のVERSION文字列とタグはなく、Web検索でもリリース日を確認できなかった。
  そのため、0.8.8がどちらの向きかはソースからは判断できない。2024-08-20以降のmasterから作ったビルドなら0.8.9と同じ向きになる。
- 選曲画面のSTART + SELECT + 鍵盤（KEY4 = `DURATION_DOWN`、KEY6 = `DURATION_UP`、`MusicSelectKeyProperty`）は、2017年から変わっていない。

## BMZの対応

- 既存の動作（E1と同じキーモードごとの方向、7Kの白鍵で緑数字を下げる）は0.8.9と同じ。既定はこのままにした。
- プロファイルの `input.invert_e2_key_green_number` をONにすると、`play_option_control_for_input` でE2 hold + 鍵盤の増減だけを入れ替える。
  8Kで鍵盤ごとに変更した方向も、その方向を入れ替える。beatorajaで変わっていないE1 hold + 鍵盤、E2 hold + 皿、9KのKEY8 / KEY9のカバー操作、選曲画面の詳細パネルは対象外。
- 設定の場所は、eguiの「プロファイル → 表示」の「HS変更時のアニメーション時間」の下と、選曲画面の「設定 → 表示」の `CONSTANT FADE` の下。
  選曲画面にはHS変更時のアニメーション時間が無かったため、同時に `HS ANIMATION` を追加した。

仕様は [controls.md](../../docs/controls.md#レーンカバーとhs) と [settings.md](../../docs/settings.md#プレイ) に記載した。

## 未確認

- beatoraja 0.8.8の配布物での実際の向き。
- eguiと選曲画面の設定の見た目、実機のプレイ中の操作（自動テストのみで確認）。
