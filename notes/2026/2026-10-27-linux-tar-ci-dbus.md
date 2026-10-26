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
