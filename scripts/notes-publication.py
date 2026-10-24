#!/usr/bin/env python3
"""Notes détaillées d'une publication GitHub, écrites sur la sortie standard.

Usage : notes-publication.py <étiquette> <dossier des fichiers publiés>

Les nouveautés viennent de la section « ## vX.Y.Z » de CHANGELOG.md. Elle
doit contenir « ### En bref » (ce qui change, pour qui, en langage simple)
et « ### À faire après la mise à jour » (actions concrètes, ou « Rien de
particulier » avec la raison), mises en tête ; sinon le script échoue et
rien n'est publié. Les consignes
d'installation et de mise à jour ne citent que les fichiers réellement
présents dans le dossier, avec leur nom exact.
"""

from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parent.parent
REPO = "https://github.com/Asfax03L/openheadset-control"
TODO = "À faire après la mise à jour"
BRIEF = "En bref"
FIRST_PUBLIC = (0, 8, 0)


def version_tuple(tag):
    return tuple(int(x) for x in re.findall(r"\d+", tag)[:3])


def changelog_section(tag):
    """Corps de la section de `tag` et étiquette de la version précédente."""
    text = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    heads = list(re.finditer(r"^## (v\d+\.\d+\.\d+)\b.*$", text, re.M))
    for i, head in enumerate(heads):
        if head.group(1) == tag:
            end = heads[i + 1].start() if i + 1 < len(heads) else len(text)
            previous = heads[i + 1].group(1) if i + 1 < len(heads) else None
            return text[head.end():end].strip(), previous
    sys.exit(f"CHANGELOG.md : aucune section « ## {tag} » ; l'écrire avant de publier.")


def split_sections(body):
    """Sépare « En bref » et « À faire après la mise à jour » du reste."""
    parts = re.split(r"^(### .*)$", body, flags=re.M)
    intro, rest, special = parts[0], [], {}
    for title, content in zip(parts[1::2], parts[2::2]):
        name = title[4:].strip()
        if name in (BRIEF, TODO):
            special[name] = content.strip()
        else:
            rest.append(f"{title}\n{content.rstrip()}\n")
    return intro.strip(), "\n".join(rest).strip(), special


def find(files, pattern):
    return next((f for f in files if re.search(pattern, f)), None)


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    tag, folder = sys.argv[1], Path(sys.argv[2])
    version = version_tuple(tag)
    # Installateurs préparés et récupérables, widget, désinstallation Linux :
    # depuis la v0.9.0.
    v09 = version >= (0, 9, 0)
    files = sorted(p.name for p in folder.iterdir() if not p.name.endswith(".sha256"))
    zip_ = find(files, r"-windows-x86_64\.zip$")
    tar = find(files, r"-linux-x86_64\.tar\.gz$")
    flatpak = find(files, r"\.flatpak$")
    deb = find(files, r"\.deb$")
    rpm = find(files, r"\.rpm$")
    sbom = find(files, r"sbom.*\.tar\.gz$")
    known = {zip_, tar, flatpak, deb, rpm, sbom}
    others = [f for f in files if f not in known]
    tar_dir = tar[: -len(".tar.gz")] if tar else None
    zip_dir = zip_[: -len(".zip")] if zip_ else None

    body, previous = changelog_section(tag)
    intro, news, special = split_sections(body)
    missing = [s for s in (BRIEF, TODO) if not special.get(s)]
    if missing:
        sys.exit(f"CHANGELOG.md, section {tag} : sous-section(s) « ### "
                 + " », « ### ".join(missing) + " » à écrire avant de publier "
                 "(explication pour l'utilisateur ; « Rien de particulier » "
                 "avec la raison si aucune action n'est nécessaire).")
    brief, todo = special[BRIEF], special[TODO]
    out = []
    w = out.append

    w("> **Projet non officiel**, sans lien avec Sennheiser ni Sonova. "
      "Exécutables non signés : Windows SmartScreen peut demander une "
      "confirmation (« Informations complémentaires » > « Exécuter quand même »).")
    w("")
    w(f"## {BRIEF}")
    w("")
    w(brief)
    w("")
    w("Détail complet plus bas, dans « Nouveautés ».")
    w("")
    w(f"## ⚠️ {TODO}")
    w("")
    w(todo)
    w("")

    w("## Quel fichier télécharger")
    w("")
    w("| Votre système | Fichier | Contenu |")
    w("|---|---|---|")
    if zip_:
        w(f"| Windows 10 ou 11 (x64) | `{zip_}` | interface WinUI 3, service, `ctl`, `probe`, installateur |")
    if flatpak:
        w(f"| Bazzite, SteamOS, Linux en lecture seule, ou toute distribution avec Flatpak | `{flatpak}` | client KDE, service, `ctl` |")
    if deb:
        w(f"| Debian 13, Ubuntu 26.04 | `{deb}` | client KDE, service, `ctl`, widget Plasma, manuel |")
    if rpm:
        w(f"| Fedora 43 et suivantes (pas Bazzite) | `{rpm}` | client KDE, service, `ctl`, widget Plasma, manuel |")
    if tar:
        widget = ", widget Plasma" if v09 else ""
        w(f"| Autre Linux avec KDE Plasma 6 | `{tar}` | client KDE, service, `ctl`{widget}, installateur sans sudo |")
    if sbom:
        w(f"| (vérification) | `{sbom}` | inventaire des dépendances (SBOM CycloneDX) |")
    for f in others:
        w(f"| (autre) | `{f}` | |")
    w("")
    w("Chaque fichier est accompagné de son empreinte `.sha256` "
      "(voir « Vérifier le téléchargement »). **Une seule méthode "
      "d'installation par PC** : le service porte le même nom dans toutes ; "
      "pour changer de méthode, désinstaller d'abord l'ancienne (commandes "
      "ci-dessous). Les réglages, scènes et historique de la batterie sont "
      "toujours conservés par une mise à jour.")
    w("")

    w("## Installer ou mettre à jour")
    w("")
    w("La même procédure sert pour une première installation et pour une "
      "mise à jour : la version en place est remplacée, vos réglages restent.")
    w("")
    if zip_:
        w("### Windows")
        w("")
        w("Prérequis : Bluetooth, casque appairé et connecté au PC "
          "(Paramètres > Bluetooth et appareils), "
          "[Windows App Runtime 2](https://aka.ms/windowsappsdk/2.0/latest/windowsappruntimeinstall-x64.exe) "
          + ("(l'installateur prévient s'il manque et ne change rien)."
             if v09 else "(l'installateur prévient s'il manque)."))
        w("")
        w(f"1. Télécharger `{zip_}`, puis clic droit > « Extraire tout ».")
        w(f"2. Dans le dossier extrait `{zip_dir}`, clic droit sur `installer.ps1` > "
          "« Exécuter avec PowerShell », ou dans un terminal ouvert dans ce dossier :")
        w("   ```powershell")
        w("   powershell -ExecutionPolicy Bypass -File .\\installer.ps1")
        w("   ```")
        w("3. L'installateur arrête la version en place, copie la nouvelle dans "
          "`%LOCALAPPDATA%\\Programs\\momentum-ctl-win`, puis relance l'interface "
          "(icône près de l'horloge). Sans droits administrateur.")
        if v09:
            w("4. Tout est vérifié avant de toucher à la version en place ; en cas "
              "d'échec, l'ancienne version est remise (copie gardée dans "
              "`momentum-ctl-win.ancien`).")
        w("")
        w("Réglages : `%LOCALAPPDATA%\\momentum-ctl-win` (conservés). "
          "Désinstaller : `powershell -ExecutionPolicy Bypass -File .\\installer.ps1 -Desinstaller`.")
        w("")
    if flatpak:
        w("### Flatpak (recommandé sur Bazzite)")
        w("")
        w("1. Quitter OpenHeadset Control (menu de l'icône > Quitter), puis arrêter "
          "l'ancien service s'il tourne :")
        w("   ```sh")
        w("   flatpak kill org.openheadset.Control")
        w("   ```")
        w("2. Première installation :")
        w("   ```sh")
        w(f"   flatpak install --user ./{flatpak}")
        w("   ```")
        w("   Mise à jour d'une version déjà installée :")
        w("   ```sh")
        w(f"   flatpak install --user --reinstall ./{flatpak}")
        w("   ```")
        w("3. Relancer « OpenHeadset Control » depuis le menu des applications, "
          "ou `flatpak run org.openheadset.Control`.")
        w("")
        w("Réglages : `~/.var/app/org.openheadset.Control/` (conservés par "
          "`--reinstall`). `ctl` : `flatpak run --command=ctl "
          "org.openheadset.Control etat`. Désinstaller : "
          "`flatpak uninstall --user org.openheadset.Control`.")
        if tar:
            w("")
            w("Widget Plasma avec le Flatpak : il n'est pas dans le Flatpak. "
              f"L'installer ou le mettre à jour depuis l'archive Linux `{tar}` :")
            w("```sh")
            w(f"tar -xzf {tar}")
            w(f"kpackagetool6 --type Plasma/Applet --upgrade {tar_dir}/plasma/org.openheadset.control.plasmoid \\")
            w(f"  || kpackagetool6 --type Plasma/Applet --install {tar_dir}/plasma/org.openheadset.control.plasmoid")
            w("```")
        w("")
    if deb or rpm:
        w("### Paquets Debian, Ubuntu et Fedora")
        w("")
        w("Pas sur Bazzite (système en lecture seule : utiliser le Flatpak). "
          "Si OpenHeadset Control a été installé avant par l'archive, le "
          "désinstaller d'abord depuis le dossier de cette archive : "
          "`./installer-linux.sh --desinstaller` ; s'il a été installé en "
          "Flatpak : `flatpak uninstall --user org.openheadset.Control`. "
          "Les réglages sont gardés.")
        w("")
        if deb:
            w("Debian 13 / Ubuntu 26.04 (installation et mise à jour) :")
            w("```sh")
            w(f"sudo apt install ./{deb}")
            w("```")
        if rpm:
            w("Fedora 43 et suivantes (installation et mise à jour) :")
            w("```sh")
            w(f"sudo dnf install ./{rpm}")
            w("```")
        w("Puis, dans votre session (sans sudo), relire les unités et redémarrer "
          "le service s'il tournait :")
        w("```sh")
        w("systemctl --user daemon-reload")
        w("systemctl --user try-restart momentum-daemon.service")
        w("```")
        w("Quitter le client KDE (menu de l'icône > Quitter) et le relancer depuis "
          "le menu. Aucun script du paquet ne démarre de service ni ne touche au "
          "dossier personnel. Réglages : `~/.local/share/momentum-ctl-win`. "
          "Désinstaller : `sudo apt purge openheadset-control` ou "
          "`sudo dnf remove openheadset-control`.")
        w("")
    if tar:
        w("### Autre Linux (archive, sans sudo)")
        w("")
        w("Prérequis : KDE Plasma 6 (Qt 6, Kirigami), BlueZ, D-Bus.")
        w("```sh")
        w(f"tar -xzf {tar}")
        w(f"cd {tar_dir}")
        w("./installer-linux.sh")
        w("```")
        if v09:
            w("Le script vérifie tous les fichiers avant de changer quoi que ce soit, "
              "installe dans `~/.local` (binaires gardés en `*.ancien` et remis en "
              "cas d'échec), redémarre le service s'il tournait et met à jour le "
              "widget Plasma.")
        else:
            w("Le script installe dans `~/.local` et redémarre le service s'il tournait.")
        w("Quitter ensuite le client KDE (menu de l'icône > Quitter) et le "
          "relancer. Réglages : `~/.local/share/momentum-ctl-win`.")
        if v09:
            w("Désinstaller : `./installer-linux.sh --desinstaller` (réglages gardés).")
        w("")
    if v09 and (tar or deb or rpm):
        w("Widget Plasma : la nouvelle version s'affiche après une nouvelle "
          "ouverture de session, ou tout de suite avec "
          "`systemctl --user restart plasma-plasmashell`.")
        w("")

    w("## Vérifier le téléchargement")
    w("")
    if version >= (0, 10, 0):
        w("Linux, dans le dossier du téléchargement (fichier et `.sha256` côte à côte) :")
        w("```sh")
        w("sha256sum -c <fichier>.sha256")
        w("```")
        w("Le résultat doit être « OK ».")
    else:
        w("Le fichier `.sha256` contient l'empreinte seule. Linux :")
        w("```sh")
        w('test "$(sha256sum <fichier> | cut -d" " -f1)" = "$(cat <fichier>.sha256)" && echo OK')
        w("```")
    w("")
    w("Windows (PowerShell) : comparer les deux valeurs, qui doivent être identiques :")
    w("```powershell")
    w("(Get-FileHash .\\<fichier> -Algorithm SHA256).Hash")
    w("(Get-Content .\\<fichier>.sha256).Split(' ')[0]")
    w("```")
    # Attestations et SBOM : publications faites depuis l'ajout de la provenance.
    if sbom and (ROOT / "docs" / "VERIFIER.md").exists():
        w("")
        w(f"Attestations de provenance et SBOM : voir [docs/VERIFIER.md]({REPO}/blob/{tag}/docs/VERIFIER.md).")
    w("")

    w("## Après la mise à jour : contrôler")
    w("")
    w(f"- `ctl --version` doit afficher `ctl {tag[1:]}` (Flatpak : "
      "`flatpak run --command=ctl org.openheadset.Control --version` ; Windows : "
      "`& \"$env:LOCALAPPDATA\\Programs\\momentum-ctl-win\\ctl.exe\" --version`) ; "
      "la page « À propos » de l'interface donne aussi la version.")
    w("- `ctl etat` affiche la batterie et le mode de bruit. Si le casque "
      "n'est pas trouvé : l'allumer et vérifier qu'il est connecté en "
      "Bluetooth à ce PC (le téléphone peut rester connecté en multipoint)"
      + (" ; `ctl reconnecter` relance la connexion Bluetooth." if version >= (0, 10, 0) else "."))
    if v09:
        w("- Écritures seulement sur un MOMENTUM 5 (`M5AEBT`) en micrologiciel "
          "6.21.5 ; sinon tout reste en lecture seule et le bandeau en donne la raison.")
    else:
        w("- Écritures seulement sur un MOMENTUM 5 (`M5AEBT`) ; un autre modèle "
          "reste en lecture seule.")
    w("")

    w("## Nouveautés")
    w("")
    if intro:
        w(intro)
        w("")
    w(news or "(voir CHANGELOG.md)")
    w("")
    w("## Signaler un problème")
    w("")
    w(f"[Ouvrir une issue]({REPO}/issues/new/choose) (modèles « Bogue » et "
      "« Compatibilité ») avec la version, le système et le diagnostic expurgé "
      "(page « À propos » > « Copier le diagnostic »). Ne jamais joindre "
      "d'adresse Bluetooth, de numéro de série ni de profil auditif.")
    w("")
    # La v0.8.0 est la première étiquette du dépôt public : pas de
    # comparaison avec une version antérieure.
    if previous and version_tuple(previous) >= FIRST_PUBLIC:
        w(f"**Tous les changements** : [{previous}...{tag}]({REPO}/compare/{previous}...{tag}) · "
          f"[CHANGELOG.md]({REPO}/blob/{tag}/CHANGELOG.md)")
    else:
        w(f"**Journal complet** : [CHANGELOG.md]({REPO}/blob/{tag}/CHANGELOG.md)")
    sys.stdout.write("\n".join(out).rstrip() + "\n")


if __name__ == "__main__":
    main()
