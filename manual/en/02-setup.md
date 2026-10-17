# Setup

These steps take you from downloading BMZ Player to playing your first song.
Work through them from the top. Skip any step you have already done.

## 1. Download BMZ Player

Download the file for your OS from [GitHub Releases](https://github.com/hyrorre/bmz-player/releases).

| OS | File | Notes |
| --- | --- | --- |
| Windows | `...-windows-x64-setup.exe` | The installer. Choose this if you are unsure |
| Windows | `...-windows-x64-portable.zip` | Runs after extracting. Settings and scores are saved in the `data` folder next to it |
| macOS (Apple silicon) | `...-macos-arm64.app.zip` | For M1 and later Macs |
| macOS (Intel) | `...-macos-x64.app.zip` | For Macs with an Intel CPU |
| Linux | `...-linux-x64.tar.gz` | Extract it and run `./bmz-player` |
| Linux | `...-linux-x64.flatpak` | The Flatpak package |

Files such as `sources` and `SHA256SUMS` are not needed to play.

### System requirements

- **Windows**: Windows 10 or later
- **macOS**: macOS 10.13 or later
- **Linux**: x86_64 with Ubuntu 22.04 or equivalent and newer. A graphics driver with Vulkan support is required.

### If macOS says the app cannot be opened

BMZ Player is not notarized by Apple, so macOS may show a warning the first time you open it.
Move the extracted `.app` to a folder such as Applications, then open it by following
[Apple's instructions](https://support.apple.com/en-us/102445).

## 2. Start BMZ Player

When BMZ Player starts, the song select screen appears.
A sample song is included for testing. Try selecting and playing it.

- Move the cursor with **Up / Down**, confirm with **Enter**, and close a folder with **Left**.
- For 7 keys, the keys are **Z S X D C F V** and the turntable is **Left Shift / Left Ctrl**.
- To quit, hold **Esc** at the top level of the song select screen. Inside a folder, each press of Esc closes the folder.

Press **F1** to open the **BMZ Menu**. Pick a category from the list on the left to show its settings on the right.
Settings are saved automatically when you change them, so there is no save button to press.

## 3. Register your song folders

Choose a folder for your BMS files and register it in BMZ Player.

1. Create a folder for BMS anywhere on your computer (for example, `D:\BMS`).
2. Put the songs you downloaded into it, one folder per song. You can group them into subfolders.
3. Open Settings → **Library** → **Song Folders (BMS)**, press **Choose folder…**, and add the folder you created.

The added folder is loaded automatically, and its songs appear on the song select screen.
When you add more songs later, select the folder on the song select screen and press **F5**,
or press **Rescan library** in Settings → Library.

### Where to find songs

- **BMS search sites**: find download pages by song title or artist.
- **BMS events**: event pages such as BMS OF FIGHTERS (BOF) let you download all entries.
- **Difficulty tables**: you can also find songs of the difficulty you want in a difficulty table, described next.

Some songs have extra charts, called "sabun", made separately from the original song.
Put a sabun chart into the original song's folder to make it selectable.

## 4. Use difficulty tables

A difficulty table is a community-made list of BMS charts sorted by difficulty.
Popular difficulty tables are registered in BMZ Player from the start.
Open the **Difficulty Tables** folder on the song select screen to see songs grouped by level.

To use another table, enter its URL in Settings → **Difficulty Tables** and add it.

Difficulty tables also show charts you do not have yet.
Select one of them and confirm to open its download page in your browser.
After you put the song in a song folder and rescan, you can play it.

## 5. Set up keys and controllers

The default bindings are:

| Action | Keyboard |
| --- | --- |
| 7-key keys | Z S X D C F V |
| 7-key turntable | Left Shift (up) / Left Ctrl (down) |
| 14-key 2P keys | M K , L . ; / |
| 14-key 2P turntable | Right Shift / Right Ctrl |
| 9 buttons (PMS) | Z S X D C F V G B |
| E1 / E2 / E3 / E4 | Q / W / E / R |

E1 to E4 are extra buttons, similar to START and SELECT in beatoraja. They are used to change options.

To change bindings, open Settings → **Key Config**.

1. In **Target**, choose the key mode (such as 7K) or screen to change.
2. In **Input Slot**, choose the keyboard or a controller.
3. Press the button next to the action, then press the key or button you want to use.

Press Esc to cancel, or Delete / Backspace to remove a binding.
If your controller does not respond, check that it is connected in Settings → **Input Devices**.

## 6. Check the sound

If there is no sound, or the sound cuts out, check the output in Settings → **Audio**.

1. In **Output device**, choose your speakers or headphones.
2. Press **Apply**. Audio settings take effect only after you press Apply.

If the sound seems delayed, a smaller **Buffer size** may help.
If it is too small, the sound crackles, so choose the smallest value that plays cleanly.

## 7. Adjust the timing

If notes, sound, and your input feel out of sync, adjust the timing.
Press **0** during play to turn on automatic timing adjustment. The offset is corrected as you play a few songs.
See [Timing and latency](06-timing.md) for details.

## 8. Choose skins

These skins are bundled with BMZ Player:

- **m-select**: song select, decide, and result screens, plus play skins (antique, brand-new)
- **Rm-skin**: play screens from 4 keys to 9 buttons
- **Luxe Flat**: song select, decide, and result screens

To change skins, open Settings → **Skins**, choose a screen in **Target** (such as Song Select or the 7K play screen), and pick a skin.
Each skin's own options can be changed on the same page.

### Add other skins

1. Open Settings → Skins and press **Open additional skins folder**.
2. Put the skin there as a whole folder. Extract ZIP files first.
3. Press **Refresh skin list**, then choose the new skin for each screen.

BMZ Player can load beatoraja skins (JSON and Lua) and LR2 skins (CSV).

## 9. Join the internet ranking (IR)

With an IR account, your scores are registered in an online ranking where you can compare them with other players. This is optional.

1. Create an account with **Register** on [BMZ IR](https://bmz-player.hyrorre.workers.dev/en).
2. In BMZ Player, open Settings → **IR**, enter your email and password in the BMZ IR section, and press **Log in**.

After you log in, your scores are submitted automatically.
BMZ Player also supports rianIR and BMS-IR, and you can log in to them on the same page.

### Uploading and downloading past scores

Settings → **IR** → "Bulk score operations" offers two actions.

- "Upload all local scores": sends the scores stored on this PC to your Primary IR. Use it to put scores imported from LR2 or beatoraja on IR.
- "Download scores from IR": imports your own scores stored on IR into this PC. Use it to bring scores played on another PC up to date.

Pressing a button opens a confirmation window. For the upload, it shows how many scores will be sent and a rough minimum time. Press "Start" to begin.

- With many scores, it can take from a few minutes to well over half an hour. Progress is shown on the same page, and the work continues even if you close the settings window.
- Press "Cancel" to stop at any time. Running it again continues where it stopped.
- You cannot switch profiles while it is running.
- Bulk operations work only for your Primary IR. Uploading local scores is available for BMZ IR only, and downloading works with every IR except rianIR. The buttons are disabled when you are not logged in.

You are notified when it finishes.

### Checking and resending scores that failed to send

Below the bulk operations is the "IR send queue", a list of scores that are waiting, being sent, or failed. It first shows only failed scores. Change the filter with "Show", and press "Refresh" to update the list.

- Each row shows the type, chart, status, number of attempts, next retry time, and the last error.
- "Resend" on a failed row sends that score again right away.
- "Resend all failed" sends the failed scores again one by one, after a confirmation window.

Failed scores are also retried automatically after a delay. Use these buttons when you want to resend right away after a connection problem is fixed. A score that another process is already sending is sent after that process finishes.

Setup is complete. See [Basic controls](04-controls.md) for how to play.
