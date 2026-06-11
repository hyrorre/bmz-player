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

- [ ] スキンオプションのデフォルト解決をbeatoraja準拠にする
- [ ] アシストオプション、詳細オプション実装
- [ ] LNノーツの処理方法確認
- [ ] Mineノーツの処理方法確認
- [ ] Fontで `BIZ UDGothic` / `Noto Sans CJK JP` / `Noto Sans JP` を優先表示
- [ ] Select画面の操作変更とスキン側の不一致について考える
- [ ] song searchのカーソル移動機能
- [ ] profile機能拡充
- [ ] 各設定項目のデフォルト調整

## Roadmap

- [x] Base 62 BMS (62進数BMS)
- [x] course
- [x] BMSON
- [x] PMS (9K)
- [x] csv skin (beatoraja compliant)
- [x] score database migration from LR2 / beatoraja
- [ ] UE-style BMS (4K / 6K / 8K)
- [ ] practice mode
- [ ] new IR (bmz-ir)
- [ ] read-only IR (LR2IR, Mocha, MinIR)
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
