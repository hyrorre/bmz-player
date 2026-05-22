# bmz-player

Next-Generation BMS Player (WIP)

Supported OS: Windows / macOS / Linux (probably works)

Supported Format: BMS (5K / 7K / 10K / 14K)

Supported Skin: beatoraja json skin / beatoraja lua skin

## How to build

### macOS

```sh
brew install ffmpeg
cargo run
```

### Windows

```powershell
vcpkg install ffmpeg-x64 (TODO: check command)
# set environment variables
cargo run
```

## TODO

- [ ] スクリーンショット機能を追加 (F12, profile.tomlに保存場所指定を追加)
- [ ] 膨大な数のBMSファイルが含まれたBMSファイルを開けない問題を修正 (WHERE dte.md5 IN (?,?,...))
- [ ] profile.toml のスキンオプション指定方法を確認
- [ ] profile.toml のコントローラーのキーコンフィグ指定方法を確認
- [ ] Support SE / BGM
- [ ] play 画面終了時に画面のフェードアウト処理を追加
- [ ] result 画面終了時に音声のフェードアウト処理を追加
- [ ] Support BPM change / STOP
- [ ] select 画面をホイールでスクロール可能にする
- [ ] 小節線を表示
- [ ] 楽曲プレイ後、select 画面の背景が曲のstagefile?になる
- [ ] レーンカバーと判定線の出現タイミングをREADYからシーン遷移後に変更
- [ ] コンボ数のアニメーションが機能していない
- [ ] 途中落ちのアニメーション再生
- [ ] FAST / SLOW が表示されていない
- [ ] フルコンボアニメーション再生
- [ ] play画面のAUTO PLAY表示
- [ ] cliのオプション指定方法を変更 (songs scan, songs rescan, -a, --autoplay)

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
