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

- [ ] play 画面終了時に画面のフェードアウト処理を追加
- [ ] result 画面終了時に音声のフェードアウト処理を追加
- [ ] 楽曲プレイ後、select 画面の背景が曲のstagefile?になる
- [ ] play画面のAUTO PLAY表示
- [ ] cliのオプション指定方法を変更 (songs scan, songs rescan, -a, --autoplay)
- [ ] LNノーツの処理方法確認
- [ ] Mineノーツの処理方法確認
- [x] play スキンを play5, play7, play10, play14 に分ける
- [ ] 起動時にスキン候補を検索し、egui のコンボボックスで選択可能にする
- [ ] 画面右下にバージョン情報とオートプレイ情報を常時表示
- [ ] 難易度表フォルダ内ではlibrary.dbに無い曲も表示
- [ ] 小節線のOffset(h/a)をスキン問わず設定可能に

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
- [ ] Support Read-only IR (LR2IR, mocha)
- [ ] Support new IR
- [ ] Support normalizing volume per-chart
- [ ] Support OBS WebSocket control integration
- [ ] Discord Rich Presence
