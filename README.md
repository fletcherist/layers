# Layers

![Rust](https://img.shields.io/badge/built%20with-Rust-orange?logo=rust)
![License](https://img.shields.io/badge/license-Apache%202.0-blue)
![Status](https://img.shields.io/badge/status-alpha-red)
![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Web-lightgrey)

A rusty DAW reimagined as an infinite canvas — Figma, but for audio. No tracks. Place clips, MIDI, and effects anywhere. Collaborate in real-time with others.

Built in Rust. GPU-accelerated

> Early alpha — under active development.

## Features

- **Infinite canvas** — place audio clips, MIDI, and effects freely, no fixed track lanes
- **Audio editing** — waveform display, split, fade, reverse, pitch shift, volume/pan per clip
- **MIDI** — piano roll with note velocity, automation lanes, BPM-synced grid
- **VST3** — load instruments and effects directly onto the canvas
- **Real-time collaboration** — live cursors, shared canvas, op-based sync and undo
- **Components** — reusable clip groups (Figma-style masters and instances)
- **Runs in the browser** — compiles to WebAssembly, no install needed (VST3 unavailable)

## Build

```sh
git clone https://github.com/layersaudio/layers
cd layers
cargo run
```

## Platform

macOS · Web (WASM) · Windows _(planned)_

## License

Apache 2.0
