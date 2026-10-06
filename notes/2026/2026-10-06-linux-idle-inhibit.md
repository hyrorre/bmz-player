# Linuxのコントローラープレイ中のアイドル抑止

## 背景と方針

コントローラーで演奏中にスクリーンセーバーが起動したとの報告を受けて実装した。
Linuxのコントローラー入力はgilrsの専用取得スレッドから配送しているが、
BMZにはデスクトップのアイドル状態を抑止する処理がなかった。

OS設定の変更や疑似キー入力には依存せず、BMZのウィンドウがフォーカスを持つ間だけ
D-Busで抑止を保持する。選曲・設定・リザルト・オートプレイ・リプレイも対象にし、
フォーカス喪失・ウィンドウ破棄・アプリ中断・終了で解除する。
前面で放置した場合も抑止される。現在の操作仕様は[controls.md](../../docs/controls.md)を参照。

## 実装

- `bmz-player::idle_inhibit`が専用workerと接続を所有する。
  window threadは最新のフォーカス状態だけをwatch channelへ渡し、入力・判定には追加しない。
- `org.freedesktop.portal.Inhibit.Inhibit`にIdle（8）だけを指定する。
  応答シグナルを呼び出し前に購読し、成功応答を確認してから有効と扱う。
- Portalの不在・拒否・タイムアウト時は`org.freedesktop.ScreenSaver`へ切り替える。
  `/org/freedesktop/ScreenSaver`、旧`/ScreenSaver`の順に試す。
- 取得試行ごとに独立したD-Bus接続を持つ。フォーカス喪失・終了で待機中の要求を取り消し、
  応答が未着でも接続を切断する。返却済みのhandle/cookieはClose/UnInhibitで解除する。
- サービス再起動後のcookie再利用を考慮し、解除先は取得時のunique ownerへ固定する。
  NameOwnerChangedと接続切断を監視し、失効した抑止を有効扱いし続けない。
- 接続・取得・解除には2秒の期限を設ける。全経路が失敗した場合は警告を記録し、
  フォーカス中は30秒後に再試行する。終了時だけworkerをjoinし、接続解放を待つ。
- 既存のLinux向けzbus依存を通常ビルドでも有効化した。追加パッケージ・lockfile変更はない。

参考仕様:
[Portal Inhibit](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Inhibit.html)、
[Portal Request](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Request.html)、
[Idle Inhibition Service](https://specifications.freedesktop.org/idle-inhibit/latest/)。

## 検証

Linuxのテストはサービス自動起動を無効にした専用dbus-daemonと模擬サービスを使う。
通常のsession busやデスクトップ設定は変更しない。必要な環境は
[development.md](../../docs/development.md)を参照。

- 回帰テスト10件が成功。Portal成功・拒否・不在・タイムアウト、早い応答、
  フォーカス喪失と復帰、同一フォーカス通知、応答待ち中の終了、旧ScreenSaverパス、
  サービスowner喪失、元のownerへのcookie解除を確認した。
- サンドボックス内ではUnixソケット作成が禁止されていたため、同じ専用busテストを
  サンドボックス外で実行した。テストをskipして成功扱いにはしていない。
- `cargo fmt --check`、`cargo check --workspace --locked --offline`、
  `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`、
  `cargo build -p bmz-player --locked --offline`が成功した。
  既存の任意入力構成も`cargo check -p bmz-player --locked --offline --features linux-evdev`で確認した。
- `cargo test --workspace --locked --offline --no-fail-fast`は合計3764件成功、6件失敗、26件ignore。
  失敗は未初期化の`data/skins/Luxez-Flat`を要求する既存skin_loaderテスト6件のみ。
  `luxe_detail_options`の5件と`detail_experimental_off_keeps_legacy_and_on_shows_numbers_without_buttons`。
  default / mz-select / Rmz-skinとsample-playableは存在する。外部スキンを要求して
  早期returnするテストも含まれるため、全スキンの互換確認を意味しない。
- KDE / Wayland環境でPortal Inhibit v3とScreenSaver API公開を読み取りで確認した。
  一時データディレクトリでsample-playableのAutoplayを240 Play frames実行し、
  実際のPortal成功応答、終了時のhandle解除・接続解放、正常終了をログで確認した。
  既存config / DBは使用していない。
- 長時間の実コントローラープレイ、自動ロック時間を超える確認、実デスクトップ上での
  フォーカス切替、X11 / GNOME / Flatpakの実行確認は未実施。

生ログと一時runtime dataはGit管理外の`.local/performance/idle-inhibit/`に保存した。
smoke成功は性能改善の根拠にはしない。
