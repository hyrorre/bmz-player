# Development Notes

## Render Sample Hotkeys

When the app window is focused:

- `F1`: show the sample Select scene.
- `F2`: show the sample Play scene.
- `F3`: show the sample Result scene.
- `Escape`: leave the active sample scene and return to the normal app state.

Normal Select input still uses `Enter` or `Space` to start the first chart when a chart exists.

## Playable Sample Chart

The app scans `assets/songs` on startup even when user song roots are empty. This keeps
`assets/songs/sample-playable/sample-playable.bms` available for manual play checks.

Default keyboard bindings:

- `LShift`: Scratch
- `Z S X D C F V`: Key1 through Key7
- `Enter` or `Space`: start the selected chart
