# APK & App Management

rusty-adb lets you install APK files directly to your connected device and manage installed apps — all without leaving the app.

---

## Table of Contents

- [Installing an APK](#installing-an-apk)
  - [Step-by-step walkthrough](#step-by-step-walkthrough)
  - [What "Install APK" does](#what-install-apk-does)
  - [Troubleshooting installs](#troubleshooting-installs)
- [Viewing Installed Apps](#viewing-installed-apps)
- [Uninstalling an App](#uninstalling-an-app)

---

## Installing an APK

The **Install APK** button lets you sideload any `.apk` file from your local filesystem to the connected Android device using `adb install -r`.

### Step-by-step walkthrough

**1. Navigate to the APK in the local pane**

In the **left (Local) pane**, browse to the folder containing your `.apk` file. Select it by clicking once.

```
Local ~/Downloads
📄  MyApp-v2.1.apk   ← click to select
📄  other-file.txt
```

**2. The "Install APK" button appears**

When an `.apk` file is selected in the local pane *and* a device is connected, the **Install APK** button appears in the **Android pane toolbar** (right side):

```
View [As List ▼]  [ Install APK ]  [To Local]  [⊞]
```

> **Note:** The button only appears in the **Android pane toolbar**, not the local pane toolbar. It activates based on what is selected in the **local** pane.

**3. Click Install APK**

A confirmation banner appears at the top of the pane:

```
Install  MyApp-v2.1.apk  to connected device?    [Confirm]  [Cancel]
```

Review the filename, then click **Confirm**.

**4. Wait for installation**

The toolbar shows a brief loading state while `adb install -r` runs on the device. Your phone may show a system dialog asking to confirm the install — accept it on the phone.

When complete, a **toast notification** appears:

```
✓ Installed: MyApp-v2.1.apk
```

---

### What "Install APK" does

rusty-adb runs:

```bash
adb -s <serial> install -r /path/to/app.apk
```

The `-r` flag means **reinstall** — it updates an existing app without uninstalling first, preserving user data. If the app isn't installed yet, it installs fresh.

---

### Troubleshooting installs

| Error | Likely cause | Fix |
|---|---|---|
| `INSTALL_FAILED_UPDATE_INCOMPATIBLE` | New APK has a different signature than the installed version | Uninstall the existing app first |
| `INSTALL_FAILED_OLDER_SDK` | APK requires a newer Android version | Check the app's minimum SDK requirement |
| `Failure [INSTALL_PARSE_FAILED_NO_CERTIFICATES]` | APK is unsigned or corrupt | Get a properly signed APK |
| `Install failed: device unauthorized` | USB debugging authorization was revoked | Re-authorize on the device |

---

## Viewing Installed Apps

The installed app list is accessible through the **Device Details** dialog:

1. Click the **connection status** in the status bar (e.g. `Connected — Pixel 7`).
2. The **Device Details** dialog opens. Click the **Apps** tab.

The Apps tab shows every third-party package installed on the device (equivalent to `adb shell pm list packages -3 -f`), sorted alphabetically by package ID.

Each row shows:
- **Package ID** (e.g. `com.example.myapp`)
- **APK path** on the device

Click any row to select an app. This enables the **Uninstall** button.

> **Tip:** To refresh the list (e.g. after installing a new app), click the **Refresh** button in the Device Details toolbar.

---

## Uninstalling an App

Uninstalling uses a two-step confirmation to prevent accidents.

**Step 1 — Select the app**

In the **Apps tab** of the Device Details dialog, click the app you want to remove. It becomes highlighted.

**Step 2 — Click Uninstall**

A confirmation row appears:

```
Uninstall  com.example.myapp ?    [Confirm]  [Cancel]
```

**Step 3 — Confirm**

Click **Confirm**. rusty-adb runs `adb uninstall <package-id>` in the background. The device may briefly show a system uninstall dialog.

When complete:
- The app disappears from the list (the list refreshes automatically).
- A **toast notification** confirms: `✓ App uninstalled`

If uninstall fails (e.g. the app is a system app that cannot be removed), an **error banner** appears with the reason.

> **Note:** rusty-adb can only uninstall **user-installed** apps. System apps (pre-installed by the manufacturer) are listed but cannot be uninstalled without root access.

---

← [File Transfers](file-transfers.md) · [Documentation Index](README.md) · [Device Details →](device-details.md)
