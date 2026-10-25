"""Extension Decky Loader d'OpenHeadset Control (mode jeu).

Cliente de l'API du service momentum-daemon, par `ctl --json
--service-requis` : jamais de connexion directe au casque, mêmes refus que
les interfaces (casque absent, lecture seule, scène inconnue). Seules les
actions de la liste ci-dessous sont acceptées depuis l'interface.
"""

import asyncio
import json
import os
import shutil

import decky

# Identifiant du Flatpak (v0.13.0 et suivantes).
FLATPAK_ID = "io.github.asfax03l.openheadset-control"
# Une scène peut prendre plusieurs secondes (relecture de chaque réglage).
TIMEOUT_S = 60


def _home():
    return getattr(decky, "DECKY_USER_HOME", None) or os.path.expanduser("~")


def _ctl():
    """Commande ctl de l'installation présente : archive, paquet, Flatpak."""
    home = _home()
    for path in (os.path.join(home, ".local", "bin", "ctl"), "/usr/bin/ctl"):
        if os.access(path, os.X_OK):
            return [path]
    flatpak = shutil.which("flatpak")
    if flatpak and os.path.isdir(os.path.join(home, ".local", "share", "flatpak", "app", FLATPAK_ID)):
        return [flatpak, "run", "--command=ctl", FLATPAK_ID]
    return None


def _environment():
    """Bus de session de l'utilisateur du mode jeu (service activable)."""
    env = dict(os.environ)
    home = _home()
    uid = os.stat(home).st_uid
    runtime = f"/run/user/{uid}"
    env["HOME"] = home
    env.setdefault("XDG_RUNTIME_DIR", runtime)
    env.setdefault("DBUS_SESSION_BUS_ADDRESS", f"unix:path={runtime}/bus")
    return env


def _arguments(action, value):
    """Arguments de ctl pour une action de l'interface ; None : refusée."""
    if action == "mode" and value in ("arret", "adaptatif"):
        return ["mode", value]
    if action == "niveau" and isinstance(value, int) and 0 <= value <= 100:
        return ["niveau", str(value)]
    if action in ("anti-vent", "spatial") and value in ("on", "off"):
        return [action, value]
    if action == "scene" and isinstance(value, str) and value.strip():
        return ["scene", value]
    return None


async def _run(arguments):
    ctl = _ctl()
    if ctl is None:
        return {"ok": False, "erreur": "OpenHeadset Control n'est pas installé (ctl introuvable)"}
    process = await asyncio.create_subprocess_exec(
        *ctl, "--json", "--service-requis", *arguments,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
        env=_environment(),
    )
    try:
        out, _ = await asyncio.wait_for(process.communicate(), TIMEOUT_S)
    except asyncio.TimeoutError:
        process.kill()
        return {"ok": False, "erreur": "pas de réponse du service"}
    try:
        value = json.loads(out.decode("utf-8") or "{}")
    except ValueError:
        return {"ok": False, "erreur": "réponse illisible de ctl"}
    if not isinstance(value, dict):
        return {"ok": False, "erreur": "réponse illisible de ctl"}
    value.setdefault("ok", process.returncode == 0)
    return value


class Plugin:
    async def etat(self):
        """État relu sur le casque (format de `ctl --json etat`)."""
        return await _run(["etat"])

    async def scenes(self):
        """Noms des scènes (`ctl --json scenes`)."""
        value = await _run(["scenes"])
        names = value.get("scenes")
        return names if isinstance(names, list) else []

    async def appliquer(self, action, value):
        """Envoie une action de la liste au service ; résultat de ctl."""
        arguments = _arguments(action, value)
        if arguments is None:
            return {"ok": False, "erreur": "action refusée par l'extension"}
        result = await _run(arguments)
        if not result.get("ok"):
            decky.logger.warning("action %s refusée : %s", action, result.get("erreur"))
        return result

    async def _main(self):
        decky.logger.info("OpenHeadset Control : ctl = %s", _ctl())

    async def _unload(self):
        pass
