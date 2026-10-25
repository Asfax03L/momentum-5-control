# Frequently asked questions

*[Version française](FAQ.md) · [User guide](GUIDE.en.md)*

### Is this an official Sennheiser application?

No. OpenHeadset Control is an independent project, not affiliated with
Sennheiser or Sonova. The protocol was reconstructed from public
documentation and captures of the official app
([MOMENTUM5.md](MOMENTUM5.md), French).

### Which headphones are supported?

The Sennheiser MOMENTUM 5 (and its tested firmware: see the compatibility
table in the [README](../README.md#compatibilité-momentum-5)). Other models
are not supported; the repository's “Compatibilité” form lets you report a
test.

### Can it update the firmware or reset the headphones?

No, and it is not planned: no firmware update, no factory reset, and no
write that has not been validated on the headphones. Use the official app
for that.

### Why is a setting greyed out or “unavailable”?

The interface is built from what the headphones answer. A setting they did
not report, or that is not validated yet, stays read-only or unavailable
rather than being guessed. The diagnostic (About) says which settings were
read.

### Can my phone stay connected at the same time?

Yes, with **Multipoint**: the headphones stay connected to two devices, for
example the PC and the phone. Turning Multipoint off disconnects the other
device (a confirmation is asked).

### And with two PCs?

With Multipoint, the headphones can be connected to two PCs, and
OpenHeadset Control can run on each.

### Windows or Linux does not find the headphones.

1. Check that Bluetooth is on and the headphones are on.
2. Check that they are **paired** in the system Bluetooth settings: the
   application pairs nothing itself.
3. The “Reconnect the headphones” button (Home, icon menu) asks the system
   to reconnect headphones it already knows.
4. When dual-booting Windows + Linux, each system has its own pairing key:
   see [LINUX.md](LINUX.md#dual-boot-windows--bazzite--la-clé-dappairage)
   (French).

### SmartScreen blocks the installer.

The executables are not signed: SmartScreen asks for confirmation (“More
info” > “Run anyway”). A download's authenticity can be checked with its
checksum and provenance attestation ([VERIFIER.md](VERIFIER.md), French).

### “Windows App Runtime 2 not found”.

The WinUI interface depends on it. Install it from
<https://aka.ms/windowsappsdk/2.0/latest/windowsappruntimeinstall-x64.exe>
or with `winget install Microsoft.WindowsAppRuntime.2`, then run the
installer again.

### How do I change the language?

Settings > Language: system language, Français or English, applied at the
next start. For a one-off test, the `OPENHEADSET_LANG=fr` or `en`
environment variable takes priority.

### Does the application send data over the Internet?

No. Two exceptions, always on your initiative: the **new version
notice**, off by default, reads the latest release on GitHub at startup
(GitHub sees your IP address, nothing else is sent); the bug report opens
a GitHub form in your browser, prefilled with the redacted diagnostic you
reviewed, and you are the one who submits it.

### What does the diagnostic contain?

Versions, system, model, firmware, which settings were read, recent
incidents. Never: headphones or device names, Bluetooth address, serial
number, personal folder, hearing profile.

### Where are my settings? How do I move them?

In the data folder (see the [guide](GUIDE.en.md#10-where-your-data-lives)).
Settings > backup exports and imports a JSON file, after a preview of what
will change.

### Does it work on GNOME, on the Steam Deck?

On GNOME, a GTK client exists but is partial (Home page). In game mode
(Steam Deck, Bazzite), the Decky Loader extension shows battery, noise
cancelling, anti-wind and scenes.

### Is it on Flathub, winget, the AUR?

Not yet: the submissions are prepared ([SOUMISSIONS.md](SOUMISSIONS.md),
French). Meanwhile, use the files on the Releases page (installer, Flatpak,
`.deb`, `.rpm`, `PKGBUILD`).

### What does version 1.0 guarantee?

Settings files, the JSON backup, the `ctl` command line and the D-Bus API
stay compatible for the whole 1.x series: see the
[version policy](VERSIONS.md) (French). Vulnerabilities are reported
privately ([SECURITY.md](../SECURITY.md)).

### How do I report a problem or ask a question?

A bug: About > “Report a problem…”. A question, an idea:
[Discussions](https://github.com/Asfax03L/openheadset-control/discussions).
