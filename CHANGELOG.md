# Changelog

## 0.1.0 (2026-04-16)


### Features

* add channels-on-air packet parsing and BeatFinder dispatch ([0d726f1](https://github.com/anweiss/prodjlink-rs/commit/0d726f1ddd9948f853f2591dabe332bf1b6cd04b))
* add master handoff (Baroque dance) protocol state machine ([979b9d8](https://github.com/anweiss/prodjlink-rs/commit/979b9d809987c56d458a4773094c4169834c2c0c))
* add MetadataProvider trait, NetworkProvider, and fetch convenience functions ([4dd6094](https://github.com/anweiss/prodjlink-rs/commit/4dd6094601b5b193fb4b58e5a7f497d3eeb4d5f2))
* add missing CdjStatus and MixerStatus convenience methods ([6b1a5cd](https://github.com/anweiss/prodjlink-rs/commit/6b1a5cd50add0052a95c91861fcffc510d36cc09))
* add virtual-cdj3000 example binary for testing ([6acc39e](https://github.com/anweiss/prodjlink-rs/commit/6acc39ef12ab74427cdbd683baa4459a17ecc786))
* add VirtualCdj commands, PlayerSettings, and status broadcasting ([8e909b1](https://github.com/anweiss/prodjlink-rs/commit/8e909b11bae9274b1521b016df32c971eb7a7e5f))
* CDJ-3000 compatibility — auto-detect interface, keep-alive fix, robust tests ([af1b7c0](https://github.com/anweiss/prodjlink-rs/commit/af1b7c062bd8549e5a0a232dad350201216d4b36))
* CDJ-3000 support — beat-based master inference and effective tempo tracking ([629a70a](https://github.com/anweiss/prodjlink-rs/commit/629a70ad5c62b534bb939ad542357a91aebacd68))
* **example:** implement beat phase matching in virtual CDJ-3000 ([be690fc](https://github.com/anweiss/prodjlink-rs/commit/be690fc4eaf36c446939dda0cfa161c2760fec02))
* implement MenuLoader for browsing rekordbox media libraries ([fa0a84e](https://github.com/anweiss/prodjlink-rs/commit/fa0a84e0bb10605b8b7477031573ce1704667b54))
* implement Opus Quad compatibility mode ([0927630](https://github.com/anweiss/prodjlink-rs/commit/0927630a4d9235c2a98f6b8864c32b416aac66ce))
* implement tempo master tracking and handoff protocol ([46eda6a](https://github.com/anweiss/prodjlink-rs/commit/46eda6a296f452ab39ea8af0182a7acea4c030f9))
* implement TimeFinder for playback time reconstruction ([afb62ec](https://github.com/anweiss/prodjlink-rs/commit/afb62ec6ec6c71659c36bd320e373792d8283199))
* **menu:** add SortOrder enum and missing menu browsing methods ([350e91c](https://github.com/anweiss/prodjlink-rs/commit/350e91cd2a44c06897b130b360a213db91e546ad))
* network interface discovery, command reception, and finder improvements ([98de60d](https://github.com/anweiss/prodjlink-rs/commit/98de60ddabcf81ef1152a5d973bc656cb95e4a6c))
* redesign virtual-cdj3000 with real-time TUI and instant keys ([e1a2e4f](https://github.com/anweiss/prodjlink-rs/commit/e1a2e4f498df0a4ac42b78a254d07a2c9c926e36))


### Bug Fixes

* collapse nested if in match arm for clippy 1.95 ([f364998](https://github.com/anweiss/prodjlink-rs/commit/f364998de3d7c5cd4339d01e180f2b7fbfbd0897))
* correct all protocol offsets and flags for CDJ-3000 and DJM-A9 support ([a8cbb65](https://github.com/anweiss/prodjlink-rs/commit/a8cbb6502c55750adc0e130648c0d2426e1ce049))
* correct keep-alive packet offsets for announce.rs ([a30e8e8](https://github.com/anweiss/prodjlink-rs/commit/a30e8e852985166ffad50de8b830a1fc84689194))
* enable SO_REUSEPORT on all listener sockets and wire up tempo master tracking ([01f7bb9](https://github.com/anweiss/prodjlink-rs/commit/01f7bb996151741b92a3edf3441e437ebcf6a099))
* remove custom CodeQL workflow (default setup handles scanning) ([4fc2bb3](https://github.com/anweiss/prodjlink-rs/commit/4fc2bb302b0c3f8a07bb27598f64bd38aa1ce4ea))
* resolve clippy warnings and format code ([2acb7cc](https://github.com/anweiss/prodjlink-rs/commit/2acb7ccab13b3cf3e22b962f1d3a7ea46d8c0557))
* status broadcast reads BPM/master from TempoMaster; add synced state ([c3f6f8f](https://github.com/anweiss/prodjlink-rs/commit/c3f6f8f6260f49776f3117ba459af8d160e45f25))
