# Command examples

Most users need these commands only during the first setup.
The daily flight instructions use command files that you can double-click.

Before you enter a command, open PowerShell in the application folder.
[The user guide](user-guide.md#6-open-powershell-in-the-application-folder) gives the steps.
Close DCS before setup, recovery, or profile removal.

File paths in these examples are examples only.
Keep quotation marks around a path that contains spaces.
Use the file or folder that exists on your PC.

## Select a DCS installation

Example: DCS is installed in `D:\DCS World`.

```powershell
.\dcs-tablet.exe setup --dcs 'D:\DCS World\bin\DCS.exe'
```

Result: Setup uses that `DCS.exe` file.
The command must identify the application file, not only its folder.

## Select a source profile

Example: Your normal profile is **DCS.openbeta** in Saved Games.

```powershell
.\dcs-tablet.exe setup --source-profile "$env:USERPROFILE\Saved Games\DCS.openbeta"
```

Result: Setup copies the initial settings from **DCS.openbeta**.
The source folder must contain `Config\options.lua`.

The command above assumes the usual Saved Games location.
If Windows stores your Saved Games elsewhere, use that folder's actual path.

## Select a virtual display

1. Enter this command:

```powershell
.\dcs-tablet.exe displays | ConvertFrom-Json | Format-Table device, description, active, primary
```

2. Find the entry for the intended virtual display.
3. Record its device name.
4. Enter a setup command with that name.

Example display names:

```text
device        description              active primary
------        -----------              ------ -------
\\.\DISPLAY1  Physical display adapter   True    True
\\.\DISPLAY5  Virtual Display Driver    False   False
```

For this example:

```powershell
.\dcs-tablet.exe setup --display '\\.\DISPLAY5'
```

Result: Setup selects the fifth display's device name.
Your virtual display can have a different number.
The `primary` value must be `False` for the selected display.

## Use another Saved Games location

Windows normally identifies this location automatically.
Use this option only for the actual Saved Games location that DCS uses.
It cannot make DCS use an arbitrary new storage folder.
Setup checks this path against Windows Saved Games and refuses a different folder.
Do not pass the DCS installation directory or the DCS profile itself.

Example:

```powershell
.\dcs-tablet.exe setup --saved-games 'D:\Saved Games' --source-profile 'D:\Saved Games\DCS'
```

The application still creates a profile named **DCS.Tablet**.

## Combine setup options

Example: DCS and Saved Games use separate folders.

```powershell
.\dcs-tablet.exe setup --dcs 'D:\DCS World\bin\DCS.exe' --saved-games 'E:\Saved Games' --source-profile 'E:\Saved Games\DCS' --display '\\.\DISPLAY5'
```

Result: Setup uses all four selected locations or settings.
Enter the complete command as one line.

## Use another network port

Example: Another application uses port 8090.

For a temporary demo:

```powershell
.\dcs-tablet.exe --demo --port 8092
```

For future managed flights:

```powershell
.\dcs-tablet.exe setup --port 8092
```

The tablet address then ends with `:8092/`.
For example, use `http://192.168.1.20:8092/`.

The saved setup port applies to managed flights.
The demo still uses port 8090 unless you give the demo its own `--port` option.

## Show the version

```powershell
.\dcs-tablet.exe --version
```

The output includes the version, UTC build date, and build ID.
UTC is the time reference for the build date.

## Open the local settings folder

1. Press **Windows+R**.
2. Enter `%LOCALAPPDATA%\DCS Tablet`.
3. Press **Enter**.

Result: File Explorer opens the local settings folder.

| File | Purpose |
| --- | --- |
| `settings.json` | Saved application setup |
| `capture.json` | Generated display capture settings |
| `stream.log` | Video service messages |
| `display-recovery.json` | Display restoration data |
| `session.lock` | Prevents simultaneous managed sessions |

Do not delete the recovery file while recovery is still necessary.

## Command reference

| Command | Task |
| --- | --- |
| `--demo` | Start the colored test displays. |
| `displays` | Show Windows display information. |
| `setup` | Prepare the separate DCS profile. |
| `launch` | Start a managed DCS flight. |
| `restore` | Restore the previous virtual display state. |
| `uninstall-profile` | Archive the whole dedicated profile without deleting files. |
| `doctor` | Show information for a problem report. |
| `--version` | Show the version and build date. |
| `--help` | Show command help. |

For normal flights, `Start-DCS-Tablet.cmd` executes `launch`.
`Restore-Display.cmd` executes `restore`.
