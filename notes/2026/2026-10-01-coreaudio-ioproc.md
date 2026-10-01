# Core Audio IOProc音声と16 frames設定

IOProcの実験用backendと、最小16 framesの音声設定を実装した。
現在の操作・対応範囲は [操作](../../docs/controls.md)、
時計と比較の注意点は [macOS遅延検証](../../docs/macos-latency.md)、
featureのビルドは [配布](../../docs/packaging.md) を参照。

## 実装

- `experimental-coreaudio-ioproc`でのみmacOSのIOProcをコンパイル。通常のCPAL出力を維持。
- player/audioの設定とデバイス列挙は共通。保存設定の`CoreAudioIoProc`を非対応ビルドでAutoへ移行。
- HAL IOProcへpacked float32/float64を直接出力。planar、チャンネル順に連続する複数stream、出力channel pairに対応。
  stream configurationとvirtual formatを開始前に照合。監視対象外のproperty通知が混在しても無音化しない。
- NativeOutputRenderer、既存AudioEngine、source/engine command queue、AudioClockのframe座標を共用。
- nominal sample rate、buffer rangeと採用値を確認。非対応形式・レートはエラー。
- property listenerはレート・virtual format・切断時に無音化し、HAL overloadを別カウンターへ記録。
- callback停止・登録解除後にcontextを解放。解除失敗時はcontextを保持しUAFを避ける。
- Fixedバッファの下限をF1/スキン設定とも16へ変更。プリセットにも16を追加。既定は256を維持。

## 検証

macOS 26.6.2 / arm64、debug、48 kHz、実出力「外部ヘッドフォン」で実施。
無音の実機テストで16 framesを2回open/start（各3秒）/dropし、実callback framesが16であることを確認。
各runのstream error・HAL overload・timeline catch-up・lock missは0。
この短い無音試験は多音再生や長時間の負荷試験、物理出力遅延の証拠ではない。
音声形式・planar/multi-stream分配・変更時の拒否・時刻欠落・逆行・overload分類を単体テストで確認。

同じ機器・48 kHz・debug・IOProc 16 framesで、同梱sample-playableのautoplayをリザルトまで実行し
正常終了を確認。最終診断は実callback frames 16、stream error / engine lock miss / queue drop /
timeline catch-upが0、HAL processor overloadは1件だった。このrunには並行ビルドのCPU負荷を含む。
超過0や16 framesでの長時間安定動作を保証する結果として扱わない。
データ・設定・DBは通常環境から分離した`.local/performance/ioproc-smoke.cPVmmJ/`へ保存。
ローカルログ・DBはコミットしない。

feature有効・通常feature構成の両方でworkspace check / all-targets Clippy（`-D warnings`）/ testが成功。
testは既存の実機・外部素材依存のignored testを除き、`--test-threads=1`で実施した。
開始前のbuffer/channel構成確認と監視対象外通知の除外を加えた後も、実機16 framesでの
2回のopen/start/dropは成功。workspaceテストを並行実行したこの再確認のHAL overloadは1件/0件。
stream error、lock miss、timeline catch-upは両方0だった。

最終ビルドで並行ビルド・workspaceテストを止め、同じ譜面を再生して正常終了を確認。
`play-final.log`の最終集計も実frames 16、stream error / lock miss / queue drop / catch-upが0、
HAL overloadが1件だった。並行ビルドだけを原因と断定しない。
約15秒の出力稼働であり、長時間安定性やアナログ出力遅延は未確認のまま。

## 未確認

物理キー音の改善量、16 framesの長時間多音再生、外部USB/HDMI/Bluetooth機器、
実機でのスリープ復帰・抜き差し・他アプリからの形式変更は未確認。
IOProcのoutputTime−nowとCPALのplayback−callbackは同じ測定量ではない。
device/stream latencyとアナログ出力を含む測定は別途必要。

## レビュー修正: 入力のない機器に限定

IOProc登録APIは出力専用ではないため、入力streamがある機器を登録前に拒否する。
共有設定変更前・登録直前・開始前に確認し、取得失敗も許容しない。入力構成変更も監視する。
duplex機器は従来のCore Audioを使う。登録後のstream usage設定では登録時の権限要求を防げないため、
対応範囲を明示的に限定した。実機の権限要求・マイク取得を伴う検証は行わない。
入力streamあり/なし・取得失敗と、入力構成変更通知の回帰テストを追加。
feature有効のaudio/player check・all-targets Clippy、audioテスト120件（実機2件ignored）、
playerのIOProc設定テスト2件が成功。

## 参照資料

- Apple SDK `AudioHardware.h` / `AudioHardwareBase.h` / `CoreAudioTypes.h`。
- [AudioDeviceIOProc](https://developer.apple.com/documentation/coreaudio/audiodeviceioproc)
- [AudioDeviceCreateIOProcID](https://developer.apple.com/documentation/coreaudio/audiodevicecreateioprocid(_:_:_:_:))
- CPAL 0.18.1のmacOS device / output callback実装。
- 依存は既存CPALのtransitive依存を任意direct依存として使用。objc2 framework bindingsは
  Zlib / Apache-2.0 / MIT、mach2はBSD-2-Clause / MIT / Apache-2.0。配布のlicense生成は従来手順を使う。
