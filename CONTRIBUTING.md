# Build and test instructions

This page is for developers.
Users of the Windows release package do not need these tools.

Initial aircraft support is for the **DCS F/A-18C Hornet only**.
The supported display position is landscape.
The application does not operate with VR.

## Before you start

Use a Windows 10 or Windows 11 x64 PC with these tools:

| Tool | Purpose |
| --- | --- |
| Rust with the MSVC toolchain | Build the Windows application. |
| Visual Studio C++ Build Tools | Build the native software components. |
| Node.js | Do the JavaScript and HTTP tests. |
| Playwright | Do the browser tests. |
| Lua 5.1 | Do the DCS control connection tests. |
| Python 3.11 or later | Rebuild the offline documentation. |

Close any application that uses the output executable before you replace it.
Do not use a real flight profile for an automated test.

## Build the application

1. Open PowerShell in the source folder.
2. Enter this command:

```powershell
cargo test --locked
```

3. Enter this command:

```powershell
.\scripts\Set-PrivateBuild.ps1
cargo build --release --locked
```

Result: Cargo creates `target\release\dcs-tablet.exe`.
The application includes the browser page and the Lua control script.

Configure path remapping before compiling release dependencies. For an existing
checkout, run `cargo clean --release` once before the remapped release build.
The packaging script rejects user-directory paths in the executable and every
ZIP member, including UTF-16 strings. Do not distribute PDB files.
Run the check directly with `python scripts/Check-ReleasePrivacy.py target/release/dcs-tablet.exe`.

## Check the source

1. Enter each command separately:

```powershell
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --features test-hooks
node tests/check_frontend.cjs
```

Result: Each command completes without an error.
The `test-hooks` feature permits controlled fault tests.
Release builds must use the default features.

## HTTP and page recovery tests

Before you start, build the release executable.

1. Enter this command:

```powershell
node --test tests/frontend_recovery.test.cjs tests/frontend_lifecycle.test.cjs tests/http_limits.test.cjs
```

Result: The tests complete without an error.
The HTTP test starts its own demo on port 18194.
It checks slow requests, connection limits, health checks, and shutdown.
It stops its demo after the test.

## Browser page tests

1. Enter this command:

```powershell
npm install --no-save playwright@1.56.1
```

2. Enter this command:

```powershell
npx playwright install chromium webkit
```

3. Enter this command:

```powershell
node tests/browser-smoke.cjs
```

Result: The test checks actual video in Chromium and page layout in WebKit.
It uses a separate demo on port 18092.
A WebKit layout test does not prove video operation on a physical iPad.

## Select a test executable

Example:

```powershell
$env:DCS_TABLET_EXE = 'D:\Test Builds\dcs-tablet.exe'
```

The HTTP and browser tests use this file when the variable is present.
The `DCS_TABLET_PLAYWRIGHT` variable can identify an existing Playwright module.

## Lua connection tests

1. Open PowerShell in the source folder.
2. Enter this command with Lua 5.1 available:

```powershell
lua tests/controls_bridge.lua
```

Result: The test checks all 40 aircraft button mappings and the button-release protections.
The test uses simulated DCS functions.
It does not need a live mission.

## Physical display test

This test temporarily activates a real virtual display.
It does not form part of the normal automated test set.

Before you start, close DCS.
Use a PC with exactly one inactive virtual display.
Complete any pending display recovery first.

1. Enter this command:

```powershell
cargo test --locked -- --ignored virtual_display_roundtrip --test-threads=1 --nocapture
```

Result: The test activates the virtual display and checks its capture identity.
It then restores the display state.
If the test stops unexpectedly, use the normal display recovery procedure.

## Make a Windows ZIP

1. Build the release executable.
2. Enter this command:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\Package.ps1
```

Result: The script creates the versioned ZIP and its SHA-256 file in `dist`.
It also collects dependency license files.
The script stops if the output folder already exists.

Example with a different executable:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\Package.ps1 -Version '0.1.1' -ExecutablePath 'D:\Test Builds\dcs-tablet.exe'
```

## Documentation changes

Use the [writing guide](docs/writing-guide.md) for new text.
Keep user instructions separate from developer information.
Check local links and command examples after each documentation change.
From the source checkout, rebuild and check the offline guide:

```powershell
python -m pip install -r requirements-docs.txt
python scripts/Build-Guide.py
python scripts/Build-Guide.py --check
```

The generator reads the version from Cargo.toml and embeds documentation images.
It checks local links, heading anchors, and image descriptions.
Python and Python-Markdown are documentation tools, not application runtime requirements.

[Test records](docs/validation.md) distinguish completed tests from tests that are still necessary.


## Failure recovery integration

Build the development executable with `cargo build --locked --features test-hooks`.
Then run `node tests/failure_recovery.cjs` with Playwright installed.
The test uses a synthetic source and a temporary fault file; it does not start DCS or change displays.
It checks encoder/capture failures, viewer release, reconnect, sender overflow, and source freshness.
Restart budget and profile upgrades use isolated Rust tests. The Lua 5.1 regression suite runs in a separate CI job.
Rebuild release with default features before packaging.
The package script verifies its source fingerprint and rejects test-hook builds.

For optional source formatting, use Prettier 3.6.2 for the browser page/JavaScript
and StyLua 2.3.1 with Lua51 syntax for the bridge. Neither is an application runtime dependency.

After building both executables, run `powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/package_guards.ps1`.
The packaging script also runs `python scripts/Build-Guide.py --check`, so documentation tools must be available in the packaging shell.

`tests/launcher_logging.ps1` checks owned child startup, pipe draining, and clean exit with a test-only synthetic demo.

For H.264 media tests on Windows Server, set `DCS_TABLET_BROWSER_CHANNEL=chrome`
(or `msedge` for an installed Microsoft Edge browser). The CI runner installs official Chrome;
bundled Chromium codec availability depends on the host. WebKit remains a layout check.
