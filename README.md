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

- [ ] デフォルトスキンのグルーブゲージが表示されないよう修正
- [ ] スクリーンショット機能を追加 (F12, profile.tomlに保存場所指定を追加)
- [ ] profile.toml のコントローラーのキーコンフィグ指定方法を確認
- [ ] Support SE / BGM
- [ ] play 画面終了時に画面のフェードアウト処理を追加
- [ ] result 画面終了時に音声のフェードアウト処理を追加
- [ ] 小節線を表示
- [ ] 楽曲プレイ後、select 画面の背景が曲のstagefile?になる
- [ ] 途中落ちのアニメーション再生
- [ ] フルコンボアニメーション再生
- [ ] play画面のAUTO PLAY表示
- [ ] egui の日本語の vertical-align を修正
- [ ] cliのオプション指定方法を変更 (songs scan, songs rescan, -a, --autoplay)
- [ ] All offset / Notes offset / Judge offset / Judge Detail offset

## Roadmap

- [ ] Support deside skin
- [ ] Support course
- [ ] Support courseresult skin
- [ ] Support score database migration from LR2 / beatoraja
- [ ] Support Base 62 BMS (62進数BMS)
- [ ] Support PMS (9K)
- [ ] Support Qwilight-style BMS (4K / 6K / 8K)
- [ ] Support BMSON
- [ ] Support csv skin
- [ ] Support IR
- [ ] Support 22K BMS
