# Hachimi Cat

An open-source voice calling and conferencing software that can be "low-cost and self-hosted".

## Run

### listen

```sh
hacat listen
```

if you use source build:

```sh
cargo run --bin=hacat --release -- listen
```

### call

```sh
hacat call EndpointId
```

if you use source build:

```sh
cargo run --bin=hacat --release -- call EndpointId
```

## Build

### 1. Install System Dependencies

if you use nix:

```sh
   direnv allow .
```

#### Custom Install System Dependencies

require Opus, webrtc-audio-processing, libclang, pkg-config, autoconf, automake, cmake

### 2. Build Rust Program/Library

```sh
   cargo build --release
```

### Total

```sh
   direnv allow .
   cargo build --release
```

## Architecture

1. AudioService
   - depends on AudioEngine
   - Add Single Encoder binding Single/Multiple Sender Task
   - Add Multiple Decoder - Reciver Task binding Pair
2. AudioEngine
   - depends on AudioProcessing
   - Add cpal/coreaudio
3. AudioProcessing
