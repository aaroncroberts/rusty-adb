# Device Details

The Device Details dialog gives you a rich view of your connected Android device — hardware info, storage breakdown, installed apps, and network status — all fetched live via ADB.

---

## Table of Contents

- [Opening Device Details](#opening-device-details)
- [Overview Tab](#overview-tab)
- [Storage Tab](#storage-tab)
- [Apps Tab](#apps-tab)
- [Network Tab](#network-tab)
- [Refreshing](#refreshing)

---

## Opening Device Details

Click the **connection status text** in the bottom status bar when a device is connected:

```
Connected — Pixel 7      Copying 1/3 · 42%    Logs
^^^^^^^^^^^^^^^^^^^^^^^
      click here
```

The Device Details dialog opens as a full-window overlay. Close it with the **Close** button or press `Escape`.

> The connection status is only clickable when the device state is **Connected**. If the status bar shows *Disconnected*, *Unauthorized*, or *Connecting*, the click has no effect.

---

## Overview Tab

The Overview tab shows fundamental hardware and software information fetched from the device's system properties.

| Field | Example | Source |
|---|---|---|
| **Model** | Pixel 7 | `ro.product.model` |
| **Manufacturer** | Google | `ro.product.manufacturer` |
| **Brand** | google | `ro.product.brand` |
| **Android version** | 14 | `ro.build.version.release` |
| **API level** | 34 | `ro.build.version.sdk` |
| **Security patch** | 2024-01-01 | `ro.build.version.security_patch` |
| **Build fingerprint** | google/pixel7/... | `ro.build.fingerprint` |
| **Kernel version** | 5.15.131-android13 | `uname -r` |
| **System uptime** | up 2 days, 3:45 | `uptime` |
| **SoC** | Qualcomm SM8550-AB | `ro.soc.manufacturer` + `ro.soc.model` |
| **CPU ABI** | arm64-v8a | `ro.product.cpu.abi` |
| **Total RAM** | 7.4 GB | `proc/meminfo` |
| **Screen resolution** | 1080×2400 | `wm size` |
| **Screen density** | 420 dpi | `wm density` |
| **Serial** | R5CWA0ABC123 | device serial |
| **Connection** | USB / TCP/IP / Emulator | inferred from serial |
| **Battery** | 87% | `dumpsys battery` |

Fields shown as `—` indicate the property isn't available on this device or Android version.

---

## Storage Tab

The Storage tab shows how storage space is allocated on the device.

| Field | Description |
|---|---|
| **Internal storage** | Total / used / free on `/storage/emulated/0` |
| **SD Card** | Total / used / free (if an external card is present) |
| **Data partition** | `/data` partition usage |

Space values are shown in human-readable form (GB, MB). The percentage bar gives a quick visual indication of how full each volume is.

> **No SD Card row?** Your device either doesn't have an external card inserted, or the card isn't mounted.

---

## Apps Tab

The Apps tab lists every **user-installed** app on the device (third-party packages only — system apps are excluded).

Each row shows:
- **Package ID** — the app's unique identifier (e.g. `com.whatsapp`)
- **APK path** — where the APK is stored on the device

**To uninstall an app from here:**
Select the app in the list, then click **Uninstall**. A confirmation prompt appears before anything is removed. See [APK & App Management](apk-management.md#uninstalling-an-app) for full details.

---

## Network Tab

The Network tab shows how the device is connected.

| Field | Description |
|---|---|
| **Connection type** | USB, TCP/IP, or Emulator |
| **IP address** | WiFi IP address (`wlan0`) — shown when connected over WiFi |
| **Serial** | ADB device serial number |

> **TCP/IP connections:** If you've connected via `adb connect <ip>:<port>`, the serial will show the IP address and the Connection type will be **TCP/IP**.

---

## Refreshing

Device details are fetched once when the dialog opens. All ADB property queries run in parallel, so the dialog loads quickly even with many fields.

To re-fetch all details (e.g. after plugging in a charger and wanting to see updated battery), click the **Refresh** button in the dialog toolbar.

---

← [APK & App Management](apk-management.md) · [Documentation Index](README.md) · [Settings & Logs →](settings.md)
