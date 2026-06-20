# bmz-player

Next-Generation BMS Player (WIP)

beatoraja の後継を目指しています。 (LR2ではなく)  
多くの機能はまだ未実装ですが、まずは開発者自身が快適にプレイできることを目指して開発しています。

Supported OS
- Windows
- macOS
- Linux (probably works)

Supported Format
- BMS (5K / 7K / 10K / 14K)
- PMS (9K)
- UE (4K / 6K / 8K)

Supported Skin
- beatoraja json skin
- beatoraja lua skin
- beatoraja csv skin

Features
- ASIO support
- import LR2/beatoraja scores

**Don't use this application for playing copyrighted contents.**

## Clone

Bundled skin assets are managed as Git submodules. Clone with submodules, or initialize them after cloning.

```sh
git clone --recurse-submodules https://github.com/hyrorre/bmz-player.git
```

```sh
git submodule update --init --recursive
```

Currently bundled submodules:

- `data/skins/Rmz-skin` — BMZ bundled fork of Rm-skin. See the skin repository's README and `_license/` directory for license details.

## System Requirements

- Windows: Windows 10 or higher is natively supported via DirectX 12. Older versions of Windows may require Vulkan drivers.
- macOS: macOS 10.13 High Sierra or newer is required to utilize Apple's Metal API.
- Linux: Vulkan support is required. You must ensure you have the correct Vulkan drivers installed (e.g., the proprietary drivers for your dedicated GPU or mesa-vulkan-drivers for Intel/AMD graphics).

## How to build

Install latest graphics driver.

Use the same version of FFmpeg as specified in Cargo.toml

### Windows (stable-x86_64-pc-windows-msvc)

```powershell
# Install vcpkg
git clone https://github.com/microsoft/vcpkg.git C:\vcpkg
$Env:Path+=";C:\vcpkg"
vcpkg integrate install

# Install rust
winget install Rustlang.Rustup llvm.llvm
rustup toolchain install stable-msvc
rustup default stable-msvc

# Install dependencies
vcpkg install ffmpeg:x64-windows

cargo build
cargo run
```

### macOS (stable-aarch64-apple-darwin)

Install Homebrew beforehand.

```sh
# Install rust
brew install rust rustup
rustup toolchain install stable

# Install dependencies
brew install ffmpeg

cargo build
cargo run
```

### Linux (stable-x86_64-unknown-linux-gnu)

Example in `fedora`

```sh
# Install rust
sudo dnf update -y
sudo dnf install -y rustup
rustup-init
source ~/.bashrc

# Install dependencies
sudo dnf install -y gcc g++ make clang llvm git ffmpeg-free ffmpeg-free-devel openssl-devel alsa-lib-devel rust-libudev-devel fontconfig-devel pipewire pipewire-pulseaudio wireplumber alsa-utils alsa-plugins-pulseaudio google-noto-sans-cjk-fonts google-noto-sans-cjk-vf-fonts

cargo build
cargo run
```

## TODO

- [x] 各設定項目のデフォルト調整
- [x] rule modeごとにランプをまとめるor区別するかの設定項目追加
- [x] songs.rootとtablesの並び替え機能
- [x] player statistics
- [x] ランプソートで曲プレイ後カーソルがズレる
- [x] 選曲画面でプレイスキンを変えると選曲スキンも再読込される
- [x] WAV定義などもIRに送信されている
- [x] スキンオプションのデフォルト解決をbeatoraja準拠にする
- [x] リザルトスキンとコースリザルトスキンが分かれていない
- [x] ランキングのユーザー重複を排除し、ベストスコアのみ表示する
- [x] ランキングのテーブルの上に自己ベストスコア表示と自己スコア全履歴を表示するボタンを追加
- [x] song searchのカーソル移動機能
- [x] profile機能拡充
- [x] 同梱スキン準備作業 (git submodule化)
- [x] ライセンス周りの整備
- [x] 同梱スキンに4K / 6K / 8Kを追加
- [x] 判定調整に応じて小節線の位置も調整
- [x] スクリーンショットの非同期化
- [x] 8K / 9K のキーコンフィグ項目が2P KEY1 / 2P KEY2になっているのでKEY8 / KEY9に変更
- [x] /入力で検索モードに入る条件の調整
- [x] クイックリトライの操作方法検討と実装
- [x] defaultの result / course result が機能しておらず、シーンがスキップされ select 画面に戻ってしまう
- [x] course mode でコンボ数を引き継ぐよう変更 (単曲スコアとして保存されるMAX COMBOは単曲のものを使う)
- [x] course modeでdecide画面がスキップされている
- [x] 曲開始前に終了させると、READYまでFAILED演出が出ない
- [x] course mode ir送信エラー rule.ln_policy is invalid
- [x] course mode 途中落ちでも単曲リザルトを表示させたい
- [x] course mode 時に単曲のクリアランプがCLEARになるが、NO PLAYにしたい (beatorajaと同様)
- [x] 毎曲ごとにdecide画面が表示されるが、course選択時のみにしたい
- [x] decide画面では曲名の代わりにcourse名を表示させる
- [x] course mode時、play skinに曲名が表示されるのが遅い
- [x] course result skinに各曲の情報、IR情報が表示されていない
- [x] course result時にF1を押した時、コースリザルト情報は出てくるが、IR情報が出てこない
- [x] course mode の単曲リザルトでIR送信対象が無しになっている
- [x] course mode の単曲リザルトでMYBEST表示、MYBESTとの差分表示が無い
- [x] course mode の最後の曲の単曲リザルトも表示する
- [x] course result skinのMYBEST表示が今回のスコア表示になっている
- [x] course result skinの各曲の情報表示窓に表示されるのがcourse 1-4という仮のテキスト？になっている
- [x] 単曲リザルトだけでなくcourse resultもNoPlayに丸められている (CLEAR, HARD, EXHなどになるべき)
- [x] course mode の Gauge Auto Shift 対応

- [x] アナログスクラッチの停止判定が緩すぎて、連続で同じ方向に回すと全然反応しない (beatorajaと同じアルゴリズムを目指す)
- [x] Calamity Fortune[F]のLN終端が普通のノーツになっている (G:\BMS\INSANE\[ZUN remixed by LeaF] Calamity Fortune)
- [x] 設定フォルダ内で[編集中]にスクラッチを回して選択項目を上下させる操作に対応させる
- [x] 画像形式のBGAの読み込みが同期になっており遅い
- [ ] play skin turntableの回転方向が逆？
- [x] 低解像度BGAのピクセル補間について確認
- [x] IRの送信完了タイマーとrefへの対応
- [x] decide skin, result skinへ難易度表情報を連携 (その譜面がどの難易度表のどの難易度に含まれているか)
- [x] beatorajaにselect skinへ難易度表情報を連携するインターフェースがあるか確認
- [x] リザルト画面のSEフェードアウト
- [ ] /deletescoreコマンドの実装
- [ ] 難易度表のデフォルトを追加
- [ ] ln_policyをAUTO/FORCEとLN/CN/HCNに分けることでselect skinのLN変更機能を有効化
- [ ] IR側でln_policyが見やすくなるよう表示を調整
- [ ] `Noto Sans CJK JP` or `Noto Sans JP` 同梱
- [ ] アシストオプション、詳細オプション実装
- [ ] Select画面の操作変更とスキン側の不一致について考える
- [ ] Select画面の操作が一部profile.tomlに設定されており複雑なので整理


## Roadmap

- [x] Base 62 BMS (62進数BMS)
- [x] course
- [x] BMSON
- [x] PMS (9K)
- [x] csv skin (beatoraja compliant)
- [x] score database migration from LR2 / beatoraja
- [x] auto-adjust (自動判定調整)
- [x] normalizing volume per-chart
- [x] UE-style BMS (4K / 6K / 8K)
- [x] new IR (bmz-ir)
- [ ] IR score import (IRからスコアをダウンロードしてscore.dbに保存する機能)
- [ ] random select
- [ ] battle mode
- [ ] rec mode (譜面動画作成モード)
- [ ] practice mode
- [ ] read-only IR (LR2IR, Mocha, MinIR)
- [ ] OBS WebSocket control integration
- [ ] Discord Rich Presence
- [ ] Arena Mode
- [ ] i18n
- [ ] RawInput / GameInput / 8000Hz Input
- [ ] WASAPI exclusive
- [ ] ギミック系BMSへの対応

## Out of Scope (but welcome your contributions)

- [ ] 24K / 48K BMS
- [ ] 18K PMS (9K DOUBLE PLAY)
- [ ] LR2-style csv skin
- [ ] More features like LR2
- [ ] osu! mania charts
- [ ] ModernChic skin support (too much code that relies on Java / libGDX)
