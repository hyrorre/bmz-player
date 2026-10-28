# 曲スキャンの隠しファイル判定

2026-10-28: Windowsではドット名ではなく隠し属性、macOSではドット名とUF_HIDDEN、Linuxではドット名で判定する。書庫内はOS属性を参照せず、Windowsではドット名を除外しない。macOSとLinuxではドット名を除外する。詳細は[曲の取得元とスキャン](../../docs/song-sources.md#走査と更新)を参照。

Everythingではファイルとroot以下の親フォルダの属性を確認し、読み取れなければ通常走査へフォールバックする。実機のmacOSおよびEverythingとの連携は未確認。
