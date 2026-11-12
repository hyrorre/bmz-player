# Moving from LR2 or beatoraja

This page explains how to bring your songs, scores, and skins from LR2 or beatoraja into BMZ Player.
BMZ Player does not change the original software's files, so you can keep using LR2 or beatoraja afterward.

## 1. Register your song folders

First, register the BMS folders you used with LR2 or beatoraja in BMZ Player.
Add the same folders in Settings → **Library** → **Song Folders (BMS)**.
You do not need to copy the song files.

**Wait until the songs have finished loading before you import scores.**
LR2 scores are matched against the charts BMZ Player has loaded,
so scores for charts that are not loaded yet are not imported.

## 2. Import scores

Open Settings → **Data import** → **Import Scores**.

1. Press **Choose file…** and select the score file from the original software.

   | Software | File to select |
   | --- | --- |
   | LR2 | `LR2files/Database/Score/<player name>.db` |
   | beatoraja | `player/<player name>/score.db` |
   | LR2oraja | `score.db` in the same place as beatoraja |

2. In the list next to it, choose which software the file came from.
3. In **Input device**, choose whether those scores were played on a keyboard or a controller.
4. Press **Import**.

The number of imported scores and the number that could not be imported are shown.

To put imported scores on IR as well, log in as described in [Join the internet ranking (IR)](02-setup.md), then use "Upload all local scores" in Settings → **IR**.

### If imported scores do not appear

BMZ Player saves scores separately for each rule (how timing and gauges work).
Imported scores are saved under the rule that matches the original software:

| Software | Saved under |
| --- | --- |
| beatoraja | beatoraja |
| LR2, LR2oraja | LR2oraja |
| LR2oraja (DX Mode) | DX MODE |

If the scores do not appear on the song select screen, check that **Rule** in Settings → **Gameplay** matches the rule you imported into.
See [Play options](05-play-options.md) for details about rules.

## 3. Import replays (beatoraja)

You can also import beatoraja replays.
In Settings → **Data import**, switch the page at the top to **Replay import**,
then select beatoraja's `player/<player name>` folder, its `replay` folder, or a single `.brd` file.

## 4. Move your skins

Skins you used with beatoraja work in BMZ Player as they are.

1. Open Settings → **Skins** and press **Open additional skins folder**.
2. Copy the skin folder from beatoraja's `skin` folder.
   If the skin uses fonts or images from another folder, copy those as well.
3. Press **Refresh skin list**, then choose a screen on the left and a skin on the right.

Skin options are not carried over. Set them again as needed.

## 5. Review your settings

Key bindings, hi-speed, and timing settings are not carried over.
Set them up using "Set up keys and controllers" in [Setup](02-setup.md) and [Timing and latency](06-timing.md).

### Main differences from LR2 and beatoraja

- All settings are in the settings window opened with **F1**. There is no separate launcher.
- There are five hi-speed modes. See [Play options](05-play-options.md) for which to choose if you want LR2 or beatoraja behavior.
- You can choose whether to play long notes as LN, CN, or HCN. Scores are saved separately for each LN type.
