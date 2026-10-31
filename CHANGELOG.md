# Version history

Initial aircraft support is for the **DCS F/A-18C Hornet only**.
Basic live cockpit video and touch controls have been checked.
Extended tablet and complete managed flight tests are still necessary.

## Version 0.1.1 — profile and data safety

Profile removal now archives the entire dedicated profile without deleting files.
Edited bindings, missions, credentials, and generated files remain in the backup.
Managed paths reject junctions, symbolic links, and shared hard-linked files.
Setup exclusively creates a new profile and verifies the actual Windows Saved Games location.
Atomic state writes use unique, exclusively created temporary files and synchronized contents.
Backup moves never replace an existing destination.
Display recovery checks for overlaps and verifies the restored state before deleting its record.
The instructions add a pre-setup backup and remove destructive profile-removal advice.

## Version 0.1.0

### Reliability and maintainability review

Browser status and control requests are isolated between connection generations.
Playback failures now enable retry. Encoded-frame overflow requests a fresh keyframe.
Capture freshness is reported separately from service availability.
Setup upgrades an unchanged managed bridge while preserving user edits.
Saved settings, profile manifests, and display recovery records have typed schema validation.
Packaging checks the source fingerprint, target, production features, and generated guide.
Fault injection, browser recovery regressions, package guards, and Lua tests are included in CI.
Source documentation explains ownership and protocol invariants; browser/Lua formatting is readable.
Logs rotate during sessions and identify their build.

### Windows encoder and documentation

Windows Media Foundation now provides H.264 encoding.
The application no longer builds or distributes OpenH264.
The browser protocol remains H.264 over WebRTC.
The documentation identifies the Visual C++ runtime and Windows N media feature requirements.
The source includes the offline-guide generator and its pinned documentation dependency.

### Control connection correction

The control connection now drains queued UDP replies before the next polling cycle.
Old replies no longer delay acknowledgement of a new button press or release.
The page distinguishes an unavailable DCS control connection from a mission that has not started.
The status response includes control acknowledgement age for diagnostics.

### HTTP service improvements

The HTTP service now receives complete requests before it gives them to application workers.
Slow request headers and bodies have fixed time limits.
Connection counts and request queues have fixed capacity limits.
Health checks and shutdown continue while requests remain incomplete.

The HTTP layer now uses httparse instead of tiny_http.
Six network regression scenarios check the availability protections.
The Windows workflow includes these tests.

### Initial functionality

The first development build included:

- Windows capture and H.264 video through WebRTC.
- Touch controls for the two F/A-18C DDIs.
- Native virtual display control and display recovery.
- A separate DCS.Tablet profile.
- Landscape operation and a portrait rotation message.
- A visible version and UTC build date.
- Windows package preparation and GitHub workflow files.

## Documentation revision

The user documentation now gives numbered tasks, examples, results, and a glossary.
It uses ASD-STE100 writing rules.
`START-HERE.html` gives the user instructions in a web browser.
This version includes the control connection correction described above.
