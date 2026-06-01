# bmz-player

Next-Generation BMS Player (WIP)

Supported OS: Windows / macOS / Linux (probably works)

Supported Format: BMS (5K / 7K / 10K / 14K)

Supported Skin: beatoraja json skin / beatoraja lua skin

## How to build

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

- [x] スキンオプションのデフォルト解決をbeatoraja準拠にする
- [x] wgpuバックエンド選択機能 (DirectX 12 / Metal / Vulkan / OpenGL)
- [x] ファイル選択ダイアログ `rfd`
- [ ] アシストオプション、詳細オプション実装
- [ ] select 画面の未実装要素、マウス操作など
- [ ] result 画面終了時に音声のフェードアウト処理を追加
- [ ] LNノーツの処理方法確認
- [ ] Mineノーツの処理方法確認
- [ ] 判定とゲージのアルゴリズム変更機能 (beatoraja Mode / LR2oraja Mode / DX Mode)
- [ ] FAST/SLOW 表示条件変更機能 (ms単位)
- [ ] SELECT画面UIベースの設定変更機能
- [ ] Fontで `BIZ UDGothic` / `Noto Sans CJK JP` / `Noto Sans JP` を優先表示
- [ ] SUDDEN / HIDDEN / LIFT をint 1000段階に
- [ ] volume関係をint 100段階に
- [ ] Select画面の操作変更とスキン側の不一致について考える

## Roadmap

- [x] Base 62 BMS (62進数BMS)
- [x] course
- [x] BMSON
- [x] PMS (9K)
- [ ] UE-style BMS (4K / 6K / 8K)
- [ ] Aery-style BMS (5K / 7K)
- [ ] csv skin (beatoraja compliant)
- [ ] score database migration from LR2 / beatoraja
- [ ] practice mode
- [ ] read-only IR (LR2IR, Mocha, MinIR)
- [ ] new IR (bmz-ir)
- [ ] normalizing volume per-chart
- [ ] OBS WebSocket control integration
- [ ] Discord Rich Presence
- [ ] Arena Mode
- [ ] i18n

## Out of Scope (but welcome your contributions)

- [ ] 24K / 48K BMS
- [ ] 18K PMS (9K DOUBLE PLAY)
- [ ] LR2-style csv skin
- [ ] More features like LR2
- [ ] osu! mania charts
- [ ] ModernChic skin support (too much code that relies on Java / libGDX)
