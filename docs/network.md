# Network connection

Complete this task before the demo's first connection.
DCS Tablet permits one connected viewer at a time.

## Before you start

Use a private network with devices that you trust.
Do not use public Wi-Fi for DCS Tablet.
Do not use a guest network that prevents communication between devices.

DCS Tablet has no password or tablet approval step.
A reachable device can request the video connection.
The active viewer can also operate the DDI buttons during a flight.

The page and control messages use HTTP without encryption.
WebRTC encrypts the video connection.
These protections do not identify the person who connects.

## 1. Set the Windows network profile

Before you change the profile, make sure that you trust the network and its devices.

1. Open Windows **Settings**.
2. Open **Network & internet**.
3. For Wi-Fi, open the connected Wi-Fi network's properties.
4. For a cable connection, open **Ethernet**.
5. Set **Network profile type** to **Private**.

Result: Windows identifies your connection as a private network.
Windows labels can be different between versions.
[Microsoft's network instructions](https://support.microsoft.com/en-gb/windows/experience/connectivity-networking/essential-network-settings-and-tasks-in-windows) show the current controls.

## 2. Open Administrator PowerShell

This task adds a Windows Firewall rule.
Normal flights do not need Administrator PowerShell.

The commands below assume that the application folder is **DCS Tablet** on your desktop.

1. Open the Windows **Start** menu.
2. Enter `PowerShell` in the search field.
3. Right-click **Windows PowerShell**.
4. Click **Run as administrator**.
5. If Windows asks for approval, click **Yes**.
6. Enter this command:

```powershell
Set-Location -LiteralPath (Join-Path ([Environment]::GetFolderPath('Desktop')) 'DCS Tablet')
```

Result: Administrator PowerShell uses the application folder.
If your application folder is elsewhere, use its full folder path instead.

Example:

```powershell
Set-Location -LiteralPath 'D:\Flight Tools\DCS Tablet'
```

## 3. Add the firewall rule

1. Enter this command in Administrator PowerShell:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\Allow-Private-Network.ps1 -ExecutablePath "$PWD\dcs-tablet.exe"
```

2. Wait for **Private local-subnet access enabled**.
3. Close Administrator PowerShell.

Result: The rule permits nearby network devices to reach this application on private networks.
It does not permit all Windows applications.

The execution policy option applies only to the PowerShell process that executes this command.
It does not change the saved Windows script policy.

Repeat this task if you move `dcs-tablet.exe` to another folder.
The firewall rule contains the application file's full path.
Do not turn off Windows Firewall.

## 4. Find the correct PC address

The demo console shows a **LAN:** address.
A VPN or virtual network adapter can cause an incorrect address.

If necessary, find the address as follows:

1. Open a normal PowerShell window.
2. Enter this command:

```powershell
ipconfig
```

3. Find the active **Wi-Fi** or **Ethernet** adapter.
4. Find its **IPv4 Address**.
5. Add that address between `http://` and `:8090/` in the tablet browser.

Example:

```text
PC IPv4 Address: 192.168.1.20
Tablet address:  http://192.168.1.20:8090/
```

The tablet cannot use the PC's `127.0.0.1` address.
That address refers to the device on which you enter it.
The tablet browser needs the PC's network address.

## During use

Keep the tablet page visible.
If the tablet stops the browser in the background, return to the page.
Tap **Connect** again if necessary.

The browser can prevent automatic screen locking on some devices.
If necessary, change the tablet's automatic screen-lock setting for the flight.

## Connection limits

DCS Tablet closes incomplete request headers after two seconds.
It closes incomplete request bodies after three seconds.
These limits prevent slow requests from holding application workers.
They do not add authentication or HTTP encryption.

## Next task

Continue with the [demo](user-guide.md#3-do-the-demo-test).
If the page does not open, use the [problem guide](troubleshooting.md).
