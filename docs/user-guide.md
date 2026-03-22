# Layers — User Guide

Layers is a spatial digital audio workstation (DAW) where you arrange audio clips, MIDI instruments, and effects on a free-form canvas. Instead of a traditional track-based layout, everything lives on an open 2D workspace — drag clips wherever you want, overlap them with effect regions, and build your mix visually.

---

## Table of Contents

- [Getting Started](#getting-started)
  - [Interface Overview](#interface-overview)
  - [Your First Project](#your-first-project)
- [Canvas Navigation](#canvas-navigation)
  - [Panning & Zooming](#panning--zooming)
  - [Grid & Snapping](#grid--snapping)
- [Working with Audio](#working-with-audio)
  - [Importing Samples](#importing-samples)
  - [Moving & Resizing Clips](#moving--resizing-clips)
  - [Volume, Pan & Color](#volume-pan--color)
  - [Fades](#fades)
  - [Splitting & Reversing](#splitting--reversing)
  - [Warp Modes](#warp-modes)
- [Working with MIDI](#working-with-midi)
  - [Creating MIDI Clips](#creating-midi-clips)
  - [Piano Roll Editing](#piano-roll-editing)
  - [Note Selection & Editing](#note-selection--editing)
  - [Velocity Editing](#velocity-editing)
- [Instruments & Effects](#instruments--effects)
  - [Loading VST3 Instruments](#loading-vst3-instruments)
  - [Effect Regions](#effect-regions)
  - [Per-Sample Effect Chains](#per-sample-effect-chains)
  - [Plugin Management](#plugin-management)
- [Automation](#automation)
  - [Volume & Pan Automation](#volume--pan-automation)
  - [Editing Automation Points](#editing-automation-points)
- [Playback & Recording](#playback--recording)
  - [Transport Controls](#transport-controls)
  - [Computer MIDI Keyboard](#computer-midi-keyboard)
  - [Metronome](#metronome)
  - [Input Monitoring](#input-monitoring)
  - [Recording Audio](#recording-audio)
- [Regions](#regions)
  - [Loop Regions](#loop-regions)
  - [Export / Render Regions](#export--render-regions)
- [Text Notes](#text-notes)
- [Components](#components)
  - [Creating Components](#creating-components)
  - [Using Instances](#using-instances)
- [Sample Browser](#sample-browser)
- [Settings & Customization](#settings--customization)
- [Keyboard Shortcuts](#keyboard-shortcuts)
- [Mouse Controls](#mouse-controls)

---

## Getting Started

### Interface Overview

The Layers interface is made up of a few key areas:

- **Canvas** — The large central workspace where you place and arrange audio clips, MIDI clips, instruments, effects, and regions. You can pan and zoom freely.
- **Transport Panel** — Located at the bottom center. Contains play/pause, record, metronome, optional computer MIDI keyboard, and the BPM display.
- **Sample Browser** — A collapsible sidebar on the left for browsing audio files and VST3 plugins. Toggle it with `⌘B`.
- **Properties Panel** — Appears on the right when you select a clip. Shows volume, pan, pitch, warp mode, and other clip-specific settings.
- **Command Palette** — Press `⌘K` to search and run any command quickly.

### Your First Project

1. **Set your tempo** — Click the BPM display in the transport panel and type a new value, or drag vertically to adjust.
2. **Open the sample browser** — Press `⌘B` to reveal the sidebar, then click the **+** button to add a folder of audio files.
3. **Add audio** — Drag a file from the browser onto the canvas. A waveform clip appears.
4. **Arrange** — Drag clips to position them. They snap to the grid by default.
5. **Play** — Press `Space` to start playback.
6. **Save** — Press `⌘S` to save your project.

---

## Canvas Navigation

### Panning & Zooming

| Action | How |
|---|---|
| Pan the view | Scroll (trackpad or mouse wheel) |
| Pan freely | Middle-click and drag |
| Zoom in/out | `⌘+` / `⌘-`, or `⌘`+scroll wheel |
| Reset zoom | `⌘0` |

Zooming is centered on your cursor position, so you can zoom into a specific area by pointing at it first.

### Grid & Snapping

The grid helps you align clips to musical time. By default, it adapts to your zoom level (Adaptive mode), showing more detail as you zoom in.

| Shortcut | Action |
|---|---|
| `⌘1` | Narrow the grid (finer divisions) |
| `⌘2` | Widen the grid (coarser divisions) |
| `⌘3` | Toggle triplet grid |
| `⌘4` | Toggle snap to grid |

You can also switch to **Fixed** grid mode in Settings, where you choose a specific subdivision (from 8 bars down to 1/32 notes).

> **Tip:** Hold nothing extra while dragging — snapping is on by default. Toggle it off with `⌘4` for free placement.

---

## Working with Audio

### Importing Samples

Drag audio files from the **Sample Browser** (`⌘B`) onto the canvas. Supported formats:

- WAV, MP3, OGG, FLAC, AAC, M4A, MP4

You can also add entire folders to the browser with `⇧⌘A` (or the **+** button in the browser).

### Moving & Resizing Clips

- **Move** — Click and drag a clip to reposition it. Clips snap to the grid.
- **Resize** — Drag the left or right edge of a clip to trim it.
- **Multi-select** — Click empty space and drag to create a selection box, or `Shift`+click to add clips to your selection.
- **Nudge** — Use arrow keys to move selected clips by one grid step. `Shift`+arrows for larger steps (4 beats horizontal, 1 row vertical).
- **Duplicate** — `⌘D`, or hold `Alt` while dragging to duplicate-and-move.
- **Disable/Enable** — Press `0` to toggle a clip's playback on or off (disabled clips appear dimmed).

### Volume, Pan & Color

When a clip is selected, the **Properties Panel** on the right shows:

- **Volume** — Adjust the gain fader. Double-click the value to type a dB number. Double-click the knob to reset to 0 dB.
- **Pan** — Adjust stereo position. Double-click the knob to reset to center.
- **Color** — Right-click a clip and choose a color from the context menu for visual organization.

Use arrow keys while a fader or knob is focused for fine adjustment. Hold `Shift` for even finer steps.

### Fades

Each audio clip has fade-in and fade-out handles at its top corners. Drag these handles to set the fade length. Fades use adjustable curves — you can control the curve shape for smooth or sharp transitions.

Fades are applied during both playback and rendering.

> **Tip:** Enable **Auto Fades** in Settings to automatically add short fades to every new clip, which helps avoid clicks at clip boundaries.

### Splitting & Reversing

- **Split** — Select a clip and press `⌘E` to split it at the playhead position. This creates two separate clips from the original.
- **Reverse** — Select a clip and click the **Reverse** button in the Properties Panel to flip playback direction. The waveform display updates to show the reversed audio. Reversing is undoable.

### Warp Modes

Warp modes control how a clip's playback relates to the project tempo. Select a clip and choose a mode in the Properties Panel:

| Mode | What it does |
|---|---|
| **Off** | Plays at the original sample rate, ignoring project BPM |
| **Re-Pitch** | Time-stretches the clip to match project BPM. Set the clip's original BPM in the Properties Panel and Layers handles the rest. |
| **Semitone** | Pitch-shifts the clip by a number of semitones (–24 to +24) with automatic time compensation |

Double-click the pitch or sample BPM fields in the Properties Panel to type exact values.

---

## Working with MIDI

### Creating MIDI Clips

Create a MIDI clip from the right-click context menu on the canvas. A new clip appears with a default piano roll range (C0–C8).

MIDI clips are color-coded differently from audio clips so you can tell them apart at a glance.

### Piano Roll Editing

Double-click a MIDI clip (or zoom in far enough) to enter the **piano roll editor**:

- The vertical axis represents pitch, with a piano keyboard on the left.
- The horizontal axis represents time, aligned to the grid.
- White lines mark octave boundaries. Black key rows are subtly shaded.

**Creating notes:** Click and drag in an empty area of the piano roll to draw a new note.

**Moving notes:** Click and drag a note's body to reposition it.

**Resizing notes:** Drag the left or right edge of a note to change its start time or duration.

Press `Escape` to exit the piano roll and return to the canvas.

### Note Selection & Editing

| Action | How |
|---|---|
| Select a note | Click on it |
| Add to selection | `Shift`+click |
| Select multiple | Drag a selection box in empty space |
| Move selected notes | Drag, or use `Left`/`Right` arrow keys |
| Transpose ±1 semitone | `Up`/`Down` arrow keys |
| Transpose ±1 octave | `Shift`+`Up`/`Down` |
| Resize duration | `Shift`+`Left`/`Right` |
| Duplicate notes | `⌘D`, or `Alt`+click |
| Delete notes | `Delete` or `Backspace` |

Overlapping notes on the same pitch are automatically trimmed so they don't conflict.

### Velocity Editing

Below the piano roll is the **velocity lane**, which shows a vertical stem for each note representing its velocity (0–127).

- Drag the circle handle at the top of a velocity stem to adjust that note's velocity.
- Higher velocity = louder and visually brighter. Lower velocity = softer and darker.

The velocity lane is resizable — drag its top border to give yourself more or less room.

---

## Instruments & Effects

### Loading VST3 Instruments

1. Open the **Sample Browser** (`⌘B`) and expand the **Plugins** section.
2. Drag a VST3 instrument onto the canvas to create an **Instrument Region**.
3. The instrument region automatically includes a MIDI clip — double-click the MIDI clip to write notes that the instrument will play.
4. Double-click the instrument block to open the plugin's native GUI.

Instrument regions are shown with a purple dashed border and an "INST" badge.

### Effect Regions

Effect regions are spatial zones on the canvas. Any audio clip that overlaps with an effect region gets processed through its plugin chain.

1. Create an effect region from the right-click context menu.
2. Drag plugins from the browser into the region, or add them from the context menu.
3. Resize the region to cover the clips you want to process.
4. Multiple plugins in a region are chained left-to-right.

Effect regions are shown with an "FX" badge and can be renamed with `⌘R`.

> **Tip:** This spatial approach means you can apply effects to specific sections of your arrangement just by positioning the region — no routing needed.

### Per-Sample Effect Chains

Every audio clip has its own effect chain, separate from spatial effect regions. With a clip selected, look for the **Effect Chain** section at the top of the Properties Panel:

- **Add a plugin** — Drag a VST3 from the **Effects** category in the browser onto the clip, or click the **Add Plugin** button in the Properties Panel.
- **Bypass** — Click the bypass button on a slot to temporarily disable that plugin.
- **Delete** — Click the delete button to remove a plugin from the chain.
- **Reorder** — Drag a plugin slot to change the processing order.
- **Shared chains** — If multiple clips reference the same effect chain, editing one affects all. Use **Detach** to create an independent copy for just that clip.

### Plugin Management

- **Open plugin GUI** — Double-click a plugin block (in an effect region or instrument region).
- **Bypass** — Toggle bypass on a plugin to temporarily disable it.
- **Remove** — Delete the plugin block.
- Plugin state (parameters, presets) is saved with the project.

---

## Automation

### Volume & Pan Automation

Automation lets you change a clip's volume or pan over time. To enable automation on a clip:

1. Select the clip and toggle automation mode (via context menu or command palette).
2. The clip display switches from waveform view to an automation curve overlay.

You can automate two parameters per clip:

| Parameter | Range | Default |
|---|---|---|
| Volume | Silent → +12 dB | 0 dB (center) |
| Pan | Full left → Full right | Center |

### Editing Automation Points

- **Add a point** — Click anywhere on the automation line.
- **Move a point** — Click and drag it to a new position (time and value).
- **Delete a point** — Right-click on a point to remove it.

Points are connected with straight lines (linear interpolation). The automation is applied per-clip, and the values are normalized across the clip's duration.

---

## Playback & Recording

### Transport Controls

The transport panel sits at the bottom center of the screen:

| Button | Action | Shortcut |
|---|---|---|
| ▶ / ❚❚ | Play / Pause | `Space` |
| ● | Start / Stop recording | Click |
| Metronome icon | Toggle metronome | Click |
| BPM display | Set tempo | Click to drag, double-click to type |
| Keyboard icon | Computer MIDI keyboard | Click to arm / disarm |
| Headphones icon | Toggle input monitoring | Click |

Valid BPM range: 20–999.

### Computer MIDI Keyboard

When the piano-keys control in the transport is **on**, you can preview the **selected instrument** (a loaded VST3 in an instrument region) from the typing keyboard:

| Keys | Action |
|---|---|
| **A S D F G H J K** | White keys (one octave, C through the next C) |
| **W E T Y U** | Black keys (sharps in that octave) |
| **Z** / **X** | Octave down / up |
| **C** / **V** | Velocity down / up (without **⌘** held) |

**Space** still toggles playback (it is not a sustain pedal). Use the **Layers** tab in the sample browser to pick an instrument on the canvas: the view centers on it and it becomes the keyboard target when exactly one instrument region is implied by selection.

### Metronome

Click the metronome button in the transport panel to toggle it on (indicated by a brighter dot).

- **Downbeats** (beat 1 of each bar): Higher-pitched click (1000 Hz)
- **Other beats**: Lower-pitched click (800 Hz)

The metronome only sounds during playback.

### Input Monitoring

Click the **headphones icon** in the transport panel to enable input monitoring. When active, your microphone input is routed directly to the output so you can hear yourself in real time. If you are positioned inside an effect region, the signal is also processed through that region's plugin chain.

Input monitoring works independently of recording — you can monitor without recording.

### Recording Audio

1. Make sure your audio input device is selected in **Settings** (`⌘,`).
2. Click the **Record** button in the transport panel.
3. Playback starts and audio is captured from your input.
4. Click Record again (or press `Space`) to stop.
5. A new audio clip is created on the canvas from your recording.

Recordings are captured at 48 kHz in stereo.

---

## Regions

### Loop Regions

A loop region defines a section of the timeline that repeats during playback.

- **Create** — Select a clip and press `⌘L`, or use the right-click menu.
- **Toggle** — Press `0` while a loop region is selected to enable/disable it.
- **Resize** — Drag the edges to adjust the loop boundaries.

Active loop regions show a "LOOP" badge at the top left. When playback reaches the end of a loop region, it seamlessly jumps back to the start.

### Export / Render Regions

Render regions define a section of your arrangement to export as an audio file.

- **Create** — Use the right-click context menu.
- **Render** — Click the **Render** pill button at the top of the region.
- **Resize** — Drag edges to set what gets exported.

Clicking Render opens a file dialog where you choose the output location. The rendered file is a 48 kHz, 32-bit float WAV that includes all audio processing — volume, pan, fades, warp, automation, and effects.

---

## Text Notes

Text notes are free-form text boxes you can place anywhere on the canvas — useful for labelling sections, leaving reminders, or annotating your arrangement.

- **Create** — Right-click on the canvas and choose **Add Text Note** from the context menu.
- **Edit** — Click a text note to select it, then start typing. Notes support multi-line text; press `Enter` to add a new line.
- **Navigate** — Use `Left`/`Right` arrow keys to move the cursor, `Up`/`Down` to jump between lines, and `Home`/`End` to jump to the beginning or end of a line.
- **Move** — Drag the note to reposition it on the canvas.
- **Delete** — Select the note and press `Delete` or `Backspace`.
- **Style** — Each text note has a customizable background color, font size, and border radius, set from the context menu.

Text note content and position are saved with the project.

---

## Components

Components let you save a group of clips as a reusable block.

### Creating Components

1. Select the clips you want to group.
2. Create a component from the context menu or command palette.
3. Give it a name.

The original clips become the component's **definition**. A component is indicated by a diamond badge.

### Using Instances

Once a component is defined, you can place **instances** of it elsewhere on the canvas:

- Instances show semi-transparent "ghost" waveforms so you can see what's inside.
- Instances are linked to the original definition.
- A lock icon indicates the instance is read-only — to edit, modify the original component.
- Move and resize instances freely.

Double-click a component to enter its edit mode and modify the original definition.

---

## Sample Browser

Open the sample browser with `⌘B`. It appears as a sidebar on the left side of the screen.

**Features:**

- Browse folders of audio samples with expandable directory trees.
- Drag and drop files directly onto the canvas.
- Add sample folders with the **+** button or `⇧⌘A`.
- Scroll through files with the mouse wheel.
- Resize the browser by dragging its right edge (150–600px).

The sidebar has four categories:

- **Layers** — A Figma-style hierarchical view of everything on your canvas: instruments with their nested MIDI clips, audio clips (waveforms), effect regions with nested plugin blocks, and text notes. Click any row to select the entity and center the canvas on it. Click the chevron (or the row itself for parent items) to expand or collapse children. Use `⌘[` / `⌘]` to reorder items up and down in the list. **Double-click a row** (or press `⌘R`) to rename it inline — type a new name and press `Enter` to confirm, or `Escape` to cancel. Press `⌘Delete` while renaming to clear the text at once. **Right-click a row** to rename or delete it. Layer order and expand/collapse state are saved with the project.
- **Samples** — Audio file browser with expandable folders.
- **Instruments** — Available VST3 instrument plugins.
- **Effects** — Available VST3 effect plugins.

Drag plugins from the Instruments or Effects categories onto the canvas to create instrument regions or plugin blocks.

Your sample library folders are saved globally and persist between sessions.

---

## Settings & Customization

Open Settings with `⌘,` (Cmd+Comma).

### Audio Devices

- **Driver** — Select your audio driver / host API.
- **Input Device** — Choose your recording input.
- **Output Device** — Choose your playback output.
- **Buffer Size** — Set the audio buffer (32–2048 samples). Smaller values reduce latency; larger values improve stability. Changing this restarts the audio engine.

### Grid

| Setting | Description |
|---|---|
| Grid Enabled | Show or hide grid lines |
| Snap to Grid | Clips snap to grid when moved |
| Grid Mode | Adaptive (auto-scales with zoom) or Fixed (choose a subdivision) |
| Triplet Grid | Enable triplet subdivisions |
| Vertical Snap | Snap to vertical grid lines |
| Auto Fades | Automatically add fades to new clips |

### Appearance

| Setting | Description |
|---|---|
| Primary Hue | Shift the overall color theme (0–360°) |
| Brightness | Adjust UI brightness |
| Color Intensity | Adjust color saturation |
| Grid Line Intensity | Control how visible the grid lines are |
| Theme Preset | Choose "Default" or "Ableton" (darker) |

### Other

- **Dev Mode** — Show additional debug information.
- All settings are saved to `~/.layers/settings.json` and loaded on startup.

---

## Keyboard Shortcuts

### General

| Shortcut | Action |
|---|---|
| `Space` | Play / Pause |
| `⌘S` | Save project |
| `⌘Z` | Undo |
| `⇧⌘Z` | Redo |
| `⌘A` | Select all |
| `⌘C` | Copy |
| `⌘V` | Paste |
| `⌘D` | Duplicate |
| `Delete` / `Backspace` | Delete selection |
| `Escape` | Clear selection / exit edit mode / close menus |

### View & Navigation

| Shortcut | Action |
|---|---|
| `⌘B` | Toggle sample browser |
| `⌘K` / `⌘T` / `⌘P` | Open command palette |
| `⌘,` | Open settings |
| `⌘+` | Zoom in |
| `⌘-` | Zoom out |
| `⌘0` | Reset zoom |

### Grid

| Shortcut | Action |
|---|---|
| `⌘1` | Narrow grid |
| `⌘2` | Widen grid |
| `⌘3` | Toggle triplet grid |
| `⌘4` | Toggle snap to grid |

### Layers Panel

| Shortcut | Action |
|---|---|
| `⌘[` | Move selected layer up |
| `⌘]` | Move selected layer down |

### Audio Clips

| Shortcut | Action |
|---|---|
| `⌘E` | Split clip at playhead |
| `⌘L` | Add loop region to clip |
| `⌘R` | Rename selected clip or effect region |
| `⌘Delete` | Clear text while in rename mode |
| `⇧⌘A` | Add folder to sample browser |
| `0` | Toggle clip or loop region enabled/disabled |
| Arrow keys | Nudge selection by grid step |
| `Shift`+arrows | Large nudge |
| `Alt`+drag | Duplicate and move |

### MIDI Editing (inside piano roll)

| Shortcut | Action |
|---|---|
| `Left` / `Right` | Move selected notes horizontally |
| `Up` / `Down` | Transpose ±1 semitone |
| `Shift`+`Up` / `Down` | Transpose ±1 octave |
| `Shift`+`Left` / `Right` | Resize note duration |
| `⌘D` | Duplicate selected notes |
| `Delete` / `Backspace` | Delete selected notes |
| `Escape` | Exit piano roll |

### Properties Panel (when focused)

| Shortcut | Action |
|---|---|
| `Up` / `Down` | Adjust focused parameter |
| `Shift`+`Up` / `Down` | Fine adjust focused parameter |
| Double-click value | Type exact value |
| Double-click knob | Reset to default |

---

## Mouse Controls

### Left Mouse Button

| Action | What it does |
|---|---|
| Click on clip | Select it (deselects others) |
| `Shift`+click | Add clip to selection |
| Drag clip | Move selected clips |
| `Alt`+drag clip | Duplicate and move |
| Drag clip edge | Resize (trim) clip |
| Drag fade handle | Adjust fade in/out length |
| Drag empty space | Box select |
| Double-click MIDI clip | Enter piano roll editor |
| Double-click plugin | Open plugin GUI |
| Double-click component | Enter component edit mode |
| Click automation line | Add automation point |
| Drag automation point | Move point |
| Click in piano roll | Draw new note |
| Drag note body | Move note |
| Drag note edge | Resize note |
| Drag velocity handle | Adjust note velocity |

### Right Mouse Button

| Action | What it does |
|---|---|
| Right-click canvas | Open context menu |
| Right-click clip | Clip context menu (color, effects, automation, etc.) |
| Right-click automation point | Delete point |
| Right-click in browser | Browser context menu |

### Middle Mouse Button / Scroll

| Action | What it does |
|---|---|
| Middle-click drag | Pan the canvas |
| Scroll wheel | Pan the view vertically |
| `Shift`+scroll | Pan the view horizontally |
| `⌘`+scroll | Zoom in/out (centered on cursor) |
| Trackpad pinch | Zoom in/out |
