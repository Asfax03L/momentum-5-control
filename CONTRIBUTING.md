# Contributing to OpenHeadset Control

**English** · [Français](CONTRIBUTING.fr.md)

Thank you for your interest! The project is **unofficial** and not affiliated
with Sennheiser or Sonova. It drives a real device: caution comes before
features.

## Rules that are not up for discussion

- **Nothing invented.** No command, command identifier or UUID that is not
  documented or observed. Anything unverified is marked “to be validated on the
  hardware”.
- **Read-only by default.** A write is added only after it was captured in the
  official app, then replayed and read back on the headphones. Writes are
  limited to the model and firmware they were verified on (see the
  [compatibility table](docs/COMPATIBILITE.en.md)).
- **Never**: firmware update (DFU), factory reset, scanning of command
  identifiers, developer options, removing a pairing, installing a voice pack.
- **No copied code** from another project or from the official app: everything
  is reimplemented from documented or observed facts, with sources cited.
- **Personal data**: no full Bluetooth address, serial number, personal device
  name, personal folder path or hearing profile in code, logs, screenshots,
  issues or pull requests. The script `scripts/verifier-confidentialite.sh`
  (run by CI) rejects the most common cases.

## Reporting a problem or a headset

Use the issue templates:

- **Bug** (“Bogue”): version, system, app, steps, and the redacted diagnostic
  (About page > “Copy the diagnostic”).
- **Compatibility** (“Compatibilité”): another model or another firmware.
  Attach the output of `probe --deep` (addresses and the serial number are
  hidden by default; read it again before you publish it). The project stays
  read-only on that hardware until a write has been verified on it.
- **Idea** (“Idée”): describe the need. If the feature needs a new write, say
  where the command comes from (capture of the app, documentation).

## Languages

The project is written in French first: code comments, commit messages,
changelog and most documents are in French. Issues, discussions and pull
requests are welcome in English or in French. User-facing text exists in both
languages: a new text in an app needs its English translation in
`crates/momentum-core/src/i18n/en.txt` (a test fails without it).

Bilingual documents come in pairs. At the repository root, English is the
default (`README.md`, `CONTRIBUTING.md`, `SECURITY.md`) and French has the
`.fr.md` suffix. In `docs/`, French is the default and English has the `.en.md`
suffix. Please update both files of a pair.

## Setting up

- **Windows**: stable Rust (GNU or MSVC toolchain). The WinUI 3 app needs
  Windows App Runtime 2.
- **Linux**: stable Rust, `dbus` and `pkg-config`; for the KDE app, Qt 6 and
  Kirigami; for the GNOME app, GTK 4 and libadwaita (see
  [docs/LINUX.md](docs/LINUX.md), in French; on Bazzite, inside a Fedora
  distrobox).

```bash
cargo build
```

The KDE and GNOME apps are not built by default:

```bash
cargo build -p momentum-kirigami -p momentum-gtk
```

To try the apps without headphones: `--demo` (simulated headphones, no service,
no write).

## Before you open a pull request

```bash
cargo fmt --check
```
```bash
cargo clippy --all-targets -- -D warnings
```
```bash
cargo test
```
```bash
sh scripts/verifier-confidentialite.sh
```

- One pull request per topic, with what was verified and how.
- New code is tested when possible: the core (`momentum-core`) is pure Rust and
  can be tested without headphones thanks to `MockTransport`.
- Any new or changed write: exact frames, source (capture), trial on the
  headphones with read-back and return to the original value.
- Documentation up to date: the README files, and `docs/MOMENTUM5.md` for the
  protocol. `CHANGELOG.md` is written by the maintainer at release time:
  describe the changes in the pull request (see `AGENTS.md`).

## Test coverage

CI (job `couverture`) shows per-file coverage in the Summary tab and fails if
the protocol coverage (`protocol/` and `sdp.rs` in `momentum-core`) drops below
80 % of lines. Locally:

```bash
cargo install cargo-llvm-cov --version 0.9.1 --locked
```
```bash
cargo llvm-cov -p momentum-core --summary-only
```

## Fuzzing the decoders

On Linux, install a C++ compiler, `pkg-config`, the D-Bus development files and
Python 3, then, from the repository root:

```sh
rustup toolchain install nightly-2026-10-22 --profile minimal --component rust-src,rustfmt
cargo install cargo-fuzz --version 0.13.2 --locked
sh fuzz/lancer.sh 60
```

The script runs **each** of the four targets for 60 seconds with
AddressSanitizer, inputs limited to 4096 bytes, 10 seconds per case and 1 GiB of
resident memory. `fuzz/` is a separate workspace: neither the stable toolchain
nor `cargo test` runs the fuzzing. The seed corpus is kept as readable
hexadecimal in `fuzz/corpus-initial.json`, with the origin of each excerpt from
the public tests. No personal capture and no hearing-profile data is used.

The targets are `frame_decode`, `frame_stream`, `sdp_element` and `messages`.
CI runs each target for 60 seconds on pull requests, and for 600 seconds weekly
or on demand. Any panic or loop that is found must be fixed in the decoder and
kept as an ordinary unit test. The French version of this page has the details
for a single target.

## Architecture in short

The service `momentum-daemon` is the only program that talks to the
headphones. The apps (`frontends/winui`, `frontends/kirigami`, `frontends/gtk`)
and `ctl` are its clients. Details, in French:
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and
[frontends/README.md](frontends/README.md).

## License

By contributing, you agree that your contribution is published under the dual
license of the project, your choice of [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE).
