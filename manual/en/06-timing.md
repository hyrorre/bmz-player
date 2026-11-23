# Timing and latency

Use this page if you keep getting FAST or SLOW even when you feel on time, or if the sound seems delayed.
Delay depends on your computer, monitor, and audio device, so adjust the settings for your setup.

It is easier to get good results in this order:

1. Reduce audio delay
2. Reduce display delay
3. Adjust the timing

## 1. Reduce audio delay

Adjust these in Settings → **Audio**. Press **Apply** after changing them.

### Buffer size

This is how much audio is processed at once. A smaller buffer reduces delay, but if it is too small the sound crackles.
Set **Buffer size mode** to **Fixed**, then choose the smallest value that plays cleanly.
We recommend starting with the 128 or 64 preset.

### On Windows

When **Backend** is WASAPI, you can change the **Output mode**.

| Output mode | Description |
| --- | --- |
| Shared | Other apps can play sound at the same time. Delay is larger |
| Shared low latency | Other apps can still play sound, with less delay. Requires Windows 10 or later |
| Exclusive | Only BMZ Player plays sound. This gives the lowest delay, but other apps go silent |

If your audio device supports ASIO, choosing ASIO as the **Backend** may reduce delay further.
With ASIO, the buffer setting in the device driver may take priority.

## 2. Reduce display delay

Adjust these in Settings → **Video**.

| Setting | Description |
| --- | --- |
| Sync mode | Choose whether to prevent screen tearing. Preventing it adds a little display delay |
| Frame latency | **Low latency** reduces display delay. Choose **Stable** if the picture stutters |
| Target FPS | The maximum number of frames drawn per second. 0 means no limit |
| Window mode | Fullscreen may reduce display delay |

## 3. Adjust the timing

### Automatic timing adjustment (recommended)

Press **0** during play to turn on automatic timing adjustment.
Play a few songs with it on, and the visual offset (below) is adjusted little by little to match your timing.
When it feels right, press **0** again to turn it off.

### Visual offset

This shifts when notes appear to reach the judgement line, relative to the sound.
Change **Visual offset** in the **Timing** section of Settings → **Gameplay**. During play, press **3** / **9** to change it by 1 ms.

- If you get many **FAST**, adjust it in the minus direction.
- If you get many **SLOW**, adjust it in the plus direction.

Use this when you want to match your timing to what you see.

### Input offset

This shifts when key and button presses are judged, relative to the sound.
Use it when your input device itself is delayed, such as some controllers.
If you get many SLOW, adjust it in the minus direction. If you get many FAST, adjust it in the plus direction.
This is the opposite direction from the visual offset.

The visual offset alone is usually enough. If you change both by a lot, it becomes hard to tell which one is causing the problem.

## FAST / SLOW display

FAST is shown when you hit too early, and SLOW when you hit too late.
Set **FAST/SLOW display mode** in Settings → **Gameplay** to **Threshold (ms)** to show FAST / SLOW on PGREAT as well.
**FAST/SLOW display threshold** sets how far off a PGREAT must be to show it. Set it to 0 to show it for every judgement.
