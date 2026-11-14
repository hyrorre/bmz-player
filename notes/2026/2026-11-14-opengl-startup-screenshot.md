# OpenGL の起動復旧とスクリーンショット

基点は `7a19ad32`。wgpu 29.0.3 の OpenGL Surface は `RENDER_ATTACHMENT` をサポートする。従来は BMZ が初期化と同期設定変更で `COPY_SRC` を要求していたため、Surface の設定が失敗した。

Surface の対応用途を初期化時に取得し、`COPY_SRC` を使えない場合は、撮影フレームだけスキンと egui をウィンドウの物理解像度のテクスチャに合成する。その画像を等倍で Surface へ描画し、同じ画像を非同期に読み出す。Linux の EGL でウィンドウの表示環境を選べるよう、wgpu へ winit のディスプレイハンドルを渡す。

OpenGL の初期化に失敗した場合は OS に応じた次の候補を試す。CLI のレンダラー指定はその起動だけに適用する。設定と操作は[映像設定](../../docs/settings.md#映像)と[CLI 起動時オプション](../../docs/cli-overrides.md#レンダリングバックエンド)を参照。

## 検証

- **Windows 11 / NVIDIA GeForce RTX 5090（OpenGL 4.6、ドライバー 621.14）**で、`--renderer gl --smoke-screenshot` により実効バックエンドが `Gl` であること、初回表示、960×600 の PNG 保存を確認した。実ウィンドウで BMZ メニューを開き、F12 の撮影とクリップボードへのコピーを確認した。内部解像度を `Native` から `Skin`、同期モードを `Vsync` から `Vsync Off`（実効 `Immediate`）に切り替え、1480×960 で再撮影した。通常最大化と最小化からの復帰後は 3840×2036 で保存した。
- フォールバック検証用ビルドでは OpenGL Surface の初回設定だけに `COPY_SRC` を付け、元の wgpu バリデーションエラーを再現した。保存済み `Gl` から DX12 で選曲画面まで起動し、BMZ メニューの「映像」で Vulkan に変更できた。検証用の変更を外したビルドで、バックエンドを CLI 指定せずに起動して Vulkan が選ばれることを確認した。保存値が `Vulkan` の状態で検証用ビルドを `--renderer gl` で起動すると DX12 で表示・撮影でき、保存値は `Vulkan` だった。
- DX12・Vulkan の実ウィンドウで PNG を保存した。サンプル譜面のオフスクリーン動画出力は H.264、320×180、15 fps、229 フレーム。OpenGL・DX12・Vulkan のオフスクリーン GPU テストで、内部解像度 `Native` / `Skin` の中間テクスチャ経由の PNG・出力先ピクセルと直接描画が一致した。描画をスキップした後も撮影要求が保持された。
- `cargo fmt --check`、workspace の `check --locked`、`clippy --all-targets --locked -- -D warnings`、`test --locked --no-fail-fast` が成功した。テストは指定コミットのスキン submodule を配置して実行した。
- Linux の Wayland・X11、macOS の Metal へのフォールバックと ANGLE での OpenGL 描画は実ウィンドウ未確認。既存の WSL ディストリビューションは仮想ディスクが見つからず起動できず、Linux 開発ホストにも接続できなかった。macOS は wgpu のビルド条件と候補順をソースで確認した。

生ログと画像は作業用 worktree の `validation/` に保管した。
