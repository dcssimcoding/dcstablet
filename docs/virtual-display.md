# Virtual display

A virtual display is an extra Windows screen without a physical monitor.
DCS uses this screen to draw both DDIs outside your physical screen.
DCS Tablet then sends those images to the tablet.

The virtual display must be active during a flight.
Space outside all active Windows displays does not contain an image that the application can capture.

## Required settings

| Setting | Required value |
| --- | --- |
| Virtual display size | 1024 × 512 pixels |
| Position during a flight | Immediately right of the main display |
| Display arrangement | Extend |
| Main display | Your physical monitor |
| HDR | Off on the virtual display |

Example arrangement:

```text
Physical monitor              Virtual display
+-------------------------+   +-------------------+
|                         |   | Left DDI | Right  |
|     DCS cockpit         |   | 512x512  | DDI    |
|                         |   |         | 512x512|
+-------------------------+   +-------------------+
Visible on your desk          Extra drawing space
```

For a 1920 × 1080 physical monitor, the complete DCS image is 2944 × 1080 pixels.
The cockpit still uses the physical monitor's area.
The tablet receives the two DDI areas.

## 1. Install the driver

Before you start, close DCS World.
If the driver is already installed, continue with task 2.

1. Open the [Virtual Display Driver release page](https://github.com/VirtualDrivers/Virtual-Display-Driver/releases).
2. Get the installer identified in the driver's instructions.
3. Open the installer.
4. Follow its installation instructions.
5. If Windows asks for administrator approval, check the file's source before you continue.
6. If the source is correct, click **Yes**.

Result: Windows has an extra virtual display adapter.
The [driver instructions](https://github.com/VirtualDrivers/Virtual-Display-Driver#readme) identify its installation controls.
DCS Tablet does not include or install the driver.

## 2. Check the display mode

1. Open Windows **Settings**.
2. Open **System** > **Display**.
3. Click **Identify**.
4. Select the virtual display's numbered rectangle.
5. Set its display arrangement to **Extend**.
6. Select **1024 × 512** in **Display resolution**.
7. If Windows asks to keep the change, click **Keep changes**.
8. Set HDR to **Off** for this display.
9. Make sure that your physical monitor remains the main display.

Result: The virtual display has the required size.
If **1024 × 512** is unavailable, complete task 3.
Otherwise, continue with task 4.

## 3. Add the missing display mode

Use this task only if the driver's settings do not offer **1024 × 512**.
It adds one resolution to the driver's existing configuration file.

The usual file is `C:\VirtualDisplayDriver\vdd_settings.xml`.
If this file is absent, use the location specified by the driver's documentation.
Do not create a replacement file in an unrelated folder.

1. Open the configuration file's folder in File Explorer.
2. Select `vdd_settings.xml`.
3. Press **Ctrl+C**.
4. Press **Ctrl+V** to make a backup copy.
5. Open the Windows **Start** menu.
6. Enter `Notepad` in the search field.
7. Right-click **Notepad**.
8. Click **Run as administrator**.
9. Click **File** > **Open** in Notepad.
10. Enter the original configuration file's full path in **File name**.
11. Click **Open**.
12. Find the closing line `</resolutions>`.
13. Add the following entry immediately before that closing line:

```xml
<resolution>
  <width>1024</width>
  <height>512</height>
  <refresh_rate>60</refresh_rate>
</resolution>
```

14. Keep the other entries in the file.
15. Save the file.
16. Close Notepad.
17. Save your work in other applications.
18. Restart Windows.
19. Repeat task 2.

Result: Windows offers **1024 × 512** for the virtual display.

The [driver configuration example](https://github.com/VirtualDrivers/Virtual-Display-Driver/blob/master/Virtual%20Display%20Driver%20%28HDR%29/vdd_settings.xml) shows the XML structure.
The example above is an entry, not a complete configuration file.

## 4. Prepare for automatic restoration

The application records the virtual display's state before each flight.
It restores that state after DCS closes.

If you want the virtual display inactive between flights:

1. Select only the virtual display in Windows Display settings.
2. Select **Disconnect this display**.
3. If Windows asks to keep the change, click **Keep changes**.

Result: The driver remains installed, but the virtual display is inactive.
DCS Tablet activates it when you start a flight.

Do not disconnect your physical monitor.
The Windows control can have a different name in another Windows version.

## More than one virtual display

DCS Tablet selects a virtual display automatically when it finds only one.
If it finds more than one, use the [display selection example](commands.md#select-a-virtual-display).

The application places its selected display to the right of the main display.
Another active monitor must not occupy the same area.

## During a flight

Keep the DCS window visible.
Do not cover the DDI areas with other windows.
The application captures those screen areas, including another window that covers them.

## Next task

Continue with [DCS profile setup](user-guide.md#7-prepare-the-dcs-profile).
For an unexpected stop, use [Recovery and removal](removal.md).
