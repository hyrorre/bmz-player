# bmz-player

Next-Generation BMS Player (WIP)

Supported OS: Windows / macOS / Linux (probably works)

Supported Format: BMS (5K / 7K / 10K / 14K)

Supported Skin: beatoraja json skin / beatoraja lua skin

## How to build

### Windows (stable-x86_64-pc-windows-msvc)

Install vcpkg beforehand.

```powershell
winget install llvm.llvm
vcpkg integrate install
vcpkg install ffmpeg:x64-windows
cargo build
cargo run
```

### macOS (stable-aarch64-apple-darwin)

Install Homebrew beforehand.

```sh
brew install ffmpeg
cargo build
cargo run
```

## TODO

- [x] スキンオプションのデフォルト解決をbeatoraja準拠にする
- [x] wgpuバックエンド選択機能 (DirectX 12 / Metal / Vulkan / OpenGL)
- [x] ファイル選択ダイアログ `rfd`
- [ ] アシストオプション、詳細オプション実装
- [ ] ターゲット機能実装
- [ ] select 画面の未実装要素、マウス操作など
- [ ] result 画面終了時に音声のフェードアウト処理を追加
- [ ] LNノーツの処理方法確認
- [ ] Mineノーツの処理方法確認
- [ ] レンダリングバックエンド追加検証 (SDL3)

## Roadmap

- [ ] course
- [ ] score database migration from LR2 / beatoraja
- [ ] Base 62 BMS (62進数BMS)
- [ ] PMS (9K)
- [ ] Qwilight-style BMS (4K / 6K / 8K)
- [ ] BMSON
- [ ] csv skin
- [ ] practice mode
- [ ] Read-only IR (LR2IR, mocha)
- [ ] new IR
- [ ] normalizing volume per-chart
- [ ] OBS WebSocket control integration
- [ ] Discord Rich Presence
