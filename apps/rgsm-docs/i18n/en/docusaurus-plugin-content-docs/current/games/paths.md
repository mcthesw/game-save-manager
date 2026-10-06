# Save locations and path variables

## Edit save locations

1. Open a game and click **View managed files**.
2. Select a device, then edit save locations or add entries.
3. Click **Save**.

Files, folders, and Windows registry entries are supported. Enable **Delete before overwrite** when you want a folder cleared before its contents are restored.

## Use path variables

Type `<` in a path field and choose a suggested variable. The field shows the resolved path on this device.

For example, `<home>/Saved Games/MyGame` uses the current user's home folder.

Detected dynamic paths match files on the current device. Add game library folders and installation locations in **Settings → Auto Scan**.

## Choosing between game libraries

`<root>` is a game library root. The relative path after it is appended as written. For a game at `H:\Dying Light\DyingLightGame.exe` with library root `H:\`, use `<root>/Dying Light/DyingLightGame.exe` as the launch path.

When several locations are available, select the library in the add or import dialog. For an existing game, use **View managed files → Game root directory**. A single candidate is used automatically. File existence does not decide which library to use.

The selection is saved with the paths for this game and this device. Another device can use a different library while keeping the same relative path. Launch paths require one location; dynamic save patterns can explicitly select several libraries or accounts.

Check the resolved path below the input. If it says “Select a location”, choose one first. If the file does not exist, check the path or run the game to create a save before backing up. You can also pick the actual file or folder directly.

## Device paths

Name your devices in **Settings → Device Management**. To reuse another device's paths, click its **Import Paths** action. Confirming replaces the corresponding path settings on this device.
