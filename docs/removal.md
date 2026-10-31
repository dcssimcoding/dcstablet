# Recovery and removal

Display recovery returns the virtual display to its previous state.
Profile removal preserves the complete profile in an archive.
Application removal is a separate task.

## Restore the display

Use this task after an unexpected stop or a request for recovery.

1. Close DCS World.
2. Open the application folder.
3. Double-click `Restore-Display.cmd`.
4. Read the console message.
5. If the console asks for a key, press **Enter**.

Result: The console reports restoration or **No display recovery pending**.

Do not delete `display-recovery.json` before restoration is complete.
This file contains the display state that the application must restore.

## If automatic restoration stops

A different display arrangement can prevent automatic restoration.
The application stops rather than replace a new arrangement.

1. Close DCS World.
2. Open Windows **Settings** > **System** > **Display**.
3. Identify the physical monitor and the virtual display.
4. Make sure that the physical monitor is the main display.
5. Return the virtual display to its previous state.
6. Double-click `Restore-Display.cmd` again.
7. If the warning continues, keep the recovery file for technical support.

Result: Your physical monitor operates in its normal arrangement.

If necessary, move a window back from the virtual display with **Windows+Shift+Left Arrow**.
The required arrow key depends on the display arrangement.

## Remove the DCS Tablet profile

Before you start, complete display recovery and close DCS World.

1. Open PowerShell in the application folder.
2. Enter this command:

```powershell
.\dcs-tablet.exe uninstall-profile
```

3. Read and keep the printed backup path.

Result: The application **renames the complete DCS.Tablet folder into a backup** beside it.
The name begins with `DCS.Tablet.backup-` and has a unique suffix.
No profile files are deleted. Edited controller bindings, missions, login files, and DCS-generated files stay in the archive.
The move stays on the same volume, so it does not need space for a second full copy.
A later setup creates a new independent DCS.Tablet profile.

Keep this archive until you have checked that everything you need is present.
It can contain account/login data: do not upload it publicly or include it in a support report.
An archive on the same drive is not protection against drive failure; keep a separate backup of important personal files.

The application refuses unowned profiles, invalid records, and linked managed paths.
If it refuses removal, **leave the folder intact**. Do not delete files, edit the ownership record, or force the operation.
For a manual fallback, close DCS and rename only the actual DCS.Tablet folder to an unused backup name in File Explorer.
Never merge with or replace an existing backup. Do not change your normal DCS or DCS.openbeta folders.
If you are unsure which folder is the dedicated profile, stop and request help.

## Remove the firewall rule

1. Open Administrator PowerShell as given in [Network connection](network.md#2-open-administrator-powershell).
2. Enter this command from the application folder:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\Allow-Private-Network.ps1 -Remove
```

3. Close Administrator PowerShell.

Result: Windows removes the **DCS Tablet private LAN** rule.

## Remove the application

1. Make sure that DCS Tablet has stopped.
2. Keep any local logs that you need.
3. Delete the extracted application folder.
4. If you no longer need its settings, delete `%LOCALAPPDATA%\DCS Tablet`.

Result: The application and its selected local settings are absent.

The virtual display driver is a separate application component.
If no other application needs it, use the driver's own removal instructions.
Do not delete driver files manually.
