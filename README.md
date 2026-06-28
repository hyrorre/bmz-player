# bmz-player

Next-Generation BMS Player (WIP)

beatoraja の後継を目指しています。
多くの機能は未実装ですが、まずは開発者自身が快適にプレイできることを目指して開発しています。

Supported OS
- Windows
- macOS
- Linux (probably works)

Supported Format
- BMS (5K / 7K / 10K / 14K)
- PMS (9K)
- Qwilight/UE-style BMS (4K / 6K / 8K)
- Base 62 BMS (62進数BMS)
- BMSON

Supported Skin
- beatoraja json skin
- beatoraja lua skin
- beatoraja csv skin

Features
- score database migration from LR2 / beatoraja
- auto-adjust (自動判定調整)
- normalizing volume per-chart (自動音量調整)
- internet ranking (BMZ IR)

**Don't use this application for playing copyrighted contents.**

## Recommended Skins

- m-select (bundled)
- Rm-skin (bundled)
- EC:FN / Starseeker ([https://kaidou0912.hatenablog.com/entry/2025/03/01/151604](https://kaidou0912.hatenablog.com/entry/2025/03/01/151604))

data_dir の skins フォルダに配置してください。

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
- `data/skins/mz-select` — BMZ bundled copy of m-select. See the skin repository's `readme.txt` and `license/` directory for license details.

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

- [ ] score.dbのクリアランプが更新されないことがある
- [ ] 表示できない画像BGAが多い
- [ ] BGA/WAVの拡張子探索順の確認
- [ ] 画像形式のBGAの読み込み処理改善 (現状読み込み終わるまでplay画面に入らないようになっている)
- [ ] LN終了時にキー音が鳴るバグ
- [ ] 同じWAV定義のノーツが複数置かれているときのキー音の鳴らし方調整
- [ ] portableと開発ビルドでskinがすべて同梱扱いになっている (同梱ラベルを削除)
- [ ] インストーラー版のスクリーンショットのデフォルト保存先がdata/screenshotsになっている (OSのdata_dirに変更が必要)
- [ ] GitHub Actionsで自動ビルドされたバイナリがASIO非対応になっている？
- [ ] Starseeker result skin の MISS COUNT 差分の色が逆？(-が白、+が赤な気がする)
- [ ] クイックリトライの挙動改善 (ロードのスキップ、画面が固まる部分の処理改善)
- [ ] result skin の背景動画が固まることがある
- [ ] 初プレイの曲のresult skinで、MYBEST枠に今回のプレイ結果が表示される (NOPLAY, score=0で表示されるべき)
- [ ] beatoraja風の緑数字変更機能を実装 (E2+KeyN, E2+Scr)
- [ ] E1/E2 hold 中はREADY状態にならないようガード
- [ ] E1 hold状態でプレイ画面遷移時に、緑数字などが表示されるようにする
- [ ] 最終ノーツ処理後にE1/E2を押すとfadeout開始 (このときクイックリトライ判定にならないよう注意)
- [ ] 設定画面でE2が戻る機能になっている
- [ ] play skin turntable の回転方向が逆
- [ ] decide/resultのfadeoutスキップボタンをKey1/3/5/7からE1/E2に変更検討
- [ ] 難易度表のデフォルトを追加 (DP/PMS)
- [ ] ln_policyをAUTO/FORCEとLN/CN/HCNに分けることでselect skinのLN変更機能を有効化
- [ ] IR側でln_policyが見やすくなるよう表示を調整
- [ ] アシストオプション、詳細オプション実装
- [ ] Select画面の操作変更とスキン側の不一致について考える
- [ ] Select画面の操作が一部profile.tomlに設定されており複雑なので整理
- [ ] egui設定ウィンドウを整理
- [ ] skin 独自拡張(ref/timer) 仕様検討
  - [ ] NHS / FHS
  - [ ] Ranking 切り替え (Ranking / Rival / Self-only)
  - [ ] WMII result skin 対応
  - [ ] select skin の option panel系


## Roadmap

- [ ] IR score.db import (IRからスコアをダウンロードしてscore.dbに保存する機能)
- [ ] IR score.db upload (ローカルのscore.dbをIRにアップロードする機能)
- [ ] random select
- [ ] battle mode
- [ ] rec mode (譜面動画作成モード)
- [ ] practice mode
- [ ] read-only IR (LR2IR, Mocha, MinIR)
- [ ] OBS WebSocket control integration
- [ ] Discord Rich Presence
- [ ] Arena Mode
- [ ] i18n (en / ko / zh-CN / zh-TW / zh-HK)
- [ ] RawInput / GameInput / 8000Hz Input
- [ ] WASAPI exclusive
- [ ] ギミック系BMSへの対応
- [ ] auto generate preview
- [ ] non stop mode

## Out of Scope (but welcome your contributions)

- [ ] 24K / 48K BMS
- [ ] 18K PMS (9K DOUBLE PLAY)
- [ ] LR2-style csv skin
- [ ] More features like LR2
- [ ] osu! mania charts
- [ ] ModernChic skin support (too much code that relies on Java / libGDX)
