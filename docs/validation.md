# Test record

Last updated: **2026-10-31**.
Current development version: **0.1.1**.

This record covers local DCS Tablet checks.
It does not replace the physical tablet and flight tests below.
Initial aircraft support is for the **DCS F/A-18C Hornet only**.

## Completed checks

| Check | Result |
| --- | --- |
| Windows MSVC release build | Passed |
| Rust unit tests | 20 passed; one physical display test excluded from the normal set |
| Rust tests with test-hooks | Passed |
| Source format and Clippy checks | Passed |
| HTTP availability tests | All six scenarios passed |
| Frontend recovery and version display | Passed |
| Chromium video test | Passed with actual H.264 video |
| Chromium display controls | Passed for zoom, full screen, and return to both DDIs |
| WebKit page layout | Passed for landscape and the portrait message |
| Physical virtual display check | Passed on the local PC |
| Lua control script | Passed with the installed DCS Lua runtime |
| Separate profile checks | Passed with temporary test profiles |
| Windows ZIP | Integrity and SHA-256 checks passed |

The physical display check activated the inactive virtual display.
It checked the capture device identity and restored the display state.
The main display remained unchanged.

The Lua checks covered all 40 button mappings, old-message rejection, release time limits, profile restriction, and callback order.
The profile checks covered repeated setup, controller-file copies, changed-file protection, and protection of an existing unowned profile.

These earlier component checks remain relevant to the current build.
The display check does not prove a complete managed DCS flight.

## HTTP check details

The test file is `tests/http_limits.test.cjs`.
It starts a separate demo on port 18194.
The tests reproduce failures against the original development build, before the HTTP corrections.

| Scenario | Result with the corrected HTTP service |
| --- | --- |
| Four incomplete bodies | Health checks continued. |
| Incomplete headers | The connections expired at approximately two seconds. |
| Trickled body bytes | The connection expired at approximately three seconds. |
| Oversized or ambiguous requests | The application rejected them before application processing. |
| Excess connections from one IP | The application closed the excess connection. |
| Shutdown with incomplete bodies | Shutdown completed in approximately 150–200 milliseconds. |

Windows can close oversized input with a TCP reset before its HTTP error reaches the client.
The test accepts that bounded rejection for an oversized header.

## Tests that are still necessary

| Test | Required check |
| --- | --- |
| Physical iPad Safari | Video, touch buttons, two fingers, zoom, full screen, screen lock, and reconnection |
| Extended F-18 flight | Airborne operation, remaining bezel mappings, and interrupted-session recovery |
| Other PC arrangements | Different monitor positions, display scaling, and relocated Saved Games |
| Streamer failure during a flight | Recovery attempts and the recovery limit |
| Display removal | Behavior when the virtual display becomes unavailable |
| Long session | Extended operation without increasing resource use |
| GitHub workflows | Actual execution after repository publication |

The earlier checks above did not include a live DCS Tablet flight.
The checks did not change the normal DCS options, normal export scripts, or Windows Firewall rules.
The virtual display returned to its inactive state after its check.

## Repeat the checks

Use the commands in [Build and test instructions](../CONTRIBUTING.md).
The physical display check needs its separate, explicit command.

For each new result, record the version, build ID, date, device, procedure, and outcome.
Mark an incomplete check as incomplete.
Do not describe a browser simulation as a physical tablet test.

## Control connection correction in 0.1.0

Version 0.1.0 includes the correction for queued control acknowledgements.
The original corrected build was `cc4231cc0f1327ba`, labelled 0.1.1.
The older 0.1.1 release ZIP does not include this correction.

The original release showed a control-readiness outage of approximately nine seconds
while its video viewer remained connected.
A new regression test reproduced delayed button release with queued duplicate replies.
It passed after the control worker was changed to drain queued replies without blocking.

The corrected build was checked against a running F/A-18C cockpit with desktop Chromium.
During more than one minute, 240 samples showed no control-readiness loss or video disconnection.
The maximum recorded control acknowledgement age was 23 milliseconds.
Four touch-emulated MENU taps changed the real left and right DDI pages.
All eight press and release HTTP requests were accepted.

This check used a separate local server and did not restart the user's flight.
It does not replace the full physical iPad, network interruption, or extended-session tests.

## User confirmation

The user subsequently reported that the corrected Desktop build works.
The device model, browser version, test duration, and complete set of actions were not recorded.
This confirms successful use, but does not complete every tablet acceptance test.

## Windows encoder migration in 0.1.0

The application now uses the Microsoft H.264 encoder installed with Windows Media Foundation.
Native encoder tests check immediate output, sequence headers, and requested keyframes.
Color conversion tests check NV12 levels, chroma averaging, and invalid input.
Earlier OpenH264 video results do not establish video compatibility for this encoder.
The Windows encoder and color conversion tests passed on the local PC.
Chromium decoded the synthetic demo with the expected left and right image colors.
A disconnect and reconnect created a new encoder worker and resumed video successfully.
WebKit layout checks and all six HTTP availability scenarios passed.
The offline guide generator passed its local link and consistency checks.

The current encoder was subsequently checked in the live cockpit session below.
The earlier cockpit result above used the previous encoder.


## Earlier 0.1.0 live cockpit check — 2026-10-28

Version 0.1.0, build `294e1c72d36dc6b2`, from a fresh extraction of the Windows release ZIP.
The managed launcher used the existing dedicated DCS.Tablet profile.
The mission was F/A-18C Instant Action, Caucasus, Ready on the Ramp.
The browser was desktop Chromium with touch emulation, using the PC LAN address.
This was a powered ground-cockpit test, not a physical tablet or airborne flight test.

- Managed launch positioned the cockpit and enabled the virtual display.
- Both DDI images decoded correctly through Windows Media Foundation and WebRTC.
- Four emulated MENU taps changed both real DDI pages.
- The repeat stability run recorded 240 samples over approximately one minute, with no readiness loss or viewer disconnection.
- The maximum control acknowledgement age in that run was 40 milliseconds.
- Decoded video advanced from 256 to 2055 frames over approximately 60 seconds.
- Disconnect and reconnect restored video and touch readiness. No browser script errors occurred.
- Normal DCS exit stopped streaming and restored the virtual display to inactive, with no recovery pending.
- The physical main display remained at its original resolution, position, and refresh rate.
- All 85 files in the normal DCS profile's Config and Scripts baseline remained unchanged.

The first run stopped when a sample showed an acknowledgement age of 777 milliseconds,
above the 700-millisecond readiness threshold.
DCS logged an ANTIFREEZE event at approximately the same time.
The control connection subsequently recovered without restarting the mission or server.
The repeat run above was clean; this does not prove that simulation stalls cannot interrupt readiness.
The first test's forced browser closure also recorded a socket-reset error; the repeat run used normal Disconnect.

Physical iPad Safari, other monitor configurations, long sessions, and interrupted-session recovery remain unverified for this build.


## User manual confirmation — 2026-10-28

After the current release's live DCS check, the user tested the application manually
on a physical iPad using Safari and reported that everything seemed to work correctly.
The iPad model, Safari version, duration, and individual actions were not specified.
This confirms successful manual use; it does not establish completion of each remaining acceptance test.


## Reliability review implementation — 2026-10-28

The source was subsequently changed after the live cockpit and user iPad Safari checks above.
Those physical-device results belong to their recorded builds, not automatically to this revision.

The reliability pass adds connection generations and cancellation, playback retry,
a bounded keyframe-aware encoded FIFO, source freshness diagnostics, safe bridge upgrades,
typed persistent data, source-verified packaging, restart-budget checks, and log rotation.

Local checks for the new implementation:

- 29 Rust unit tests passed; the opt-in physical display test remained excluded.
- Strict Clippy and source formatting passed.
- Four browser lifecycle regressions cover stale status responses, old control request success/failure, and playback rejection.
- Existing frontend recovery and all six HTTP availability scenarios passed.
- Chromium decoded actual H.264, and WebKit layout checks passed.
- Real browser fault tests exercised encoder failure/panic and capture exit/panic, viewer release, and reconnect.
- An injected sender stall exercised encoded FIFO overflow and recovered with clean decoded demo colors.
- An injected source stall showed a freshness warning without closing the viewer, then cleared the warning when callbacks resumed.
- The Lua bridge regression suite passed in the installed DCS Lua runtime without starting DCS.
- Matching source passed release verification; a modified same-version source fixture and a test-hook package were rejected.
- A synthetic owned-child test verified launcher startup, log-pipe draining, and clean child exit.
- Active-session log rotation stayed within its byte budget.
- CSS cleanup preserved computed styles in landscape, portrait, single-display, and immersive layouts.

GitHub workflows include the new browser/failure/package checks and a separate mocked Lua 5.1 job.
Hosted execution and long-session/interrupted real-display recovery remain separate acceptance work.
The short live DCS check was subsequently completed below; physical iPad Safari was not repeated.

The resulting production build ID is `c9f03f00be560536`.


## Reviewed 0.1.0 build: live DCS validation — 2026-10-28

Version 0.1.0, build `c9f03f00be560536`, was validated from a fresh release extraction
after the reliability and maintainability changes above.
The managed launcher started DCS with the dedicated DCS.Tablet profile.
The installed bridge matched the current source and its ownership manifest upgraded to schema 2.
The mission was F/A-18C Instant Action, Caucasus, Ready on the Ramp.
The viewer was desktop Chromium with touch emulation, using local loopback HTTP.
The user dismissed the temporary application's Windows firewall prompt; public-network access was not granted.
This was a powered ground-cockpit test, not an airborne flight or physical iPad test.

- Both DDI images decoded correctly and showed the live aircraft pages.
- Four MENU taps changed the actual left and right DDI menus. All eight press/release requests returned HTTP 202.
- 480 status samples over more than two minutes recorded zero control-readiness losses, viewer disconnections, source-staleness warnings, or application errors.
- Maximum control acknowledgement age: 20 milliseconds. Maximum capture callback age: 33 milliseconds.
- The decoder advanced by 3760 frames over 125.35 seconds: approximately 30 frames per second.
- Disconnect and reconnect restored video and touch readiness without errors.
- No browser script errors, worker panics, or failed sessions were recorded.
- Normal DCS exit stopped streaming, drained the log pipes, restored the virtual display to inactive, and cleared recovery state.
- The main display remained 4096 × 2160 at 60 Hz, primary at its original position.
- All 85 baseline files in the normal DCS profile's Config and Scripts folders remained unchanged.

This completes the short live cockpit check for the reviewed build.
The user's earlier iPad Safari confirmation still applies to its earlier build.
A new physical iPad Safari check, extended airborne sessions, other display layouts, and interrupted real-display recovery remain separate acceptance tests.


## Profile and data safety patch — 2026-10-31

Version 0.1.1, production build `4b0321f8e08197e9`.

- Rust default and test-hook suites: 35 passed each; the hardware test is separate.
- The physical virtual-display roundtrip passed: activation, capture identity, and verified restoration; the primary display remained unchanged.
- Profile fixtures checked ownership rejection, independent input copies, junction rejection at the root and Input directory, hard-linked destination rejection, and refusal of an incorrect Saved Games root.
- Removal preserved the complete edited profile byte-for-byte, including sample personal missions and synthetic login data. A partially created profile was also preserved in an archive.
- Atomic writes preserved an unrelated temporary file and its linked source. Non-replacing moves preserved an already existing backup destination.
- Source format, Clippy, 12 frontend/HTTP test cases, frontend syntax, and the installed DCS Lua runtime checks passed.
- Chromium decoded real H.264 video, reconnected, and passed the controls/layout checks; WebKit layout passed.
- Injected capture/encoder failures, queue overflow, stale capture recovery, package source/feature guards, and owned-child logging/shutdown passed.
- The regenerated offline guide passed local-link, anchor, and image checks.

These safety tests use temporary fixture profiles. They do not remove or archive the user's actual DCS.Tablet profile.
The previous live cockpit tests above remain the flight evidence; no new cockpit session or physical iPad Safari test was performed for this safety patch.
The separately installed virtual display driver and failures of the operating system/storage remain outside these guarantees. See [Data safety](data-safety.md).
