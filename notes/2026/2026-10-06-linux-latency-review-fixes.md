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
