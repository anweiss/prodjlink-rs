# prodjlink-rs

A native Rust implementation of the [Pioneer Pro DJ Link](https://djl-analysis.deepsymmetry.org/djl-analysis/startup.html) protocol — the proprietary network protocol used by Pioneer DJ equipment (CDJ-2000NXS2, CDJ-3000, DJM-900NXS2, DJM-A9, Opus Quad, XDJ-XZ, and more) to communicate on a local network.

Built as an idiomatic Rust alternative to the Java [beat-link](https://github.com/Deep-Symmetry/beat-link) library by Deep Symmetry.

## Features

- **Device Discovery** — detect CDJs, mixers, and all-in-one units on the network
- **Real-time Beats** — receive beat, tempo, position, and on-air status packets
- **CDJ/Mixer Status** — full status parsing with play state, sync, master, BPM, pitch, loops, cue countdown
- **Virtual CDJ** — join the network as a virtual player with device number claiming, keep-alive, and command sending
- **Tempo Master** — track and participate in the tempo master handoff protocol
- **Track Metadata** — fetch track title, artist, album, genre, key, BPM, rating, and more via the dbserver protocol
- **Album Artwork** — retrieve album art from players
- **Waveforms** — access preview and detail waveforms (blue, RGB, and 3-band styles)
- **Beat Grids** — parse beat grid data with bar numbering and BPM-at-beat lookups
- **Cue Lists** — hot cues, memory points, and loops with Nexus and Nxs2 binary format support
- **Menu Browsing** — browse a player's media library by artist, genre, album, key, BPM, rating, color, and more
- **Playback Position** — reconstruct precise playback time from multiple data sources
- **Opus Quad** — full compatibility with player number remapping and synthetic announcements
- **CDJ-3000** — extended loop fields, precise position packets, 3-band waveforms
- **DJM-A9 / DJM-V10** — mixer status parsing, 6-channel on-air support
- **Player Settings** — build and send "My Settings" payloads to players
- **Async/Await** — built on [Tokio](https://tokio.rs) for high-performance async I/O

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
prodjlink-rs = { path = "." }
tokio = { version = "1", features = ["full"] }
```

### Discover devices and listen for beats

```rust
use prodjlink_rs::ProDjLink;
use std::net::Ipv4Addr;

#[tokio::main]
async fn main() -> prodjlink_rs::Result<()> {
    let pdl = ProDjLink::builder()
        .device_name("my-app")
        .device_number(5)
        .interface_address(Ipv4Addr::new(192, 168, 1, 100))
        .build()
        .await?;

    // List devices on the network
    for device in pdl.devices().await {
        println!("Found: {} (player {})", device.name, device.number);
    }

    // Subscribe to beats
    let mut beats = pdl.subscribe_beats();
    while let Ok(event) = beats.recv().await {
        match event {
            prodjlink_rs::BeatEvent::Beat(beat) => {
                println!(
                    "Player {} beat {}/4 at {:.1} BPM",
                    beat.device_number, beat.beat_within_bar, beat.effective_tempo()
                );
            }
            prodjlink_rs::BeatEvent::PrecisePosition(pos) => {
                println!(
                    "Player {} at {}ms / {}s",
                    pos.device_number, pos.position_ms, pos.track_length
                );
            }
        }
    }

    Ok(())
}
```

### Monitor player status

```rust
use prodjlink_rs::{ProDjLink, DeviceUpdate};

// Subscribe to status updates
let mut status = pdl.subscribe_status();
while let Ok(update) = status.recv().await {
    match update {
        DeviceUpdate::Cdj(s) => {
            println!(
                "Player {}: {} at {:.1} BPM (pitch {:+.1}%)",
                s.device_number,
                if s.is_playing() { "▶ Playing" } else { "⏸ Paused" },
                s.bpm,
                s.pitch.to_percentage()
            );
        }
        DeviceUpdate::Mixer(m) => {
            println!("Mixer {}: {:.1} BPM", m.device_number, m.bpm);
        }
    }
}
```

### Fetch track metadata

```rust
use prodjlink_rs::{fetch_metadata, DeviceNumber, TrackSourceSlot};

let metadata = fetch_metadata(
    pdl.connection_manager(),
    DeviceNumber(2),
    TrackSourceSlot::Usb,
    42, // rekordbox track ID
).await?;

println!("Title: {}", metadata.title);
println!("Artist: {}", metadata.artist.as_ref().map_or("Unknown", |a| &a.label));
println!("BPM: {}", metadata.tempo);
```

### Browse a player's library

```rust
use prodjlink_rs::{MenuLoader, TrackSourceSlot};

let mut client = pdl.connection_manager()
    .get_client(DeviceNumber(2)).await?;

// Browse by artist
let artists = MenuLoader::artist_menu(&mut client, TrackSourceSlot::Usb).await?;
for artist in &artists {
    println!("{} (id: {})", artist.label1, artist.id);
}

// Get albums by an artist
let albums = MenuLoader::artist_album_menu(
    &mut client, TrackSourceSlot::Usb, artists[0].id
).await?;
```

### Send commands to players

```rust
let vcdj = pdl.virtual_cdj();

// Load a track onto player 1
vcdj.load_track(
    DeviceNumber(1),     // target player
    DeviceNumber(2),     // source player
    TrackSourceSlot::Usb,
    42,                  // rekordbox track ID
).await?;

// Control sync
vcdj.set_sync(DeviceNumber(1), true).await?;

// Become tempo master
vcdj.become_master(DeviceNumber(1)).await?;
```

## Architecture

```
prodjlink-rs
├── protocol/       # Packet parsing and building
│   ├── header      # Magic header validation, packet type dispatch
│   ├── announce    # Keep-alive packets, device claim protocol
│   ├── beat        # Beat, PrecisePosition, ChannelsOnAir
│   ├── status      # CdjStatus, MixerStatus, CdjStatusBuilder
│   ├── command     # Fader start, load track, sync, master commands
│   └── media       # MediaDetails packets
├── network/        # Async services (Tokio-based)
│   ├── finder      # DeviceFinder — device discovery and tracking
│   ├── beat        # BeatFinder — beat/position/on-air/sync dispatch
│   ├── status      # StatusListener — CDJ/mixer status monitoring
│   ├── virtual_cdj # VirtualCdj — network participation and commands
│   ├── tempo       # TempoMaster — master tracking and handoff
│   ├── time        # TimeFinder — playback position reconstruction
│   └── interface   # Network interface discovery
├── data/           # Track data and metadata
│   ├── metadata    # TrackMetadata, DataReference, SearchableItem
│   ├── artwork     # AlbumArt with JPEG/PNG detection
│   ├── beatgrid    # BeatGrid with bar numbering
│   ├── cue         # CueList with Nexus/Nxs2 binary parsing
│   ├── waveform    # WaveformPreview/Detail (blue, RGB, 3-band)
│   ├── menu        # MenuLoader for library browsing
│   ├── provider    # MetadataProvider trait
│   ├── fetch       # One-shot typed fetch functions
│   └── color       # ColorItem for track color labels
├── dbserver/       # Pioneer database server protocol
│   ├── message     # Message framing, 69 message types, 65 menu item types
│   ├── field       # Number, Binary, String field codecs
│   ├── client      # Async TCP client with handshake
│   └── connection  # Connection pool with idle timeout
├── device/         # Device types and settings
│   ├── types       # DeviceNumber, Bpm, Pitch, PlayState, SlotReference, etc.
│   └── settings    # PlayerSettings builder
├── testing/        # Test harness (hardware-free testing)
│   ├── packets     # Mock packet builders for all packet types
│   ├── fixtures    # Golden packet fixtures (CDJ-3000, DJM-A9, etc.)
│   └── scenarios   # Multi-packet workflow simulators
└── lib.rs          # ProDjLink entry point and re-exports
```

## Hardware-Free Testing

The `testing` module provides mock packet builders and golden fixtures so you can develop and test without physical DJ equipment:

```rust
use prodjlink_rs::testing::packets::MockCdjStatusBuilder;
use prodjlink_rs::testing::fixtures;
use prodjlink_rs::protocol::status::parse_cdj_status;

// Build custom mock packets with a fluent API
let packet = MockCdjStatusBuilder::new(1)
    .name("CDJ-3000")
    .bpm(174.0)
    .playing()
    .synced()
    .on_air()
    .track(42, 2, 3)
    .beat(64, 4)
    .cdj3000_loop(1000, 5000, 4)
    .build();

let status = parse_cdj_status(&packet).unwrap();
assert!(status.is_playing());
assert_eq!(status.bpm.0, 174.0);

// Use golden fixtures modeled after real hardware
let cdj3000_pkt = fixtures::cdj_3000_looping();
let djm_a9_pkt = fixtures::djm_a9_status();
let opus_quad_pkt = fixtures::opus_quad_keepalive();
```

Available fixtures: CDJ-2000NXS2 (playing, cued), CDJ-3000 (looping), CDJ-900 (pre-nexus), DJM-900NXS2 (master), DJM-A9, Opus Quad, beat packets, precise position, and channels-on-air.

## Supported Hardware

| Device | Status |
|--------|--------|
| CDJ-3000 | ✅ Full support (extended loops, precise position, 3-band waveforms) |
| CDJ-2000NXS2 | ✅ Full support |
| CDJ-2000NXS | ✅ Full support |
| CDJ-900NXS | ✅ Full support |
| CDJ-900 / CDJ-2000 | ✅ Pre-nexus support (legacy packet format) |
| XDJ-XZ | ✅ Full support (dual USB slots) |
| XDJ-1000MK2 | ✅ Full support |
| DJM-A9 | ✅ Full support |
| DJM-900NXS2 | ✅ Full support |
| DJM-V10 | ✅ Full support (6-channel on-air) |
| Opus Quad | ✅ Full support (player remapping, synthetic announcements) |

## Protocol Reference

This implementation is based on the [DJ Link Packet Analysis](https://djl-analysis.deepsymmetry.org/djl-analysis/startup.html) by Deep Symmetry and verified against the [beat-link](https://github.com/Deep-Symmetry/beat-link) Java library.

### Ports
- **50000** — Device announcement (keep-alive, device claim)
- **50001** — Beat sync (beats, sync commands, master handoff, fader start, on-air)
- **50002** — Status updates (CDJ status, mixer status, load track, media queries)

### Packet Types
| Type | Port | Description |
|------|------|-------------|
| `0x06` | 50000 | Keep-alive announcement |
| `0x0a` | 50000/50002 | Device claim / CDJ status |
| `0x28` | 50001 | Beat |
| `0x0b` | 50001 | Precise position (CDJ-3000+) |
| `0x03` | 50001 | Channels on-air |
| `0x29` | 50002 | Mixer status |
| `0x2a` | 50001 | Sync command |
| `0x26` | 50001 | Master command |
| `0x02` | 50001/50002 | Fader start |
| `0x19` | 50002 | Load track |

## License

This project is not affiliated with Pioneer DJ or AlphaTheta Corporation. "Pioneer", "CDJ", "DJM", "rekordbox", and "Pro DJ Link" are trademarks of their respective owners.

## Acknowledgments

- [Deep Symmetry](https://deepsymmetry.org/) for the [beat-link](https://github.com/Deep-Symmetry/beat-link) Java library and [DJ Link Packet Analysis](https://djl-analysis.deepsymmetry.org/djl-analysis/startup.html)
- The [dysenern/prolink-go](https://github.com/EvanPurkhiser/prolink-go) Go implementation for additional protocol insights
