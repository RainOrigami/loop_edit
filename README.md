# loop_edit

A rolling audio buffer with a crop-and-save GUI. It records continuously into a
ring buffer, so when something worth keeping *just* happened, you can still grab
it — the last 60 seconds are always there.

The classic use: someone says something hilarious on voice chat. Hit **Capture &
Edit**, drag two markers around the good part, save a WAV. Recording keeps
running in the background the whole time, so you never miss what happens next.

## Features

- Continuous ring-buffer recording (duration configurable, 60 s by default)
- Live waveform while recording
- Freeze a snapshot to edit without interrupting capture
- Drag-to-crop editor with waveform zoom
- WAV export at 16/24-bit with an optional export gain
- Captures PipeWire monitor sources, so you can record app audio (a chat client,
  a game, the whole desktop) rather than just a mic

## Requirements

- Linux with PipeWire (PulseAudio works through the compatibility layer)
- A GPU with Vulkan/OpenGL for the egui/wgpu front-end
- Build: Rust toolchain, `libpipewire-0.3-dev`, `libpulse-dev`, `libclang-dev`,
  `pkg-config` (Arch: `pipewire libpulse clang pkgconf`)

## Install

```bash
git clone https://github.com/RainOrigami/loop_edit
cd loop_edit
cargo build --release
install -Dm755 target/release/loop_edit ~/.local/bin/loop_edit
```

Prebuilt x86_64 binaries are attached to [each release](../../releases).

## Usage

Run `loop_edit`. On first start it writes `~/.config/loop_edit/config.toml` and
begins recording from the configured device.

- **Capture & Edit** freezes the current buffer; recording continues underneath.
- Drag the start/end markers, then **Save WAV**.
- **Back to Recording** returns to the live view — you can capture again at once.

Run it from a terminal to see capture and export logs.

## Configuration

`~/.config/loop_edit/config.toml`:

```toml
[audio]
# Substring match against PipeWire node names
capture_device = "pipewire"
playback_device = "default"
sample_rate = 48000
channels = 2

[buffer]
duration_seconds = 60

[output]
default_folder = "~/Music/LoopRecordings"
filename_pattern = "loop_%Y%m%d_%H%M%S.wav"   # strftime
bit_depth = 16                                 # 16 or 24
gain_db = 0.0                                  # applied on export

[ui]
waveform_color = [0, 255, 0, 255]              # RGBA
```

### Picking a capture device

`capture_device` is matched as a substring against PipeWire node names:

```bash
pw-cli list-objects | grep node.name
```

- A **source** (microphone): use its node name directly.
- A **sink** (anything playing audio): append `.monitor` to record what it plays.

So a virtual sink named `comms` that your chat client outputs to becomes
`comms.monitor`. `pw-loopback` is a convenient way to create such per-app sinks.

## Design notes

- Three threads: GUI at display rate, a real-time capture thread, and a
  downsampler that feeds the waveform at ~10 Hz. The capture thread never blocks
  on the GUI.
- ~25 MB of RAM for 60 s of 48 kHz stereo — the buffer is plain interleaved f32.
- Editing operates on a snapshot copy, which is why capture can continue while
  you drag markers around.

## Provenance

Written entirely by an AI agent (Claude Code) under human direction. Treat it
accordingly: read the source before trusting it with anything you can't re-record.

## License

[Unlicense](UNLICENSE) — public domain.
