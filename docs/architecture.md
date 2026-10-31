# Technical description

This page is for developers and technical support.
The [user guide](user-guide.md) contains the operating instructions.

The first aircraft for this application is the DCS F/A-18C Hornet only.
Aircraft button mappings are specific to that module.

## Application components

DCS Tablet is one native Windows Rust application.
The executable includes its browser page and Lua script.
It uses operating-system threads without a Rust async runtime.

| Component | Function |
| --- | --- |
| Windows Graphics Capture | Reads the selected display's pixels. |
| Windows Media Foundation | Encodes the DDI images with the Microsoft H.264 transform. |
| str0m | Provides the WebRTC video connection. |
| httparse | Parses HTTP request headers. |
| Windows APIs | Control display state and window position. |
| Lua script | Operates the aircraft's DDI buttons. |


## Video path

```text
DCS DDI exports
    -> Active virtual display
    -> Windows Graphics Capture
    -> Windows Media Foundation H.264 encoder
    -> WebRTC connection
    -> Tablet video decoder
    -> Left and right display images
```

The default combined image is 1024 × 512 pixels at 30 frames per second.
The target bit rate is 1 Mbit/s.
The H.264 configuration uses the existing Level 3.1 limits.

Capture and encoding operate only while a video viewer is active.
New raw frames replace old raw frames in storage.
Encoded frames use a four-frame FIFO. Overflow clears dependent output and requests a fresh keyframe before sending resumes.
Video queues and diagnostic histories have fixed limits.
The capture check compares the Windows device identity with the selected display.

## Control path

The browser sends ordered button-state messages to `/controls`.
Each DDI uses a 20-bit button mask.
The Rust control worker sends messages to the Lua script through loopback UDP.

The Lua script listens at `127.0.0.1:17881`.
It operates only inside the `DCS.Tablet` profile.
It accepts a fixed message format rather than executable Lua text.

| Mapping | Value |
| --- | --- |
| Left DDI device | 35 |
| Right DDI device | 36 |
| Button commands | 3011 through 3030 |
| Press value | 1 |
| Release value | 0 |

Both sides reject old message revisions.
Lost contact causes button release.
A continuous button press has a two-second limit.
These checks do not authenticate a user.

## HTTP limits

One nonblocking I/O thread manages the HTTP sockets.
Four application workers receive only complete requests.

| Limit | Value |
| --- | --- |
| Active connections | 32 |
| Connections per source IP | 8 |
| Remote connection slots | 28 |
| Slots reserved for loopback connections | 4 |
| Complete request queue | 16 |
| Header size | 8 KiB |
| Header fields | 64 |
| Header deadline | 2 seconds |
| Body deadline after complete headers | 3 seconds |
| Response write deadline | 2 seconds |
| Application response deadline | 10 seconds |
| Offer body size | 64 KiB |
| Control body size | 512 bytes |

Slow progress does not extend these deadlines.
Other routes accept no body.
The application accepts fixed Content-Length bodies.
It rejects Transfer-Encoding, upgrades, Expect, duplicate Content-Length, and pipelining.
Each response closes its connection.

A full work queue gives HTTP 503.
Excess connections close immediately.
Shutdown closes incomplete input and gives computed responses a maximum 100-millisecond flush interval.

## Managed flights

The launcher starts a separate streamer process and DCS.
It checks the streamer process identity and HTTP health.
It permits three recovery attempts in a 60-second interval.
After a streamer restart, the tablet must connect again.

The launcher records the display state before a change.
Updates to JSON records use unique, exclusively created temporary files, synchronized contents, and atomic replacement.
A first-time write cannot replace a newly appearing unrelated destination.
An exclusive session lock prevents simultaneous managed operations.
Recovery waits until DCS has closed.

## DCS profile protection

The separate profile contains initial copies of options and controller settings.
It does not share writable junctions with the source profile.
Removal validates ownership and managed paths, then archives the whole profile by a nonreplacing directory move.
No profile contents are deleted, including user-edited and DCS-created files.
Managed paths reject reparse points and shared hard-linked files.

## Build information

`build.rs` calculates a build ID from source files, build metadata, browser content, Lua content, and enabled features.
The UTC build date uses compile-time information.
`SOURCE_DATE_EPOCH` can supply a fixed timestamp for a release build.
The build ID is diagnostic information, not a signature.

## Windows encoder lifecycle

The encoder worker initializes COM and Media Foundation on its own thread.
It uses the synchronous Microsoft H.264 encoder with Baseline profile and Level 3.1.
BGRA images are converted to limited-range BT.601 NV12 before encoding.
The encoder uses low-latency mode without B-frame reordering.
Requested keyframes contain the sequence and picture parameter sets required by WebRTC.
The worker releases COM objects before it shuts down Media Foundation.
Windows supplies the encoder; DCS Tablet does not redistribute codec implementation files.


## Failure and ownership contracts

The [control protocol](control-protocol.md) defines viewer sessions, revisions, UDP retries, and release timers.
A video session owns its capture source and encoder worker. Cancellation wakes pacing waits before joins.
The encoder initializes COM and Media Foundation on its own thread. Its interfaces are released before that runtime shuts down.
Media Foundation timestamps use 100-nanosecond units; RTP video uses a 90-kHz clock.
Display coordinates are physical desktop pixels; the generated DCS monitor layout uses window-relative export coordinates.

Capture freshness is separate from HTTP liveness. The page warns after eight seconds without a source callback; it does not restart static content.
Managed profile schema 2 upgrades an unchanged owned bridge atomically. User-edited files stop the upgrade.
Options and input bindings remain independent user copies. A monitor-layout resolution change still requires profile reinstallation.
The old ownership manifest remains until the bridge write succeeds, so an interrupted upgrade can accept either complete version.
Settings and display recovery use typed schema validation before persistent values are applied.

## Build and diagnostics

`--build-info` prints machine-readable build metadata. `--verify-source PATH` verifies the source/feature fingerprint.
This fingerprint identifies accidental mismatches; it is not a cryptographic signature.
Packaging rejects test-hook binaries, the wrong target, mismatched source/dependency files, and an out-of-date offline guide.
Each package contains BUILD-INFO.json.
The launcher owns piped streamer output and rotates stream.log after two MiB, retaining at most three prior logs.
Rotation also occurs during an active session. Each new log is limited to two MiB; session boundaries include a timestamp and build ID.
A peer UDP reset is logged as a transport departure; encoder and capture errors remain pipeline failures.

The [data-safety contract](data-safety.md) describes write boundaries, backups, and recovery limits.
