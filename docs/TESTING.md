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

## Automatic tests added in Phase 1

| Where | What it proves |
|---|---|
| `vz-core` (53 tests) | every edit (split, trim, ripple delete, move, insert) keeps the main track packed; random editing can always be fully undone and redone; a drag is one undo step; save → open gives the same project; moved media is found again; autosave + recovery; the first clip sets the project shape and frame rate |
| `vz-media` (18) | files are probed correctly (size, fps, sound, phone rotation); frames are found by time to the exact frame; playing forward visits every frame once; colours come out right, also for HD (BT.709) footage; thumbnails are made in the background |
| `vz-render` (6) | the right clip shows at the right time; small videos are enlarged; wide videos on a tall canvas get black bars; trimmed clips show the right part of the file |
| `vz-audio` (10) | the 440 Hz test tone comes out as 440 Hz; reading in pieces equals reading at once; seeking back and forward gives the same sound; a cut does not change the sound; a trimmed clip plays the right part; **the real sound card's clock runs in real time** |
| `vz-export` (8 + 1 manual) | the MP4 has the right size, frame rate, H.264 + AAC, duration and A/V start; it matches the preview's colours; edits and ranges are what gets exported; cancelling leaves no file; real footage (manual) |
| `vz-ui` (19) | the timeline's mouse and keyboard logic: split, Q / W, delete, drop slot while dragging, edge trimming step by step (one undo), limits |
| Real-window scripted checks (by me, not in `cargo test`) | click to select, ruler click, Ctrl+B, drag to reorder, edge trim, Ctrl+Z / Ctrl+Shift+Z, Delete, auto-export through the real dialog, playback at 30 pictures/s on a real 1080p file |

## Phase 1 — your checklist

You need: 2–3 videos from your phone or camera (a short one with speech is best), one photo, and ideally one longer video (about 2 minutes).

1. Start vidzcut → **Create project** → click **＋ Import** (Media tab) and choose your files. You can also drag files from Explorer into the window.
   *You should see:* a tile with a picture and a length for each file within a second or two.
2. Click the yellow **+** on a tile (or double-click it).
   *You should see:* the clip on the **Main** track, the picture in the player, and in **Details** the ratio and frame rate changed to match your video.
3. Click on the numbers bar (ruler) above the clips, then drag the white playhead left and right.
   *You should see:* the picture follows your mouse.
4. Press **Space**.
   *You should see and hear:* the video plays with its sound, in sync. Press Space again to pause. **Watch someone's mouth: does the speech match?** Tell me if sound and picture drift apart.
5. Put the playhead in the middle of a clip and press **Ctrl+B** (or the **Split** button).
   *You should see:* the clip cut in two.
6. Drag the left or right **edge** of a clip.
   *You should see:* the clip gets shorter or longer (until the end of the file); later clips follow. One **Ctrl+Z** undoes the whole drag.
7. Drag a clip left or right over its neighbours.
   *You should see:* a yellow line showing where it will land; let go and the order changes.
8. Click a clip and press **Delete**.
   *You should see:* the clip disappears and the gap closes. **Ctrl+Z** brings it back, **Ctrl+Shift+Z** removes it again.
9. Put the playhead inside a clip and press **Q** (removes everything to the left) or **W** (to the right).
10. Drag a tile from the Media tab onto the timeline.
    *You should see:* a yellow line where it will land.
11. Zoom with **Ctrl + mouse wheel**, scroll with the wheel, and press **Fit**.
12. Press **Ctrl+S**, choose a name and place.
    *You should see:* "Project saved", "● Saved" in the top bar. Close vidzcut (it asks about unsaved changes if there are any). Start again: your project is in **Recent projects** on the Home screen; click it: everything is back.
13. **Crash test:** make a change, wait about 40 seconds until the top bar says **Auto saved: hh:mm:ss**, then end vidzcut from Task Manager (End task). Start vidzcut again.
    *You should see:* "vidzcut closed unexpectedly. Restore?" → **Restore** brings your work back.
14. **Missing file test:** rename or move one of your video files in Explorer, then open the project.
    *You should see:* the clip red with "file missing" and a red **Missing** tile. Right-click the tile → **Relink…** → choose the file in its new place.
15. Press **Export** (or **Ctrl+E**), keep **1080p / Recommended**, press **Export**.
    *You should see:* a progress bar, then "Export finished". Press **Open folder** and play the MP4 in your usual player: it should look and sound like the preview and have the same length.
16. Type a Sinhala project name in the top bar (carried over from Phase 0): `ශ්‍රී ලංකාව`. Try Backspace and the arrow keys.
17. Look at everything at your normal 125 % screen scaling.

**Please tell me:** anything that crashed, froze, looked wrong, or felt slow; whether sound and picture stay in sync; and which of your own files (phone model/format) did not import.
