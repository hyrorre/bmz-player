# Linux遅延対応のレビュー指摘修正

開始時点は `feat/linux-latency` の `cf08d9fa`。`main` との差分レビューで見つかった
focus配送、共有入力queueのoverflow回復、build metadataの再実行条件を順に修正する。
Linux環境で実施し、既存のRmz-skin / mz-select submoduleの変更は保持した。

## 描画を待たないfocus配送

Waylandでは非表示中にframe callbackが止まり、`RedrawRequested` が来ないことがある。
実効focusの変更時に既存の入力routeへ直接focusを反映し、gilrs / native keyboardの
配送停止・再開を描画から独立させた。route自体の更新とmacOSの実効focus判定は維持する。
現在の契約は [gameplay-runtime.md](../../docs/gameplay-runtime.md) を参照。

- 回帰テストは描画とroute再設定なしでfocus喪失・復帰を起こし、旧保持のrelease、
  非focus中の配送停止、復帰後のpressを確認。入力captureの3テストが成功。
- `cargo fmt --check`、`cargo check -p bmz-player --locked --features linux-evdev`、
  `cargo clippy -p bmz-player --all-targets --locked --features linux-evdev -- -D warnings` が成功。
- `LANG=ja_JP.UTF-8 cargo test -p bmz-player --locked --features linux-evdev --no-fail-fast` は
  libで2187成功・40失敗・19 ignored。40件のうちsandboxのloopback通信禁止による17件は
  制限外で個別に再実行して全成功。残る23件は変更対象外の外部スキン関連テスト。
  外部アセットが無く早期returnするテストもあり、成功件数はスキン互換性の保証ではない。
- Wayland compositor上でウィンドウを隠す実機確認、Windows / macOS確認は未実施。

詳細ログは `.local/performance/linux-latency-review-2026-10-06/` に保存した。
ローカル専用であり、この記録には同梱しない。

## 共有入力queueのoverflow回復

破棄するqueueとoverflowを起こしたイベントを順に適用して、実際に残る保持だけを抑止する。
Releaseが確認済みのキーは次のPressを受け付ける。過去のoverflowで抑止した保持は
合成Releaseでは解除せず、実入力のReleaseまで維持する。
配送済み状態はgameplayがReleaseをdrainするまで保存し、その前の再overflowでも
Releaseを再生成する。現在の契約は [linux-latency.md](../../docs/linux-latency.md) を参照。

- 配送済み/未配送とqueue内/overflow原因のReleaseの4組合せ、Release後の再保持、
  drain前の連続overflowを回帰テストで確認。入力関連88テストが成功。
- fmt、linux-evdev付きcheck / all-targets Clippyが成功。
- player全体は2190成功・40失敗・19 ignored。失敗したテスト集合は1件目の検証と同じで、
  通信制限17件と外部スキン23件。新しい失敗はない。

## build metadataと再ビルド条件

存在しない `BUILD-COMMIT` / `packed-refs` 等をCargoの監視対象に渡していたため、
変更なしでも毎回build scriptとplayerが再コンパイルされていた。既存metadataだけを監視し、
GitのHEAD・refパスはlinked worktreeも扱える `git rev-parse --git-path` で取得する。
packed branchの次のcommitで作られるloose refは、作成前には既存の親ディレクトリを監視する。
手順は [development.md](../../docs/development.md) に記載。

- `scripts/test_build_identity.py` は実際のbuild.rsを小さなCargo workspaceで実行する。
  通常checkout、linked worktree、manifestあり/なしのアーカイブの4テストで、修正前は
  変更なしのビルドがfreshにならず全失敗、修正後は全成功。
- packed refからのcommit、通常checkoutとworktreeのdetached HEAD、dirtyソース、
  archive manifestの更新、overrideの設定・解除で識別情報が更新されることも確認。
- fmt、linux-evdev付きcheck / all-targets Clippyが成功。
- 最終のplayer全体テストはloopback通信可能な環境で2207成功・23失敗・19 ignored。
  失敗集合は前2回の外部スキン23件と一致。
  [先行記録](2026-10-06-linux-input-alternatives.md)にも同じ外部スキン環境の失敗が記録されている。
- 直後の `cargo test -p bmz-player --locked --features linux-evdev --no-run` は
  再コンパイルなしで0.19秒。これは当環境の無変更ビルドの確認であり、実行時の遅延計測ではない。
- 今回はLinuxのplayer crateを検証。Windows / macOS実ビルド、Wayland非表示の実機操作、
  PipeWire feature / 配布パッケージの再ビルド、workspace全体の再検証は未実施。
