# Installation

*[Version française](INSTALLATION.md)*

How to install OpenHeadset Control on Windows or Linux, from a release or from
source. Linux details (Flatpak, packages, service): [LINUX.md](LINUX.md), in
French.

## Before you start

- A PC with Bluetooth, on Windows 10/11 or Linux.
- The headphones **paired and connected** to the PC, in the Bluetooth settings
  of the system. OpenHeadset Control does not pair anything itself.
- Your phone can stay connected to the headphones at the same time: the
  MOMENTUM 5 accepts the PC in Multipoint.

## From a release (no Rust needed)

Download the files from the
[Releases page](https://github.com/Asfax03L/momentum-5-control/releases/latest).
CI publishes them for each `v*` tag, with a SHA-256 checksum, a provenance
attestation and the dependency inventory ([how to verify](VERIFIER.md), in
French). The executables are not code-signed: Windows SmartScreen may ask you
to confirm.

| System | File | How |
|---|---|---|
| Windows 10/11 | `openheadset-control-vX.Y.Z-windows-x86_64-installateur.exe` | Run it. Per-user install, no administrator rights. Uninstall from Settings > Apps |
| Windows, without the installer | `openheadset-control-vX.Y.Z-windows-x86_64.zip` | Extract, then run `installer.ps1` |
| Bazzite and other read-only systems | `openheadset-control-vX.Y.Z-x86_64.flatpak` | `flatpak install --user openheadset-control-vX.Y.Z-x86_64.flatpak` |
| Debian, Ubuntu | `.deb` package | Install with your package manager |
| Fedora | `.rpm` package | Install with your package manager |
| Other Linux | `openheadset-control-vX.Y.Z-linux-x86_64.tar.gz` | Extract, then run `./installer-linux.sh` |

The Windows app needs Windows App Runtime 2. The installer checks it.

## Windows, from source

You need Rust 1.95 or newer (the tested version is pinned in
`rust-toolchain.toml`).

```bash
powershell -ExecutionPolicy Bypass -File scripts\installer.ps1
```

The script installs, without administrator rights, in
`%LOCALAPPDATA%\Programs\momentum-ctl-win`: the **WinUI 3** app
(`momentum-winui.exe`), the service `momentum-daemon.exe` that the app starts
when needed, and `ctl.exe`.

What the script does:

- It takes the executables next to it (release archive) or, without them,
  builds the release version.
- It registers the app at Windows startup (it starts hidden: tray icon only),
  adds “OpenHeadset Control” to the Start menu and starts the app.
- It checks and prepares everything before it stops the installed version. The
  replaced version is kept (`momentum-ctl-win.ancien`) and restored if the new
  app does not start.
- Without Windows App Runtime 2, nothing is changed (`-SansRuntime` installs
  anyway).

To remove everything: `scripts\installer.ps1 -Desinstaller`.

To try the app without installing it:

```bash
cargo run --release --bin momentum-winui
```

A headphones icon appears near the clock (if needed, in the hidden icons, arrow
**^**). With `--page son` (or `scenes`, `batterie`, `audio`, `reglages`,
`apropos`), the window opens on that page. With `--cache`, it starts hidden.
With `--demo`, it shows simulated headphones and sends nothing.

## Linux, from source

You need Rust 1.89 or newer (1.92 for the GNOME app), `dbus` and `pkg-config`;
Qt 6 and Kirigami for the KDE app; GTK 4.14 and libadwaita 1.5 for the GNOME
app. On Bazzite, build inside a distrobox. The package lists are in
[LINUX.md](LINUX.md).

```bash
cargo build --release -p momentum-cli -p momentum-daemon -p momentum-kirigami
```
```bash
sh scripts/installer-linux.sh
```

The script installs for the current user, without `sudo`: binaries in
`~/.local/bin`, the logo in the icon theme, “OpenHeadset Control” entries in
the application menu, and the service with its systemd user unit and its D-Bus
activation. A missing file stops the script before any change. Replaced
binaries are kept (`*.ancien`) and restored if a copy fails.

To remove everything: `sh scripts/installer-linux.sh --desinstaller`. Settings,
scenes and history are kept.

For the GNOME app, also build `-p momentum-gtk` before you run the script.
