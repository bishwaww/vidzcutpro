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

## Phase 2 — your checklist

You need: a vertical or normal video with speech, a logo (PNG with a transparent background is best), a music file (MP3/WAV), and the Sinhala words you want as a headline.

1. Import the video, the logo and the music (**Media → Import**). Click **+** on the video.
2. Open the **Ratio** list under the player and choose **9:16**.
   *You should see:* the picture becomes tall. In **Details** you can switch **Canvas background** to **Blur** (the video fills the empty space, blurred).
3. Click **+** on the logo tile.
   *You should see:* a new **Overlay 1** track above Main, with the logo in the middle of the picture.
4. Click the logo in the player: *a yellow frame with square corners and a round handle appears.* Drag the logo to a corner (a pink guide line shows when it touches the middle or an edge), drag a corner square to make it smaller, drag the round handle to turn it. Hold **Shift** while turning for steps of 15°. One **Ctrl+Z** undoes a whole drag.
5. In the right panel (**Video** tab) change **Opacity** and **Blend mode** and watch the picture.
6. Click the **Text** tab (top left) → click **Sinhala headline**.
   *You should see:* a text on a new **Text 1** track and the words `ශ්‍රී ලංකාව` in the player. Click into the text box on the right and **type your own Sinhala headline**. Try Backspace and the arrow keys. **Tell me if letters split, jump or look wrong.**
7. Change **Font**, **Size**, **Colour**, tick **Outline**, **Shadow / glow**, **Background box**. Double-click the text in the player: the cursor jumps into the text box.
8. Open **Stickers** → click a sticker (for example **Arrow**). Change its **Fill** colour on the right.
9. Click the music tile's **+**.
   *You should see:* an **Audio 1** track with a blue-green waveform. In the right panel (**Audio** tab) set **Volume** and **Fade in / Fade out**.
10. Press **Space** to play. *You should hear the video's speech and the music together; the music fades as you set it.* Use the speaker icon on a track header to mute it.
11. Select a clip and press **Ctrl+C**, move the playhead, **Ctrl+V** (pastes at the playhead). **Ctrl+D** makes a copy right after it. **Ctrl+X** cuts.
12. Press **M** to put a marker on the ruler (press again to remove). Drag a clip near it: it sticks to the marker.
13. Drag a clip from one track to the empty strip below or above the tracks: a new track is made. Lock (padlock) a track: its clips cannot be changed.
14. **Export** (Ctrl+E). Play the MP4: the headline, logo, sticker, video and music must look and sound like the player.

**Please tell me:** anything that looked wrong, especially Sinhala/Tamil letters (in the player, in the exported video, and while typing); anything slow; anything confusing.

## Phase 3 — your checklist

You need: two or three videos (one with speech or music) and a text on the timeline (Text tab → click a style).

**Keyframes**
1. Put a text or a picture on the timeline, click it, and put the playhead at its start. In the right panel (**Video** tab) click the small **diamond** next to **Position X** (it fills in).
2. Move the playhead 2 seconds on, then drag the clip in the player (or change Position X).
   *You should see:* a second diamond on the clip in the timeline. Press **Space**: the clip glides from one place to the other.
3. Right-click a diamond → **How it moves to the next keyframe** → **Bounce ease out** (or any other). Play again and watch the difference.
4. Use the **‹ ›** buttons next to **Transform** to jump between keyframes. Click a filled diamond to remove that keyframe.

**Animations**
5. Select a clip → **Animation** tab → **In** → **Pop** (or Slide, Zoom...). Drag the playhead over the start of the clip.
6. Choose an **Out** one too, and a **Combo** one (it removes In and Out).

**Transitions**
7. Put two clips next to each other. Open the **Transitions** tab: move the mouse over the tiles to see them play. Click **Mix**.
   *You should see:* a round badge on the cut, and the blend when you drag the playhead over it. Click the badge: **Length** and **Easing** appear on the right. Try **Apply to all cuts**. **Delete** removes a selected transition.
8. Try a few more (Clock wipe, Cube, Glitch...). **Tell me which ones look wrong.**

**Speed**
9. Select a video clip → **Speed** tab → click **2×**.
   *You should see:* the clip becomes half as long; later clips follow. Play it: it should sound quick but the voice keeps its pitch (untick **Keep pitch** and the voice goes high like a squeaky tape).
10. Click **Curve**, then **Hero** (or Bullet / Montage): slow motion in the middle, fast outside. Drag a point on the curve; double-click to add a point.
11. Tick **Reverse**: the clip plays backwards (picture and sound). Play it for a few seconds: **tell me if it stutters**.
12. Put the playhead in the middle of a clip and press the **Freeze** button (toolbar) or **Freeze frame here** in the Speed tab: a 3-second still appears and the rest moves later.
13. Cut (**Ctrl+B**) and trim the edges of a sped-up clip: the picture should continue smoothly across the cut.
14. **Export** (Ctrl+E) the whole thing and play the MP4: transitions, animations, speed and sound must match the player.

**Please tell me:** any transition or animation that looks wrong, jerky playback (especially reversed clips and transitions on 1080p), sound problems when changing speed, anything confusing.

## Phase 4 — your checklist

You need: a normal video clip on the timeline (a person, ideally), and if you have one, a green-screen clip and a `.cube` look file.

**Colour**
1. Click a clip → **Adjust** tab. Drag **Exposure**, **Contrast**, **Saturation**, **Temperature** and watch the player.
2. **HSL**: pick a colour dot (for example the green one) and change its Hue / Saturation: only that colour changes.
3. **Curves**: drag on the graph to add a point; drag it up in the middle to brighten. Right-click a point to remove it.
4. **Wheels**: drag inside the **Shadows** wheel towards blue and the **Highlights** wheel towards orange (a film look). Double-click a wheel to reset it.
5. **Look (LUT)**: pick **teal orange** and move **Strength**. If you have a `.cube` file, press **Load a .cube file…**.
6. Press **Ctrl+Z** a few times: every change should undo, a whole slider drag in one step.

**Filters and effects**
7. Open the **Filters** tab (top left, scroll the tab row) and click a few tiles. With a clip selected it changes that clip; with nothing selected it adds a layer on a new **Effects** track.
8. Open **Effects**: hover over the tiles (they move). Click **Blur** or **VHS**: it goes on the selected clip (see the **Effects** tab on the right, where you can change the sliders, switch it off, or reorder). Select nothing, right-click a tile → **Put on the timeline as an effect clip**; drag its edges on the timeline.
9. **Adjustment** tab → **Adjustment layer**: it changes everything below it for its length. Try it with a mask.

**Masks and green screen**
10. Select a clip on an **overlay** track above another video → **Video** tab, scroll to **Mask** → **+ Add mask** → **Circle**. Change **Size**, **Feather**, **Invert**.
11. Put a green-screen clip on an overlay track, tick **Remove background colour**, raise **Intensity** until the green is gone; use **Edge feather** and **Remove spill** to clean the edges.

**Scopes**
12. Press **Scopes** (top right of the player): waveform, vectorscope, histogram. Raise Exposure and watch the waveform go up.

13. **Export** (Ctrl+E) and compare the MP4 with the player: colours, effects, masks and keying must match.

**Please tell me:** colours that look wrong or ugly in a filter, effects that look broken, slowness (especially with Blur or Glow on 1080p video while playing), the green-screen quality, and anything confusing.

## Phase 5 — your checklist

You need: a video clip on the timeline. For captions, also a small `.srt` file (any subtitle file; I can make one for you).

**Text (Sinhala and English)**
1. **Text** tab → **Add text** → pick a style. In the box on the right type `ශ්‍රී ලංකාව`, then English. **Tell me if typing, Backspace or the cursor misbehave in Sinhala** (this is still untested by you).
2. Select some words in the text box (drag over them): under the box, **Selected: "…"** appears. Change the **Colour**, **B**, **I** and **Size** of only those words.
3. Scroll down in the Text tab: **Gradient fill**, **Extra outlines**, **Glow**, **Curve** (drag it: the text bends), **Aa / AA / aa / Title**, **Justify**.
4. **Text animators**: add one (letter by letter, word by word). Play: letters should fly or fade in one after another. Sinhala letter groups (like ශ්‍රී) must stay together.
5. Click the colour square → the picker opens. Press **Pick from the picture**, then click a colour in the player: the text takes that colour. Recently used colours appear under **Recent**.

**Text templates**
6. **Text** tab → **Text templates**. Click **Lower third**: three layers appear (a bar and two texts) and animate in. Click one of its clips on the timeline: all three are selected (they are a group). **Alt + click** selects only one part so you can type your own name. Try **Subscribe button**, **Quote card**, **Countdown 3-2-1** and **Sinhala title card**.
7. Select the group's clips, press **Ctrl+Shift+G** (ungroup) and **Ctrl+G** (group again).

**Shapes and the pen tool**
8. **Stickers** tab → **Shapes**. Add a **Star**, then in the **Shape** tab change **Points** and **Depth**. Add a **Polygon** and change **Sides**.
9. Give a shape a **Gradient** fill (Linear / Radial, two colours, Angle). Turn on **Outline**: try **Inside / Centre / Outside**, **Dashes**, line ends and corners.
10. With an outline on, open **Trim the outline**: set **End** to 50 % and move **Offset**: the line draws only half and slides round. (Keyframes for this come later.)
11. **Stickers → Shapes → Pen tool**. Click 4 points on the player; **drag** when you click one to pull a curve out of it. Click the first point to close the shape (or press **Enter** for an open line, **Esc** to cancel).
12. Select your path → **Shape** tab → **Edit points**: drag points and the round handles; click on the line to add a point; right-click a point to delete it; double-click a point to make it smooth / sharp. Tick **Closed** to fill it.

**Captions**
13. **Captions** tab → **Import** → choose a `.srt` file (or `.vtt`, `.lrc`, `.ass`, `.txt`). Pick a **style** first (21 to choose from), then check the **Caption list**: edit text, **Merge**, **Split** at the cursor, **Find / replace**, **Shift all** by seconds, **Apply style to all**.
14. Choose a **karaoke** style: the words colour in as they are said. Also try **Reveal** and **Single word**. Use Sinhala words.
15. **Captions → Export**: save as `.srt` and `.vtt`, open them in Notepad: times and text must be right.

**Compound clips and timelines**
16. Select two clips that touch on the main track (Ctrl+click), press **Alt+G** (or the **Compound** button). They become one green clip. Play it: it must look and sound the same.
17. **Double-click** the compound clip (or **Open**): a new tab **Compound 01** shows what is inside. Change something there, go back to **Timeline 01**: the change shows.
18. Press **+** next to the tabs for a new empty timeline; double-click a tab name to rename it; right-click a tab for **Make a copy** / **Delete** (a timeline used by a compound clip cannot be deleted).
19. **Ctrl+Z** after making a compound clip: everything returns, and you are taken back to the right tab.
20. **Export** (Ctrl+E): captions, templates, shapes and compound clips in the MP4 must match the player.

**Please tell me:** anything that looks wrong in text or captions (especially Sinhala), pen tool handling that feels awkward, slowness while playing compound clips (they are drawn twice), and anything confusing.
