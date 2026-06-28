# CHANGELOG

## v0.1.5

### 改善

- プレイ中のスキン option 変更と live editing の反映を大幅に高速化しました。
  - decoded skin document / source / font / GPU texture を cache。
  - options-only の変更は可能な範囲で即時反映。
  - full reload 中も描画を止めず、切り替え時の固まりを軽減しました。
  - 同一画像・動画 first frame・font payload の再 decode / 再 upload を抑制しました。

- WMII FHD / LR2Skin / Lua skin のプレイスキン互換性を改善しました。
  - レーンカバー、LIFT、緑数字/白数字、score graph、target 表示、判定詳細、gauge 表示を beatoraja の挙動に近づけました。
  - LR2 `#SETOPTION` 由来 op、play key mode op、autoplay 中の score graph 表示を正しく扱うようにしました。
  - LR2 text の overflow shrink、bitmap font、TGA font page、w=0 text align などの描画差異を修正しました。

- BGA / chart asset の beatoraja 互換性を改善しました。
  - `#BMP` の同 stem 拡張子 fallback を追加。
  - GIF / TGA 静止画 BGA、低 bitrate 動画 stream、WebM などの扱いを改善。
  - 小さい静止画 BGA を beatoraja 互換の 256x256 padding で読み込むようにしました。
  - 動画 BGA の初回フレームを open 時に prime し、黒表示を減らしました。
  - 非表示の動画 BGA decoder を停止するようにしました。

- 文字描画品質を改善しました。
  - bitmap font の非整数倍率拡縮を bilinear 補間に変更。
  - vector font atlas を supersampling して、小サイズや非整数倍率でのジャギーを軽減しました。

- 音声の乱れを改善しました。
  - system sound の音量更新をまとめ、同値更新時の lock を回避。
  - play keysound の音量変更は pending queue 化し、AudioEngine が busy の場合は次フレーム以降に retry するようにしました。

### 修正

- リプレイ再生時の FAST/SLOW 表示条件を修正しました。
  - replay 中に profile の autoplay state が skin op へ漏れないようにしました。
  - replay でも Auto 表示の scope を正しく扱うようにしました。

- 選曲画面から単曲リプレイを開始した場合も decide 演出を経由するようにしました。

- Result 画面の fadeout skip と終了キーの扱いを調整しました。
  - 長い fadeout を入力でスキップ可能にしました。
  - Key2 / 2P Key2 を result exit 対象から外しました。
  - リプレイ後に選曲へ戻ったとき、押下状態が stale hold として残る問題を修正しました。

- Floating hispeed / HS-FIX / READY 前の表示を修正しました。
  - READY 前から基準 BPM と固定緑数字に基づく HS 表示を揃えました。
  - 曲開始前は HS-FIX 基準 BPM、開始後は現在 BPM を使うようにしました。

- EmptyPoor を LR2 / beatoraja 互換の poor / miss 系 skin ref に含めるようにしました。

- STAGEFILE / BANNER / BACKBMP などの chart meta image でも同 stem 画像拡張子 fallback を行うようにしました。

- スキン filepath 選択を basename ではなく skin root 相対 path で解決し、同名ファイルや LR2 `#CUSTOMFILE` の選択が正しく反映されるようにしました。

### テスト・開発環境

- WMII FHD / LR2Skin / BGA / skin reload cache / audio diagnostics まわりの回帰テストを追加・更新しました。
- スキン reload timing ログを通常時に邪魔にならないよう debug 出力へ変更しました。
- `AGENTS.md` の PowerShell 7 / utf8NoBOM commit message / worktree setup に関する作業メモを更新しました。

- `bmz-audio` / `bmz-player` に CPAL / ASIO 出力の診断ログを追加しました。
  - callback 時間、lock miss、clipping、stream error を計測。
  - lock miss を system / play / draining などの source 別に追跡可能にしました。

- BGA 互換確認用の最小 fixture `data/songs/bga-compat` を追加しました。
  - PNG / GIF / TGA / WebM / 拡張子 fallback / animated GIF の挙動をテストで固定しました。

- プレイ中スキン reload の計測を追加しました。
  - skin decode / source decode / GPU upload / main apply の時間をログ化。
  - cache hit / miss や skip 件数を確認できるようにしました。
