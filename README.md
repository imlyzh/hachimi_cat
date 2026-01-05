# Hachimi Cat

An open-source voice calling and conferencing software that can be "low-cost and self-hosted".

## TODO List (Wish List)

1. [ ] RingBuf换rtrb
2. [ ] RTP协议封装，包括包id和端到端（采集）时间戳
3. [ ] 实现Jitter
   1. [ ] 包排序
   2. [ ] Jitter内调用Decoder，操作Decoder完成FEC和PLC行为
4. [ ] Mixter 混音器，拥有接受多个语音通道输入程度的能力
5. [ ] 实时性配置，降低端到端延迟

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
