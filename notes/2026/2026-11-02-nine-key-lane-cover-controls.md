# 9KEYの補助ボタンとKEY8 / KEY9によるレーンカバー操作

9Kのボタンだけでプレイ中のレーンカバーを動かせるようにします。
操作の仕様は[操作方法](../../docs/controls.md#レーンカバーとhs)で管理します。

## 操作と実装

- E1 + KEY8 / KEY9をUp / Downと同じ操作にします。
- E2 + KEY8 / KEY9をE2 + Up / Downと同じ1刻みの微調整にします。
- 設定された押下量、長押し待機時間、連続変化量、カバー変更対象、カバー無効時のHS操作を共用します。
- キーコンフィグ後の論理KEY8 / KEY9で判断し、キーボードとコントローラーの両方に適用します。
- 長押しはデバイスと物理入力で追跡し、鍵盤や補助ボタンの解放、E1+E2の同時押しで終了します。
- 補助ボタンの判定には最新の押下状態を使い、ゲーム状態の反映を待たずに同じ入力バッチの鍵盤操作を受け付けます。
- 9KのKEY1〜KEY7はHS / 緑数字操作です。ほかのキーモードの割り当ては維持します。
- 日本語・英語・韓国語・中国語の操作マニュアルを更新します。

## 検証

Windows環境で、今回の変更だけを適用した作業用worktreeで検証しました。
同梱スキンsubmodule 3件を初期化し、共有targetを使うため`BMZ_RESOURCE_DIR`と`BMZ_DATA_DIR`を検証用worktree内のパスへ指定しました。

- `cargo fmt --check`: 成功です。
- `cargo check -p bmz-player --locked`: 成功です。
- `cargo clippy -p bmz-player --all-targets --locked -- -D warnings`: 成功です。
- `cargo test -p bmz-player --locked`: 2,801件成功、51件ignoreです。起動用の実機テスト2件もignoreです。
- `cargo build -p bmz-player --locked`: 元のcheckoutでWindowsビルドが成功しました。
- `bun test bmz-ir-web/tests/manual.test.ts`: 8件成功です。

追加した3件の回帰テストは、キーコンフィグ後のキーボード / コントローラーとカーソルキーの操作量の一致、長押しと補助ボタンの解放、別デバイスの識別、9Kの残りの鍵盤と他モードのHS操作を確認します。
既存の「選曲入力モードではなく譜面モードを使う」テストも新しい9K操作の期待値へ更新しました。

通常の実行制限下ではローカルHTTPを使うダウンロードテストが失敗したため、通信制限を外して全体テストを実行しました。
検証用worktreeには同梱スキンを用意しました。第三者製の追加スキンを必要とするテストには、素材不足で早期returnするものがあります。
詳細ログはGit管理外の`.local/nine-key-cover-*.log`に保存しています。
実物の9KEYコントローラーとmacOS / Linuxでの操作は未確認です。
