# v0.5.0 Linux tar CIのD-Bus依存

対象: v0.5.0 (`11facc8f`)、[Release Apps run 40419808072](https://github.com/hyrorre/bmz-player/actions/runs/40419808072)。

Linux tarのworkspace testでは、アイドル抑止の9テストが専用`dbus-daemon`の起動時に
`Permission denied`で失敗した。ビルドイメージは`libdbus-1-dev`をインストールするが、
推奨パッケージの`dbus`をインストールしない。テストは`nobody`で実行される。
PATH探索で権限エラーが返る正確な経路はCIログからは確認できていない。

ビルドステージに`dbus`を明示し、イメージ作成時に`nobody`で`dbus-daemon --version`を
実行して起動可能性を確認する。専用バスとPortal / ScreenSaverの動作検証は既存の
workspace testで行う。ビルド手順は[Linux tarの説明](../../docs/linux-tar.md)を参照。

Windows上の既存パッケージテストは15件中11件成功。4件はエラーメッセージのパス区切りが
Windowsの`\`となり、`/`を期待する既存のassertionに一致せず失敗した。
Dockerは利用できず、WSLも仮想ディスクを開けなかったため、Linuxコンテナ内のテストと
修正後のRelease CIは未実施。

## 同日の対応ソース取得エラー

[run 40422983023](https://github.com/hyrorre/bmz-player/actions/runs/40422983023)では
D-Busのテスト10件とrelease buildが通過した後、`systemd=249.11-0ubuntu3.22`の
対応ソースをAPTで取得できずLinux tarの作成が失敗した。イメージ内では`libudev1`が
`.22`から`.23`に更新される一方、更新されないパッケージも残っていた。`.22`を要求した
ライブラリの名前は既存ログから確定できない。

ビルド用イメージの既存パッケージも更新し、更新可能・保留中の一覧をログに出す。
同梱ライブラリのバイナリと対応ソースを取得前に記録し、失敗時にも対象を明示する。
同じソース名の複数バージョンが必要なら、現在の単一`.dsc`形式では生成前に停止する。
対応ソースの厳密なバージョン一致と取得後の検証は維持する。

Windows上のパッケージテスト19件中15件が成功し、追加した4件はすべて成功した。
既存の4件は上記と同じパス区切りの違いで失敗した。Dockerは利用できず、
Ubuntu・FedoraのWSLは仮想ディスクを開けなかったため、イメージの更新、実際の
対応ソース取得、Linux tar全体、および修正後のCIは未検証。
