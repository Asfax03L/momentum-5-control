# Security policy

**English** · [Français](SECURITY.fr.md)

## Supported versions

| Version | Security fixes |
|---|---|
| 1.x (latest minor version) | ✅ |
| Earlier versions | ❌ (please update) |

## Reporting a vulnerability

**No public issue.** Use GitHub's private reporting: the **Security** tab of
the repository > **Report a vulnerability**
(<https://github.com/Asfax03L/momentum-5-control/security/advisories/new>).

Please give: version, system, steps to reproduce, observed effect. Do not
attach a Bluetooth address, a serial number, a hearing profile or a log at the
`trace` level.

## Target response times

- **Acknowledgement**: within 7 days.
- **Fix** for a serious vulnerability: within 30 days, published as a patch
  version with a GitHub security advisory.

These are goals: the project is maintained by one person, without a contract.
The reporter is credited in the advisory if they wish.

## Scope

The service `momentum-daemon`, the apps, `ctl`, the installers and the packages
published here. The firmware of the headphones is out of scope: report it to
Sennheiser. What the service can do on the system, and what limits it:
[docs/SECURITE.md](docs/SECURITE.md) (in French).
