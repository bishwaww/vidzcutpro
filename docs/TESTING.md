# Testing vidzcut

Two kinds of tests:

1. **Automatic** (run by me before every commit): `cargo test --workspace`, plus formatting and lint checks.
2. **By hand** (you): a short click-by-click list per phase, below.

## Automatic tests so far

| Where | What it proves |
|---|---|
| `vz-core::time` | frame math is exact (23.976 and 29.97 do not drift), snapping, timecode text |
| `vz-core::project` | a project saves and loads back identically (including Sinhala names), newer files are refused politely |
| `vz-ui::settings` | panel sizes are remembered per layout, reset only touches one layout |
| `vz-ui::editor::timeline` | zoom range is sensible |
| `vz-media` | FFmpeg starts from Rust, has the hardware encoders + OpenH264, and has **no** GPL encoders |

## Phase 0 — your checklist

1. Double-click **`run-vidzcut.bat`** in `D:\Projects\vidzcutpro`. (First time: wait a few minutes while it builds.)
   *You should see:* the black and yellow vidzcut home screen with the logo and a big yellow **Create project** card.
2. Click **Create project**.
   *You should see:* the editor: top bar (logo, Menu, project name in the middle, Layout, Export), tabs (Media, Audio, Text, Stickers, Effects, Transitions, Captions, Filters, Adjustment, Templates — scroll the tab row sideways to see them all), a player with a black picture, a Details panel on the right, and a timeline at the bottom with a ruler.
3. Drag the borders between panels (left/right of the player, above the timeline).
   *You should see:* the panels resize.
4. Click **Layout: Default** (top right) → **Player on the right**, then **Big timeline**, then **Reset layout**.
   *You should see:* the panels rearrange. Close vidzcut and open it again: your last layout and sizes are still there.
5. Click different tabs (Text, Effects, …).
   *You should see:* the left list changes and a "Coming in Phase N" note.
6. In the Details panel change **Aspect ratio** to 9:16.
   *You should see:* the black player picture turns tall; Resolution changes to 1080 × 1920.
7. Change the project name at the top to Sinhala, e.g. type with your Sinhala keyboard: `ශ්‍රී ලංකාව`.
   *You should see:* the letters join correctly. **Please try typing, using Backspace and the arrow keys, and tell me if the cursor or Backspace behaves strangely on joined letters** (this is the one thing I could not test without a Sinhala keyboard).
8. Menu → File → **Back to home**.
   *You should see:* the home screen again.
9. Everything with "Coming soon" in a tooltip is deliberately switched off.
10. Check at your usual screen scaling (125 %): text should be sharp and nothing cut off. Tell me if anything looks wrong.
