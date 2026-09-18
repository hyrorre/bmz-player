Adding skins / スキンの追加
=========================

In BMZ Player, open F1 > Settings > Skin > Open additional skins folder.
Place the complete skin folder there, including its fonts, images and shared
libraries. Then click Refresh skin list and select the skin for each scene.
The path shown in the game is the actual data_dir/skins for this installation.
It also accounts for portable installations and custom data directories.

This resources/skins folder contains application resources. Packaged files at
the same paths are overwritten by updates. Windows Setup preserves files not
included in the package, but macOS app updates replace the whole app bundle.
Keep additional skins and editable copies in the additional skins folder.
Respect each skin's license when copying or editing it.

Existing skins are not moved automatically. To relocate one, copy the complete
skin and any shared libraries, then select the copy in Settings > Skin.
If an earlier installer deleted a skin, restore it from your backup first.
Changing its saved path cannot restore deleted files.

BMZ Player の F1 > 設定 > スキン >「追加スキンのフォルダーを開く」から
配置先を開いてください。フォント・画像・共有ライブラリを含めて配置し、
「スキン一覧を更新」を押して、画面ごとにスキンを選びます。
ゲーム内に表示されるパスが、現在使用中の data_dir/skins です。
portable 配置やデータ保存先の変更にも対応しています。

この resources/skins はアプリ側のリソースです。配布物と同じパスの
ファイルは更新で上書きされます。Windows Setup は配布物に含まれない
ファイルを残しますが、macOS の更新はアプリ全体を置き換えます。
追加スキンや編集用コピーは「追加スキンのフォルダー」に置いてください。
コピー・編集は各スキンのライセンスに従ってください。

既存スキンは自動移動しません。移す場合は共有ライブラリも含めてコピーし、
設定 > スキンでコピー先を選び直してください。旧インストーラーで消えた
ファイルはバックアップから復元してください。パス変更だけでは復元できません。
