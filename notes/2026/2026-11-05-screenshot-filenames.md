# スクリーンショットに曲名・クリアタイプ・フォルダ名を付ける

## 要望と仕様

- 日時だけだった手動スクリーンショットへ、撮影画面の情報を付ける。
- Resultは曲名・Subtitle・今回のクリアタイプ、コース総合Resultはコース名を使う。
- Play / Decideは対象曲名・Subtitle。コースDecideもステージの曲名を使う。
- Selectは現在開いているフォルダの表示名。ルートでは省略する。
- クリアタイプは既存の`ClearType::as_str()`ラベルを利用する。
- 詳細な命名規則は[操作方法](../../docs/controls.md#スクリーンショットのファイル名)に集約する。

## 実装

- appの`screenshot`へ要求・命名・rendererへの受け渡しを集約した。
  入力時に撮影日時と保存設定を保持し、描画直前にそのフレームのscene snapshotから名前を作る。
  コースDecideだけはコース名へ上書きする前のsnapshotから曲名を取得する。
- GPU readback / PNG保存待ちの名前も予約し、既存ファイルと合わせて連番で回避する。
  240 UTF-8バイトの上限は日時・clear・連番・拡張子を確保してから名前部分へ適用する。
- 既存UIのローカル日時変換をcrate内の`datetime`へ共有化し、秒・ミリ秒付き日時を生成する。
- PNG保存・クリップボード・OBS保持通知は既存処理を利用する。
  明示的なsmoke出力パスは命名処理を通さない。

## 検証

- Windowsで`cargo fmt --check`、`cargo check -p bmz-player --locked`、
  `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`が成功。
  Cargoのincremental cacheでhard linkを作れない環境warningは残る。
- `cargo test -p bmz-player --locked --no-fail-fast`が成功。
  libは2884 passed / 56 ignored、startup_stackは2 ignored。外部素材依存テストを含むため、
  全件成功を全スキンの実機互換確認とは扱わない。
- 新規テストは画面別の命名、全11クリアタイプ、空のSubtitle、コースDecide、
  禁止文字・制御文字、UTF-8長制限、既存ファイルと非同期保存待ちの重複を確認する。
  日時helperはUTC fallbackの日付境界・うるう日・秒を確認する。
- Windowsの独立Winit event loopと32×32 offscreen GPUを使う新規テストを個別実行。
  Selectで要求してResultへ遷移した際のPNG保存名・寸法、今回結果の利用、
  撮影frameでのトースト非表示、明示的なsmokeパスの維持を確認する。
- 変更した文書のPrettier確認と`bun test bmz-ir-web/tests/manual.test.ts`が成功（8 passed）。
- 実ウィンドウでのF12 / 物理コントローラー操作、クリップボード・OBSサービス、macOS / Linuxは未確認。
