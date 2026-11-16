# Compatibility

*[Version française](COMPATIBILITE.md)*

Headphones and firmware versions that were tried, and commands validated on the
MOMENTUM 5. Full protocol notes: [MOMENTUM5.md](MOMENTUM5.md) (in French).

## Tested models and firmware

| Headphones (model as read) | Firmware | Systems tried | Writes |
|---|---|---|---|
| MOMENTUM 5 Wireless (`M5AEBT Black`) | 6.21.5 | Windows 11; Bazzite 44 (KDE Plasma 6, Wayland) | ✅ Validated (table below) |
| MOMENTUM 5, other firmware | — | — | 🔒 Read-only (not verified) |
| Other GAIA headphones (ACCENTUM, MOMENTUM 4…) | — | Never tried | 🔒 Read-only |

Another model or firmware stays read-only (no setting is sent) until each
write has been verified on it. To report one, open a “Compatibilité” issue with
the output of `probe --deep` (see [CONTRIBUTING.md](../CONTRIBUTING.md)).

## Command details

Recorded on an M5 “M5AEBT Black”, firmware 6.21.5, in October 2026. Transport:
Bluetooth Classic RFCOMM, SDP service “GAIA”
(UUID `A2129FF3-081B-4C45-8AFE-469D9C4842EC`). Windows did not reach any GATT
service (which does not prove that none exists).

| Feature | Command | Validated on the M5? |
|---|---|---|
| GAIA RFCOMM connection (alongside the phone) | — | ✅ |
| Read battery, model, firmware, codec | `0603`, `1206`, `1201`, `0800` | ✅ |
| Read the Adaptive / Custom / Off mode | `1A05` + `1A01` | ✅ |
| Read the ANC ↔ transparency slider | `1A03` | ✅ 0 = max ANC, 50 = neutral, 100 = max transparency |
| Read anti-wind, spatial audio | `1A01` (pair 1), `3605` | ✅ |
| Read the equalizer and the bass boost | `1000`, `1002 [band]`, `1009` | ✅ 8 bands, ±6 dB |
| Read Smart Pause, wearing, calls, prompts, auto power-off… | see the protocol notes | ✅ (`probe --deep`) |
| Change the mode | `1A04` + `1A00 [03 x]` | ✅ |
| Set the ANC ↔ transparency slider | `1A02 [0-100]` | ✅ |
| Turn anti-wind on or off | `1A00 [01 x]` | ✅ |
| Turn spatial audio on or off | `3604 [0/1]` | ✅ |
| Notifications (battery, noise, spatial audio, equalizer) | `0495:0007 [feature]` | ✅ |
| Set an equalizer band | `1001 [band, gain]` | ✅ (the curve survives a power-off of the headphones) |
| Turn the bass boost on or off | `1008 [0/1]` | ✅ |
| Speech Clarity (audio mode) | `0803 [00, 02]`; equalizer: `[00, 01]` | ✅ |
| Touch controls | `1606 [1 = off]`, read `1607` | ✅ |
| Battery protection | `060A [0/1]`, read `060B` | ✅ |
| Smart Pause, auto-answer, comfort calls | `080C`, `080A`, `0814` `[0/1]` | ✅ |
| Auto power-off | `0600 [00, seconds u16]` | ✅ never / 15 / 30 / 60 min |
| Prompts (tones / tones and voice) | `0801 [1/2]` | ✅ |
| Prompt volume | `081C [i8]` (−20 to +6), read `081D` | ✅ Validated on the M5 (scale recorded, `ctl volume-invites`) |
| Headphones name | `2801 [name]`, read `2802` | ✅ Validated on the M5 (`ctl nom`, Settings page; ASCII, 16 characters at most, limit recorded on the headphones) |
| Connection management: connect or disconnect a paired device | `140F` / `1410 [address]`, list `140D`/`140E`, this PC `1413` | ✅ Validated on the M5 (`ctl connexions`, Home); removing a pairing is not exposed |
| Head tracking (Dolby Atmos) | `3602 [0/1]`, read `3603` | ✅ Validated on the M5 (`ctl suivi-tete`, spatial audio on) |
| Multipoint | `001D:0E00 [0/1]`, read `0495:1409` (1 or 2 connections) | ✅ Validated on the M5 (`ctl multipoint`; turning it off keeps one device only) |
| Prompt language | read `0807` | 🟡 Read-only: changing the language downloads and installs a voice pack (not done) |
| Wear detection (turning it off) | `0400` ? | ➖ Not captured: it also turns off 3 other features |
| Audio mode priority (high resolution, lossless) | `0405`/`0407` then restart `060F` | 🟡 Read-only: the restart often blocks the phone app (see the protocol notes) |
| Gestures (customization) | — | ➖ Not customizable on the M5 (stated by the app) |
| Find my headphones, Sound Zones | — | ➖ Features of the app (phone location), nothing on the headphones side |
| Sound personalization: selecting the existing profile | `0803 [00 03]` (after `3604 [00]` if Dolby Atmos is on) | ✅ Validated on the M5 (HCI capture + read-back from the PC) |
| Sound personalization: reading and exporting the profile | `2001`, `2003`…`2007` | ✅ Read by `probe --deep`, exported raw by `ctl exporter-profil` (byte for byte identical) |
| Sound personalization: new test, writing the profile, persistence | `2000`…`2006` | 🟡 Captured, not exposed (see the protocol notes) |

Detailed notes (method, frames, start-up sequence of the app, leads):
[MOMENTUM5.md](MOMENTUM5.md), in French.

## Worn / not worn

The physical state “worn / not worn” is carried by the service API, separate
from the sensor switch and from Smart Pause. Reading it and its notifications
were tried on the headphones from the PC, on Windows (27/10/2026) and on Linux
(01/11/2026). The Home page of the WinUI, KDE and GNOME apps shows it (KDE and
GNOME since 1.8). Music pause when the headphones are taken off was not tried.
See the
[origin and contract](MOMENTUM5.md#état-physique-du-port--observation-complémentaire-du-27102026)
(in French).

## Sources

- [PROTOCOL.md from Oein/sennheiser-desktop-client](https://github.com/Oein/sennheiser-desktop-client/blob/main/PROTOCOL.md)
  (analysis of Smart Control, verified on ACCENTUM Wireless).
- Recordings on a real M5: `probe --deep`, cross-check with the app through
  ADB, capture of the Android Bluetooth HCI log (details in the protocol notes).
- No third-party code is copied: everything is reimplemented from documented or
  observed facts.
