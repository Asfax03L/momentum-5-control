"""Essais du backend de l'extension Decky, sans Decky ni casque :
un faux module `decky` et un faux `ctl` qui note ses arguments.

    python3 -m unittest discover -s decky/tests
"""

import asyncio
import importlib
import json
import os
import stat
import sys
import tempfile
import types
import unittest

ICI = os.path.dirname(os.path.abspath(__file__))
FAUX_CTL = """#!/bin/sh
printf '%s\\n' "$*" >> "$HOME/arguments.txt"
case "$*" in
    *" etat") echo '{"version":1,"connecte":true,"batterie":80,"bruit":{"mode":"adaptatif","niveau":null,"anti_vent":true}}' ;;
    *" scenes") echo '{"version":1,"scenes":["Transport","Travail"],"avertissements":[]}' ;;
    *"scene Inconnue") echo '{"version":1,"ok":false,"erreur":"scène inconnue : Inconnue"}'; exit 1 ;;
    *) echo '{"version":1,"ok":true,"resultat":"appliqué et relu"}' ;;
esac
"""


class Backend(unittest.TestCase):
    def setUp(self):
        self.home = tempfile.mkdtemp()
        os.makedirs(os.path.join(self.home, ".local", "bin"))
        ctl = os.path.join(self.home, ".local", "bin", "ctl")
        with open(ctl, "w", encoding="utf-8") as f:
            f.write(FAUX_CTL)
        os.chmod(ctl, stat.S_IRWXU)
        avertissements = []
        logger = types.SimpleNamespace(
            info=lambda *a: None, warning=lambda *a: avertissements.append(a)
        )
        sys.modules["decky"] = types.SimpleNamespace(DECKY_USER_HOME=self.home, logger=logger)
        sys.path.insert(0, os.path.dirname(ICI))
        self.main = importlib.import_module("main")
        importlib.reload(self.main)
        self.plugin = self.main.Plugin()
        self.avertissements = avertissements
        os.environ["HOME"] = self.home

    def arguments(self):
        chemin = os.path.join(self.home, "arguments.txt")
        if not os.path.exists(chemin):
            return []
        with open(chemin, encoding="utf-8") as f:
            return f.read().splitlines()

    def run_async(self, coroutine):
        return asyncio.run(coroutine)

    def test_etat_et_scenes_par_le_service(self):
        etat = self.run_async(self.plugin.etat())
        self.assertEqual(etat["batterie"], 80)
        self.assertEqual(self.run_async(self.plugin.scenes()), ["Transport", "Travail"])
        # Toujours --json --service-requis : jamais de connexion directe.
        self.assertEqual(self.arguments(), ["--json --service-requis etat", "--json --service-requis scenes"])

    def test_actions_de_la_liste_seulement(self):
        for action, valeur, attendu in [
            ("mode", "arret", "mode arret"),
            ("niveau", 100, "niveau 100"),
            ("anti-vent", "off", "anti-vent off"),
            ("scene", "Transport", "scene Transport"),
        ]:
            self.assertTrue(self.run_async(self.plugin.appliquer(action, valeur))["ok"])
            self.assertEqual(self.arguments()[-1], f"--json --service-requis {attendu}")
        envoyes = len(self.arguments())
        for action, valeur in [
            ("mode", "personnalise"),
            ("niveau", 101),
            ("niveau", "50"),
            ("nom", "Casque"),
            ("scene", " "),
            ("reinitialiser", None),
        ]:
            r = self.run_async(self.plugin.appliquer(action, valeur))
            self.assertFalse(r["ok"], (action, valeur))
        self.assertEqual(len(self.arguments()), envoyes)

    def test_refus_du_service_rendu_tel_quel(self):
        r = self.run_async(self.plugin.appliquer("scene", "Inconnue"))
        self.assertFalse(r["ok"])
        self.assertEqual(r["erreur"], "scène inconnue : Inconnue")
        self.assertEqual(len(self.avertissements), 1)

    def test_sans_installation(self):
        os.remove(os.path.join(self.home, ".local", "bin", "ctl"))
        self.main.shutil.which = lambda _name: None
        if os.access("/usr/bin/ctl", os.X_OK):
            self.skipTest("ctl installé sur ce système")
        r = self.run_async(self.plugin.etat())
        self.assertFalse(r["ok"])
        self.assertIn("ctl introuvable", r["erreur"])


if __name__ == "__main__":
    unittest.main()
