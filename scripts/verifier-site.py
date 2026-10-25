"""Vérifie le site construit : chaque lien interne mène à une page et à une
ancre qui existent, chaque image a un texte de remplacement, chaque page
déclare sa langue. Usage : python3 scripts/verifier-site.py _site"""

import sys
from html.parser import HTMLParser
from pathlib import Path


class Page(HTMLParser):
    def __init__(self):
        super().__init__()
        self.ids = set()
        self.liens = []
        self.images_sans_alt = 0
        self.lang = None

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if "id" in a:
            self.ids.add(a["id"])
        if tag == "html":
            self.lang = a.get("lang")
        if tag == "a" and "href" in a:
            self.liens.append(a["href"])
        if tag in ("img",) and a.get("alt") is None:
            self.images_sans_alt += 1


def main(dossier):
    racine = Path(dossier)
    pages = {}
    for chemin in sorted(racine.rglob("*.html")):
        p = Page()
        p.feed(chemin.read_text(encoding="utf-8"))
        pages[chemin.resolve()] = p
    fautes = []
    for chemin, p in pages.items():
        nom = chemin.relative_to(racine.resolve())
        if not p.lang:
            fautes.append(f"{nom} : langue absente")
        if p.images_sans_alt:
            fautes.append(f"{nom} : {p.images_sans_alt} image(s) sans alt")
        for lien in p.liens:
            if ":" in lien.split("#")[0] or lien.startswith("mailto:"):
                continue
            fichier, _, ancre = lien.partition("#")
            cible = (chemin.parent / fichier).resolve() if fichier else chemin
            if not cible.exists():
                fautes.append(f"{nom} : lien vers {lien} : fichier absent")
            elif ancre and cible.suffix == ".html":
                if ancre not in pages[cible].ids:
                    fautes.append(f"{nom} : lien vers {lien} : ancre absente")
    for f in fautes:
        print(f)
    print(f"{len(pages)} page(s), {len(fautes)} faute(s)")
    return 1 if fautes or not pages else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1] if len(sys.argv) > 1 else "_site"))
