# Data safety

DCS Tablet uses a separate profile and does not install files into DCS World.
This page describes the protection boundaries. It is not a guarantee against drive failure, interrupted writes, or changes made by other software.

## Before setup

1. Exit DCS normally.
2. Copy your normal Saved Games profile (DCS or DCS.openbeta) to a separate backup folder outside Saved Games and the application folder.
3. Check that you can open the copy and that it contains your Config, Scripts, and personal missions.
4. Keep the original profile in place. Do not move or delete it.

If you have irreplaceable missions or controller settings, keep a backup on another drive too.
Run setup and flights without administrator privileges. Driver installation and the documented firewall task are the separate exceptions.

## What the application changes

| Location | Operation |
| --- | --- |
| DCS World installation | Reads/starts DCS.exe; no installation files are rewritten or removed. |
| Normal Saved Games profile | Reads initial graphics settings and input bindings; does not rewrite them. |
| Saved Games/DCS.Tablet | Creates an independent profile and its ownership record; upgrades only unchanged owned bridge content. |
| LocalAppData/DCS Tablet | Saves application settings, capture configuration, logs, lock, and display recovery data. |
| Selected virtual display | Temporarily enables/positions it; restores its recorded state after normal DCS exit. |
| Windows Firewall | The separate administrator script adds/removes only the named program-specific private-LAN rule. |

Setup refuses an existing unowned DCS.Tablet folder and claims new profile directories exclusively.
The Saved Games override must resolve to the real Windows Saved Games folder, not a game installation or a profile subfolder.
Managed profile paths must not contain junctions, symbolic links, or shared hard-linked files.
This prevents a shared configuration path from redirecting modifications into another profile.
Use independent copies rather than linking DCS.Tablet to the normal profile.

## Removal preserves the profile

In version 0.1.1, uninstall-profile archives the **whole profile** to a unique DCS.Tablet.backup-* sibling folder.
It does not delete its files. User edits, missions, bindings, credentials, and generated files stay together.
An existing backup is never replaced. Keep the printed path and inspect the archive before deciding what you no longer need.
Do not upload this archive: it can contain account information.
A same-drive archive is not a second independent copy and does not protect against a failed drive.

If automatic removal refuses a path or manifest, leave the data intact and use the non-destructive guidance in [Recovery and removal](removal.md).
Do not delete the normal DCS profile or force removal by editing the manifest.

## Display and driver recovery

Keep the launcher console open during a flight. Exit DCS normally so it can restore the virtual display.
After an interruption, close DCS and run Restore-Display.cmd.
Keep display-recovery.json until restoration is verified. The application retains that record when recovery cannot safely complete.
A new or overlapping display arrangement requires manual review rather than being overwritten.

Before installing/configuring a third-party virtual display driver, record your current Windows display layout and back up its settings file as described in [Virtual display](virtual-display.md).
The driver is installed separately and can affect Windows independently of DCS Tablet.
Avoid changing monitor arrangements while the managed session is active.

## What is tested

Temporary fixtures exercise ownership rejection, linked profile/input folders, shared files, temporary-file collisions, existing backup destinations, interrupted setup, and archival of edited/personal files.
Tests compare file contents before and after operations; your real profile is not used for destructive tests.
Hardware checks and live cockpit results are recorded separately in [Test record](validation.md).
