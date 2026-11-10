# 譜面取得の保存先をIPFS / HTTPで別々に指定する

2026-11-10に、譜面取得の保存先を方式ごとに指定できるようにした記録です。
現在の仕様は[譜面ダウンロード](../../docs/chart-downloads.md#保存先フォルダ)を参照してください。

## 変更前

- 保存先は`downloads.save_dir`の1つだけで、実際の保存先はその下の`http/` / `ipfs/`に固定されていた。
- IPFSとHTTPを別のドライブやフォルダへ分けられず、サブフォルダ名も変えられなかった。

## 変更内容

- `downloads.ipfs_save_dir` / `downloads.http_save_dir`を追加し、指定したフォルダの直下へ保存する。
  空欄では`data/songs/ipfs` / `data/songs/http`で、既定の保存先は変更前と同じ。
- 取得要求（`ChartDownloadRequest.save_dir`）はキュー投入時に方式別の保存先を解決して持つ。
  待機中・再試行のタスクが投入時の保存先を使う方針は変えていない。
- 旧`save_dir`は読み込みだけ受け付け、`<save_dir>/ipfs` / `<save_dir>/http`へ移す。
  保存時は書き出さないため、旧版へ戻すと保存先は既定値に戻る。取得済みの譜面と曲フォルダの登録には影響しない。
