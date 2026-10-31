# User guide

This guide is for DCS Tablet 0.1.1.
Initial aircraft support is for the **DCS F/A-18C Hornet (F-18) only**.

Complete the tasks in the order given.
Each task gives a result.

## Before you start

Use Windows 10 or Windows 11 x64.
The application uses the H.264 encoder supplied by Windows Media Foundation.
Windows N editions need the [Media Feature Pack](https://support.microsoft.com/en-us/windows/media-feature-pack-for-windows-n-8622b390-4ce6-43c9-9b42-549e5328e407).
Install that feature through Windows settings before you start the demo.

Install the Microsoft Visual C++ v14 **x64** Redistributable if it is absent.
Use [Microsoft's runtime download instructions](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist?view=msvc-170).
The application needs the runtime, not Visual Studio or the C++ Build Tools.
DCS Tablet does not include a runtime installer or a codec DLL.

## 1. Prepare the application folder

Before you start, download the Windows release ZIP from [DCS Tablet Releases](https://github.com/dcssimcoding/dcstablet/releases/tag/v0.1.1).
The file name for this version is `dcs-tablet-0.1.1-windows-x64.zip`.
The GitHub **Source code** ZIP does not contain the application file.

1. Create a folder named **DCS Tablet** on your desktop.
2. Right-click the release ZIP.
3. Click **Extract All**.
4. Select the desktop **DCS Tablet** folder as the destination.
5. Click **Extract**.
6. Open the **DCS Tablet** folder.
7. Make sure that the folder contains `dcs-tablet.exe` and `Start-Demo.cmd`.

Result: The application files are in the desktop **DCS Tablet** folder.

Example folder:

```text
Desktop
  DCS Tablet
    dcs-tablet.exe
    Start-Demo.cmd
    Start-DCS-Tablet.cmd
    Restore-Display.cmd
    START-HERE.html
    docs
    scripts
```

Keep these files together.
Do not start the application from inside the ZIP.

## 2. Prepare the network

1. Connect the PC to your home network.
2. Connect the tablet to the same network.
3. Complete [Network connection](network.md).

Result: Windows permits the tablet to communicate with DCS Tablet.
The PC can use an Ethernet cable while the tablet uses Wi-Fi.
Both devices must use the same local network.

## 3. Do the demo test

The demo uses colored test displays.
Its aircraft buttons do not operate.

1. Open the application folder on the PC.
2. Double-click `Start-Demo.cmd`.
3. Keep the new console window open.
4. Find the address after **LAN:** in that window.
5. Hold the tablet with its long side horizontal.
6. Open the tablet's web browser.
7. Enter the PC address in the browser's address bar.
8. Tap **Connect**.

Example console address:

```text
LAN: http://192.168.1.20:8090/
```

For this example, enter `http://192.168.1.20:8090/` on the tablet.
Your PC address can be different.
Do not use this example address unless it agrees with your PC address.

Result: Two colored displays show moving lines.
The page shows **DEMO** and **Demo · controls disabled**.
The page also shows the application version and build date.

![Example of the demo page in landscape](images/demo-landscape.png)

If the page does not open, use the [problem guide](troubleshooting.md).

## 4. Stop the demo

Stop the demo before you start a DCS Tablet flight.

1. Tap **Disconnect** on the tablet.
2. Click the demo console on the PC.
3. Press **Ctrl+C**.
4. If Windows asks **Terminate batch job**, enter `Y`.
5. If the console asks for a key, press **Enter**.

Result: The demo stops.
Its network address is available for the flight application.

## 5. Prepare the virtual display

1. Complete [Virtual display](virtual-display.md).

Result: Windows has a virtual display with a **1024 × 512** display mode.
The physical monitor remains the main display.

The virtual display supplies space for DCS to draw both DDIs.
It does not require another physical monitor.

## 6. Open PowerShell in the application folder

PowerShell is a Windows application for text commands.
You use it for the first setup.
Normal flights use a command file that you can double-click.

1. Open the application folder in File Explorer.
2. Click the File Explorer address bar.
3. Enter `powershell`.
4. Press **Enter**.

Result: A PowerShell window opens in the application folder.
The end of its folder address shows `DCS Tablet`.

For each command in this guide:

1. Copy the complete command line.
2. Paste the line into PowerShell.
3. Press **Enter**.

Do not copy the code block's label, such as `powershell`.

## 7. Prepare the DCS profile

Before the first setup, start your normal DCS installation once.
Exit DCS through its normal menu.
Keep DCS closed for setup.
The first setup reads settings from your normal DCS profile.
Before setup, copy that profile to a separate backup folder outside Saved Games and the application folder.
Keep the original profile in place. Check that the backup contains your Config, Scripts, and any personal missions.
[Data safety](data-safety.md) explains what the application changes and what the backup protects.
Run setup and flights as a normal user; only the documented firewall/driver tasks need administrator rights.

1. Enter this command:

```powershell
.\dcs-tablet.exe setup
```

2. Wait for **Setup complete**.

Result: DCS Tablet creates a separate profile named `DCS.Tablet`.
It copies your initial graphics settings and controller settings.
It adds the DDI display settings and the DDI control connection.
Your normal DCS profile remains separate. The DCS installation files are not modified.
Later changes to your normal profile do not update DCS.Tablet automatically.

The application does not copy missions, aircraft modifications, or other export scripts.
DCS can ask for your account details when you first use the new profile.

If setup cannot find DCS, use the [command examples](commands.md).
If setup finds more than one virtual display, use the display selection example on that page.

## 8. Start an F-18 flight

Before you start, make sure that the demo has stopped.
Keep the virtual display available to Windows.
The application activates that display when necessary.

1. Close the setup PowerShell window.
2. Double-click `Start-DCS-Tablet.cmd`.
3. Keep the application console open.
4. Wait for DCS World to open.
5. Start a mission with the F/A-18C Hornet.
6. Switch on the aircraft DDIs.
7. Open the application address on the tablet.
8. Tap **Connect**.
9. Wait for **Touch controls ready**.

Result: The tablet shows the left and right aircraft DDIs.
A touch button operates the corresponding aircraft button.

For the first check, use a powered aircraft in a training mission.
A dark aircraft display can give a dark tablet display.

Do not minimize the DCS window during a flight.
Do not move another window over the virtual display's DDI areas.
The tablet shows the pixels in those areas.

## 9. Use the tablet controls

| Task | Action |
| --- | --- |
| Show only the left DDI | Tap the left display image. |
| Show only the right DDI | Tap the right display image. |
| Show both DDIs | Tap **Both DDIs**. |
| Operate an aircraft button | Tap its button around the display. |
| Increase image brightness | Tap **+** beside **DDI brightness**. |
| Decrease image brightness | Tap **−** beside **DDI brightness**. |
| Hide the application header | Tap **Full screen**. |
| Show the application header | Tap **Exit full screen**. |
| End the tablet connection | Tap **Disconnect**. |

The brightness control changes only the tablet image.
It does not change the aircraft's brightness setting.
The browser can keep some browser controls visible in full screen.

A continuous button press has a two-second limit.
Release the button before another press.
Keep the tablet in landscape position.

## 10. End a flight

1. Tap **Disconnect** on the tablet.
2. Exit DCS World through its normal exit menu.
3. Wait for the DCS Tablet console to finish.
4. If the console asks for a key, press **Enter**.

Result: DCS Tablet stops its video service.
It returns the virtual display to its previous state.

If the virtual display was inactive before the flight, it becomes inactive again.
If it was active, it can remain active.

If the application stops unexpectedly, use [Recovery and removal](removal.md).
