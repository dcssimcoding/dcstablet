# Problem guide

Use the procedure for the symptom that you see.
The application initially supports only the DCS F/A-18C Hornet.

## The application does not open

If Windows reports that `VCRUNTIME140.dll` is missing:

1. Open [Microsoft's runtime download instructions](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist?view=msvc-170).
2. Download the Visual C++ v14 **x64** Redistributable from Microsoft.
3. Run its installer.
4. Start DCS Tablet again.

Result: Windows can load the application's native runtime.
Do not download individual DLL files from unofficial websites.

## The Windows H.264 encoder is unavailable

1. Make sure that you use Windows 10 or Windows 11.
2. On a Windows N edition, install the [Media Feature Pack](https://support.microsoft.com/en-us/windows/media-feature-pack-for-windows-n-8622b390-4ce6-43c9-9b42-549e5328e407).
3. Restart Windows if the feature installer requests it.
4. Start the demo again.

Result: The demo uses the H.264 encoder supplied by Windows.
The application does not download an alternative codec.

## The tablet page does not open

1. Make sure that the application console is open on the PC.
2. Make sure that the tablet uses the same network as the PC.
3. Compare the browser address with the PC's active IPv4 address.
4. Include `http://` and `:8090/` in the address.
5. Complete the [network connection tasks](network.md).
6. Open the address again.

Result: The DCS Tablet page opens.

Example of a complete address:

```text
http://192.168.1.20:8090/
```

If a VPN changes the selected address, use the Wi-Fi or Ethernet address instead.
A guest network can prevent the connection even when both devices have Internet access.

## The page opens, but video does not start

1. Tap **Disconnect** on any other device that uses DCS Tablet.
2. Tap **Connect** on the intended tablet.
3. If video remains absent, check the firewall rule.
4. If necessary, do the demo test again.

Result: The intended tablet shows moving images.

The firewall rule must permit this application file.
A rule for only TCP port 8090 can permit the page but prevent video.
The supplied firewall script also permits the video traffic.

## The page shows DEMO instead of the aircraft

1. Stop the demo as given in the [user guide](user-guide.md#4-stop-the-demo).
2. Double-click `Start-DCS-Tablet.cmd`.
3. Start an F/A-18C mission.
4. Tap **Connect** on the tablet.

Result: The page shows the aircraft's DDI images.

## The aircraft displays are dark

1. Make sure that the mission uses the F/A-18C Hornet.
2. Make sure that the aircraft has electrical power.
3. Switch on both aircraft DDIs.
4. Make sure that the DCS window is not minimized.
5. Make sure that another window does not cover the virtual display's DDI areas.

Result: The tablet also shows the powered aircraft display.

If both images remain dark, close DCS normally.
Use the [recovery procedure](removal.md#restore-the-display) before another attempt.

## The touch buttons do not operate

1. Make sure that the mission uses the F/A-18C Hornet.
2. Make sure that you started DCS through `Start-DCS-Tablet.cmd`.
3. Wait for **Touch controls ready** on the tablet.
4. Release all tablet buttons.
5. Tap one DDI button again.

Result: The corresponding aircraft button operates.

The demo does not operate aircraft buttons.
A continuous press stops after two seconds.
A lost connection also causes the application to release held buttons.

## Video works, but touch controls become unavailable

Video and touch controls use separate connections.
A control status change does not necessarily mean that video has disconnected.

1. Use version **0.1.1** or later.
2. If necessary, exit DCS normally before you replace the application files.
3. Start the updated application through `Start-DCS-Tablet.cmd`.
4. Reload the tablet page.
5. Tap **Connect**.

Result: **Touch controls ready** remains visible during normal Hornet operation.

Early development builds could delay control acknowledgements after a pause.
Current releases drain old replies to avoid this backlog.
**DCS controls unavailable · waiting for a reply** identifies an unavailable control connection.
**Start a Hornet mission for touch controls** identifies a responding connection without an active Hornet.

## Setup cannot find DCS

1. Find `DCS.exe` in your DCS installation folder.
2. Use the [DCS location example](commands.md#select-a-dcs-installation).

Result: Setup finds the application file and creates the separate profile.

## Setup cannot find the source profile

A source profile contains your normal DCS settings.
Its usual folder is **Saved Games\DCS**.
Some installations use **DCS.openbeta** or another folder name.

1. Start your normal DCS installation once.
2. Exit DCS normally.
3. Find the profile folder that contains `Config\options.lua`.
4. Use the [source profile example](commands.md#select-a-source-profile).

Result: Setup copies the correct initial settings.

## Setup finds more than one virtual display

1. Use the [display selection example](commands.md#select-a-virtual-display).

Result: Setup uses only the display that you selected.

## The console says that port 8090 is in use

1. Stop any previous DCS Tablet demo.
2. Close any previous DCS Tablet flight normally.
3. Start the intended session again.

Result: One DCS Tablet session uses the address.

If another application needs that port, use the [port example](commands.md#use-another-network-port).
Do not stop an unknown Windows process.

## The console requests display recovery

1. Close DCS World.
2. Double-click `Restore-Display.cmd`.
3. Read the result in the console.

Result: The application restores the previous virtual display state.
If restoration stops, use [Recovery and removal](removal.md).

## The tablet requests landscape position

1. Hold the tablet with its long side horizontal.
2. If necessary, release the tablet's rotation lock.

Result: The page removes the rotation message.
Use landscape position with this version.

## The image stops after the tablet locks

1. Unlock the tablet.
2. Return to the DCS Tablet page.
3. Tap **Connect** if necessary.

Result: The live images continue.
The tablet can stop its browser when the page is not visible.

## Collect information for help

Before you collect the report, close the DCS Tablet flight console.
The `doctor` command cannot use the session lock while a managed flight is active.

1. Open PowerShell in the application folder.
2. Enter this command:

```powershell
.\dcs-tablet.exe doctor
```

3. Record the version and build date.
4. Copy the exact error message.
5. Describe the last action before the problem.
6. Remove personal details before you share the report.

The report can include your Windows user name or folder paths.
The local log folder is `%LOCALAPPDATA%\DCS Tablet`.
[Command examples](commands.md) explain how to open that folder.


## Setup or removal refuses a linked path

Do not replace the managed DCS.Tablet folders or files with junctions, symbolic links, or hard links.
The application requires independent writable copies and refuses links to protect the normal profile.
Leave all folders intact. Use an ordinary copied source profile if your original input setup uses linked directories.
Do not edit the ownership manifest or delete the original profile to bypass this check.
See [Data safety](data-safety.md) and [Recovery and removal](removal.md).
