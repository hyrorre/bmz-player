# Troubleshooting

Common problems and how to fix them. If nothing here helps, report the problem as described at the end of this page.

## Starting and display

### BMZ Player does not start or closes right away

- **Windows**: update your graphics driver. Older versions of Windows may need a Vulkan driver.
- **macOS**: if macOS says the app cannot be opened, see "If macOS says the app cannot be opened" in [Setup](02-setup.md).
- **Linux**: a graphics driver with Vulkan support is required. For the tar.gz version, start `./bmz-player` in the extracted folder.

### The picture stutters or is not smooth

In Settings → **Video**, set **Frame latency** to **Stable**, or set **Target FPS** to your monitor's refresh rate.
Closing other apps may also help.

### A skin is not shown or looks broken

Check Settings → **Skins** for a loading error on that skin.
If some of the skin's folders (such as fonts or images) are missing, it is not shown correctly.
Place the skin as a whole folder, with the same structure it was distributed in.

## Songs and difficulty tables

### Songs do not appear

- Check that the folder with your songs is registered in Settings → **Library**.
- After adding songs, press **F5** on the song select screen, or press **Rescan library**.
- Check that each song is in its own folder. The chart files (such as .bms) and the audio files must be in the same folder.
- Check that a filter is not active on the song select screen, such as **0** (difficulty) or **1** (key mode).

### Songs are missing from a difficulty table, or the table is not updated

Select the difficulty table folder on the song select screen and press **F5** to fetch the table again.
A chart listed in a table cannot be played unless you have it. Select a chart you do not have and confirm to open its download page in your browser.

## Sound

### There is no sound

1. In Settings → **Audio**, check that your device is selected in **Output device**.
2. Press **Refresh device list**, select the device again, and press **Apply**.
3. In **Volume** on the same page, check that the master volume and other volumes are not 0.

If the sound stops after you unplug and reconnect a device, pressing **Apply** may fix it.

### The sound crackles or cuts out

Increase **Buffer size** in Settings → **Audio**.

### The sound seems delayed

See [Timing and latency](06-timing.md).

## Input

### The keyboard or controller does not respond

- In Settings → **Input Devices**, check that your controller is shown as connected.
- In Settings → **Key Config**, check the bindings for the key mode you are playing (such as 7K). Each key mode has its own bindings.
- A warning icon next to a binding means the same key is also used by another action.

### The turntable is hard to trigger or moves too far

Adjust **Analog scratch** and **Analog sensitivity** in Settings → **Input Devices**.

## Scores and IR

### Scores are not saved

Scores from autoplay, replay playback, practice, and plays with a converted key mode are not saved.
When an assist takes effect, only the ASSIST clear lamp is saved.

### Imported or earlier scores do not appear

Scores are saved separately for each combination of rule and LN mode.
Set **Rule** and **LN mode** in Settings → **Gameplay** back to the settings you used for those scores.

### Scores are not submitted to IR

- Check in Settings → **IR** that you are shown as logged in.
- Scores played while offline are submitted automatically once you are back online.
- To check scores that failed to send, open Settings → **IR** → "Open send queue", and use "Resend" to send them again.

## Where logs and data are stored

Press **F1** to open the BMZ Menu, go to **General**, and use the buttons under **Open directory** to open each folder.

| Button | Contents |
| --- | --- |
| data_dir | Settings, scores, replays, and skins you added |
| logs_dir | Logs |
| cache_dir | Temporary files. They are recreated as needed if you delete them |
| resource_dir | Files bundled with BMZ Player. They are overwritten by updates |

Back up data_dir to keep your settings and scores safe.
Quit BMZ Player before making a backup.

## Report a problem

Report problems on [GitHub Issues](https://github.com/hyrorre/bmz-player/issues).
The following information helps us find the cause:

- Your OS and BMZ Player version
- What you did and what happened
- The log files in logs_dir
- Screenshots if useful (press **F12** to save one)
