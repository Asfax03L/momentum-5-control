# User guide

*[Version française](GUIDE.md) · [FAQ](FAQ.en.md)*

OpenHeadset Control adjusts the Sennheiser **MOMENTUM 5** headphones from a
Windows 11 or Linux (KDE Plasma) PC, without the mobile app. **Unofficial**
project, not affiliated with Sennheiser or Sonova. Only settings validated
on the headphones are changed; everything else is read-only. No firmware
update, no factory reset.

## 1. Install

### Windows 10/11

1. Download the installer
   `openheadset-control-vX.Y.Z-windows-x86_64-installateur.exe` from the
   [Releases](https://github.com/Asfax03L/openheadset-control/releases) page.
2. Run it: per-user installation, no administrator rights. The executables
   are not signed: SmartScreen may ask for confirmation (“More info” > “Run
   anyway”).
3. The interface needs **Windows App Runtime 2**. If it is missing, the
   installer stops without changing anything and gives the runtime's
   address (or `winget install Microsoft.WindowsAppRuntime.2`).

Uninstall: Settings > Apps. Checking a download's authenticity:
[VERIFIER.md](VERIFIER.md) (French).

### Linux (KDE Plasma, Bazzite)

- **Flatpak** (recommended on Bazzite and read-only systems):
  `flatpak install --user openheadset-control-vX.Y.Z-x86_64.flatpak`.
- **Debian, Ubuntu**: `sudo apt install ./openheadset-control_X.Y.Z-1_amd64.deb`.
- **Fedora**: `sudo dnf install ./openheadset-control-X.Y.Z-1.x86_64.rpm`.
- **Arch**: `PKGBUILD` attached to the releases.
- **Script**: `./installer-linux.sh` from the `.tar.gz` archive, no `sudo`.

Do not install the Flatpak and a package at the same time: only one service
can run. Details, Plasma widget and game mode: [LINUX.md](LINUX.md)
(French).

## 2. First launch

On first launch, an **assistant** goes through:

1. **Bluetooth**: it must be on (“Open Bluetooth settings” button).
2. **Pairing**: done in the system Bluetooth settings (pairing mode: see the
   headphones' manual). The application pairs nothing itself; it shows when
   the headphones are detected.
3. **Windows App Runtime** (Windows): present, since the window is showing.
4. **Multipoint**: the headphones can stay connected to two devices at once
   (two PCs, or a PC and a phone).

The assistant can be reopened from **About** > “First-launch assistant”.

## 3. Pages

- **Home**: battery, noise cancelling modes (ANC, adaptive, transparency,
  off), ANC ↔ transparency slider, anti-wind, spatial audio and head
  tracking, devices paired with the headphones (Connect / Disconnect),
  Multipoint (turning it off asks for confirmation: the other device is
  disconnected).
- **Equalizer**: audio mode (equalizer, Speech Clarity, custom profile),
  8 bands from −6 to +6 dB, bass boost, presets, curve export and import,
  A/B comparison.
- **Scenes**: several settings at once (“Transport”: maximum ANC and
  anti-wind, for example). Built-in editor, or “Save the current state as a
  scene”.
- **Battery**: curve over 24 h, 7 or 30 days, measured on this PC (CSV
  export, confirmed clearing).
- **Connection and audio**: link state and system audio output, read-only.
- **Settings**: Smart Pause, auto-answer, comfort calls, auto-off, battery
  protection, prompts, headphones name, touch controls; light or dark
  theme; **language**; quick panel content; keyboard shortcuts; settings
  backup and restore.
- **About**: model, firmware, diagnostic, bug report, this guide.

Nothing is shown as applied before the headphones answer: every setting is
read back, and a refusal shows in a banner. A setting the headphones did not
report appears unavailable.

### Personal presets saved before 1.3

In Equalizer (Windows or KDE), older presets are preserved. Before applying
them, use “Associate older presets with this equalizer” to bind them to the
documented M5 frequencies and current gain limits. This does not change the
headset and keeps the usual backup file. Incompatible curves remain saved.
Associate presets before reusing a scene that depends on them.

While moving a slider, the requested value is distinguished from the latest
readback. Reconnecting cancels older gestures; adjust the control again
after the connection returns.

### Guided EQ assistant (1.4, Windows and KDE)

In Equalizer, select graphic mode, then **Start comparisons**. Choose Less,
Reference or More for bass, midrange and treble, waiting for each readback
before continuing. Each group changes by at most ±1 dB around its step
reference; repeated previews do not accumulate changes. Finally compare
the initial curve and the result, then save under a new name, or **Cancel
and restore the initial curve**.

The assistant plays no sound and creates no hearing profile. An initial
copy is kept in `assistant-eq/` under the application data folder. A
disconnection interrupts the session without automatic restoration.
Finish or restore before quitting; a forced close leaves the last applied
curve active. [Workflow, recovery and limits (French)](LOT3-ASSISTANT-EQ.md).

## 4. Icon and shortcuts

- **Left click** on the icon near the clock: quick panel (battery, noise
  modes, slider, scenes).
- **Right click**: full menu (modes, scenes, equalizer, Quit).
- **Global shortcuts**: Ctrl+Alt+N next noise mode, Ctrl+Alt+P custom
  profile or equalizer, Ctrl+Alt+M open the window; a scene can have its
  own. On KDE, they can be changed in System Settings > Shortcuts.
- Closing the window only hides it; “Quit” is in the icon menu.

On KDE, there are also the Plasma widget and KRunner search (“anc off”,
“scene Transport”); on Windows, the taskbar jump list (scenes); in game
mode, the Decky extension.

## 5. Language

Settings > **Language**: system language, Français or English, applied the
next time the interface starts. Without a choice, the interface follows the
system language (French if it is French, English otherwise). The `ctl`
command line and the diagnostic text stay in French.

## 6. Accessibility

- Everything works with the **keyboard**: Tab and Shift+Tab move between
  controls, Space or Enter activates, arrows adjust sliders and lists.
- Every control has a **name read by screen readers** (Narrator, NVDA on
  Windows; Orca on Linux); page and section titles are announced as
  headings.
- Colours follow the system theme (light, dark, high contrast on Windows);
  secondary texts keep a contrast of at least 4.5:1.

An accessibility problem is a bug: please report it (section 8).

## 7. Back up your settings

Settings > backup: export scenes, presets, shortcuts, panel and appearance
to a JSON file, then review it before importing it on another PC. The
battery history is only included on request. No hearing profile and no
device identifier is exported.

## 8. Report a problem

**About** > “Report a problem…”:

1. the **redacted diagnostic** is shown (versions, model, firmware, which
   settings were read, recent incidents; no name, no Bluetooth address, no
   serial number, no personal folder, no hearing profile);
2. “Open the form on GitHub” opens the “Bug” form in your browser, already
   filled in with this text;
3. review it, describe the steps, then submit on GitHub (GitHub account
   required). Nothing is sent before that.

The form is in French; answers in English are welcome. A question rather
than a bug:
[Discussions](https://github.com/Asfax03L/openheadset-control/discussions).

## 9. New versions

Settings > **New version notice** (off by default): at startup, the
interface reads the latest release on GitHub and points out a newer
version, with a “View the release” button. Only your IP address and the
application name reach GitHub; nothing is downloaded or installed
automatically. The Flatpak is updated by Flatpak. What each version
guarantees: [version policy](VERSIONS.md) (French).

## 10. Where your data lives

Everything stays on the PC, in the data folder:

- Windows: `%LOCALAPPDATA%\momentum-ctl-win`;
- Linux: `~/.local/share/momentum-ctl-win`;
- Flatpak: `~/.var/app/io.github.asfax03l.openheadset-control/`.

It holds `scenes.txt`, `prereglages.txt`, `raccourcis.txt`, `panneau.txt`,
`apparence.txt`, `langue.txt`, `mises-a-jour.txt`, `notifications.txt`,
`batterie.csv`, `journal.csv` and the service's technical log.

## 11. Battery life, event log, notifications, automatic scenes

- **Measured battery life** (Battery page): from this PC's measurements,
  battery points lost per hour and how long a full charge lasts at that
  rate, then the same per noise cancelling mode. At least one hour of
  continuous discharge is needed; these are measurements, never a battery
  “health”.
- **Event log** (Battery page): connections, losses, noise cancelling
  modes, refusals from the headphones and applied scenes, kept 30 days on
  this PC (`journal.csv`), without device names or addresses. Can be
  exported and cleared, never sent.
- **Notifications of your choice** (Settings): low battery, connection lost
  and restored, and the new **headphones on without sound** (no playback
  reported for 30 minutes, off by default).
- **Automatic scenes** (scene editor, “Automatic” list): when the
  headphones connect, or every day at a given time (the headphones must
  then be connected). One scene per trigger; only validated commands are
  sent, and each application is logged. In `scenes.txt`:
  `auto = connexion` or `auto = 08:30`.
- **Export for Equalizer APO** (Equalizer page): the headphones' curve in
  Equalizer APO's GraphicEQ format (Windows), in Documents, to reproduce it
  on the PC. Approximate match: the headphones' filter shapes are not
  documented; bass boost is not reproduced.
