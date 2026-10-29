#!/usr/bin/env python3
"""Vérifie le renommage avant l'exception locale du linter Flatpak.

L'identifiant actuel et les données des installations sont conservés.
Aucun autre échec du linter n'est autorisé par ce contrôle.
"""
import json
from urllib.error import HTTPError
from urllib.parse import urlparse
from urllib.request import HTTPRedirectHandler, Request, build_opener, urlopen

OLD = "https://github.com/asfax03l/openheadset-control"
NEW = "https://github.com/Asfax03L/momentum-5-control"
REPOSITORY_ID = 1428646656


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def canonical(url):
    parsed = urlparse(url)
    if parsed.scheme != "https" or parsed.netloc.lower() != "github.com" or parsed.query or parsed.fragment:
        return False
    return parsed.path.rstrip("/").lower() == "/asfax03l/momentum-5-control"


def main():
    request = Request(OLD, headers={"User-Agent": "OpenHeadset-Control-CI"})
    try:
        with build_opener(NoRedirect).open(request, timeout=30):
            raise SystemExit("L'ancienne URL ne redirige plus : réexaminer l'exception locale.")
    except HTTPError as error:
        if error.code not in (301, 308) or not canonical(error.headers.get("Location", "")):
            raise SystemExit("Redirection inattendue : exception du linter refusée.") from error
    api = Request("https://api.github.com/repos/Asfax03L/momentum-5-control", headers={"User-Agent": "OpenHeadset-Control-CI"})
    with urlopen(api, timeout=30) as response:
        repo = json.load(response)
    if repo.get("id") != REPOSITORY_ID or repo.get("full_name", "").lower() != "asfax03l/momentum-5-control" or not canonical(repo.get("html_url", "")):
        raise SystemExit("Dépôt canonique inattendu.")
    print(f"Identité Flatpak conservée : {OLD} redirige vers {NEW} (dépôt {repo['id']}).")


if __name__ == "__main__":
    main()
