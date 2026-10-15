# Play options

These options change how hard a song is and how it looks.
Change them in the option panel on the song select screen (shown while holding **E1** and similar buttons), or in Settings → **Gameplay**.

If you are unsure, you can start playing with the default settings.

## Gauge

The gauge shows how well you are doing. It rises with good timing and drops when you miss.
Each gauge type changes how fast it rises and drops, and what counts as a clear.

| Gauge | Description |
| --- | --- |
| ASSIST EASY | The most forgiving gauge. A clear is recorded below EASY |
| EASY | Drops more slowly than NORMAL |
| NORMAL | The standard gauge |
| HARD | Starts full. The song ends immediately (a "fail") when it reaches 0 |
| EX-HARD | Drops even faster than HARD |
| HAZARD | You fail as soon as your combo breaks |

With ASSIST EASY, EASY, and NORMAL, you clear if the gauge is at or above the clear line at the end of the song.
With HARD, EX-HARD, and HAZARD, you clear if you reach the end without failing.

If you are new, start with NORMAL. For songs that are hard to clear, try EASY or ASSIST EASY.

### Gauge auto shift (GAS)

GAS lets you keep playing on another gauge when you fail. It is OFF by default.

| Mode | Description |
| --- | --- |
| OFF | No shifting. Failing on HARD or similar ends the song immediately |
| CONTINUE | You can play to the end after failing. The selected gauge is recorded |
| HARD->GROOVE | After failing on HARD or similar, you continue on NORMAL |
| BEST CLEAR | All gauges are tracked at once, and the highest gauge you cleared is recorded |
| SELECT UNDER | Your selected gauge and the gauges below it are tracked, and the highest one you cleared is recorded |

For example, with BEST CLEAR, if you can no longer clear EX-HARD partway through, a clear on HARD or NORMAL is still recorded.
Use **GAS minimum gauge** to choose the lowest gauge that can be recorded.

## Arrangement (RANDOM and others)

Arrangements change which lanes the notes fall in, so you can practice the same chart in different ways.

| Arrangement | Description |
| --- | --- |
| OFF | Notes fall as written in the chart |
| MIRROR | Flips the lanes left to right |
| RANDOM | Shuffles the lanes for each play |
| R-RANDOM | Rotates the lanes sideways while keeping their order |
| S-RANDOM | Sends each note to a random lane |
| SPIRAL | Gradually rotates the lane order as the song progresses |
| H-RANDOM | Like S-RANDOM, but avoids short repeats in the same lane |
| ALL-SCR | Moves as many notes as possible to the turntable |
| RANDOM-EX / S-RANDOM-EX | Also includes the turntable in the shuffle |
| F-RANDOM | Splits the keys into left and right halves and shuffles within each half. In 7 keys, the center key does not move |
| MF-RANDOM | F-RANDOM, then flipped left to right |

Arrangements that move notes one by one, such as S-RANDOM, can change a chart a lot from its original shape.

## Hi-speed

Hi-speed is how fast notes fall. Faster notes are spread farther apart and easier to read, but you have less time to react.
Change it during play with **Left / Right**, or by **holding E1 and pressing keys**.

### Green number

The green number shows how long a note takes to travel from the top of the screen to the judgement line.
It is the time in milliseconds multiplied by 0.6. For example, a green number of 300 is 0.5 seconds. A smaller number looks faster.
Songs with different BPMs look equally fast when their green numbers are the same.

### Hi-speed modes (HS configuration)

| Mode | Description |
| --- | --- |
| NORMAL | Choose from 20 levels. Each level has a fixed green number |
| CLASSIC | Set the multiplier directly. This works like normal hi-speed in LR2 and beatoraja |
| FLOATING | Set a green number, and the multiplier is chosen to match each song's BPM |
| NORMAL+FLOATING | Switch between NORMAL and FLOATING during play |
| CLASSIC+FLOATING | Switch between CLASSIC and FLOATING during play (the default) |
| LR2 | Set the multiplier directly, like CLASSIC (the value is shared with CLASSIC). With HS-FIX, the multiplier means the speed at BPM 150 (as in LR2) |

In the switchable modes, **press E1 and E2 together** during play to switch.
In FLOATING, **hold E2 and press keys or the turntable** to change the green number.

FLOATING is useful if you do not want to readjust the speed for every song.

### HS-FIX

When the BPM changes during a song, HS-FIX chooses which BPM is used to set the speed.
For example, "max BPM" sets the speed for the fastest part of the song.

## Lane covers

Lane covers hide part of the lane to adjust where notes are visible.
Use them to make fast notes easier to read, or to match where notes appear to your eye level.

| Type | Description |
| --- | --- |
| SUDDEN+ | Hides the top of the lane |
| HIDDEN+ | Hides the bottom of the lane, near the judgement line |
| LIFT | Raises the judgement line |

Enable the covers you want for each key mode in the **Display** section of Settings → **Gameplay**.
During play, move them with **Up / Down**, or by **holding E1 and moving the turntable**.
One press of Up / Down moves the cover by 1% of the lane, and holding the key keeps it moving. **Hold Shift** to move it by 0.1% at a time (the right Shift key is handy next to the arrow keys). You can change how far it moves and how long to hold before it keeps moving in Settings → **Input**.
If an enabled cover is set to 0, it is set to 150 (15% of the lane) when play starts.
**Press E1 twice** to show or hide SUDDEN+.

Turn on **Adjust HS to the current BPM when lane cover changes** to keep the green number the same when you move a cover.

## LN mode

This sets how long notes (notes you hold down) are judged.

| Type | Description |
| --- | --- |
| LN | Only the start is judged. Release timing does not matter |
| CN | Both the start and the release are judged |
| HCN | Like CN, and the gauge also rises or drops while you hold the note |

Settings starting with "AUTO" follow the chart if it specifies a type, and use your choice otherwise.
Settings starting with "FORCE" always use your choice, regardless of the chart.

You can also press **3** on the song select screen to switch. Scores are saved separately for each LN mode.

## Rule

The rule sets the timing windows, gauges, and score calculation together. Scores are saved separately for each rule.

| Rule | Description |
| --- | --- |
| beatoraja | The same rules as beatoraja (the default) |
| LR2oraja | Rules close to LR2. Behaves the same as LR2oraja Endless Dream |
| DX MODE | The same rules as DX MODE in LR2oraja Endless Dream |

To see scores imported from LR2 or beatoraja, use the rule they were imported into.

## Assist

Assist options help you play, for example by widening the timing windows or showing BPM changes in advance.
Set them in the panel shown while holding **E2** on the song select screen.

When an assist takes effect, a clear is recorded as an ASSIST clear, and your score is neither saved nor submitted to IR.
Display-only features, such as showing the judgeable area (JUDGE AREA) or marking added notes (MARK NOTE), do not affect your records.
An assist that has no effect, such as BPM GUIDE on a chart with a constant BPM, does not affect your records either.

## Target

The target is the score you compare against during play.
You can choose a rank goal such as AAA, a top IR score, a rival's score, and more.

## Play types

| Type | Description |
| --- | --- |
| NORMAL | Normal play |
| PRACTICE | Practice with a chosen section and speed. Scores are not saved |
| AUTOPLAY | Plays automatically. Scores are not saved |
| AUTOPLAY BATTLE | Plays automatically on the battle screen. Scores are not saved |
| G-BATTLE | Play against a saved replay or an IR score |

## When scores are not saved

For the following plays, scores, clear lamps, and replays are not saved, and nothing is submitted to IR:

- Autoplay, replay playback, and practice
- Plays with a converted key mode (such as playing a 7-key chart as 9 buttons)

When an assist takes effect, only the ASSIST clear lamp is saved.
