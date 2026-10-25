# Sécurité

## Versions maintenues

| Version | Corrections de sécurité |
|---|---|
| 1.x (dernière version mineure) | ✅ |
| Versions antérieures | ❌ (mettre à jour) |

## Signaler une faille

**Pas d'issue publique.** Utiliser le signalement privé de GitHub :
onglet **Security** du dépôt > **Report a vulnerability**
(<https://github.com/Asfax03L/openheadset-control/security/advisories/new>).

Indiquer : version, système, étapes pour reproduire, effet observé. Ne
joindre ni adresse Bluetooth, ni numéro de série, ni profil auditif, ni
journal au niveau `trace`.

## Délais visés

- **Accusé de réception** : sous 7 jours.
- **Correctif** d'une faille sérieuse : sous 30 jours, publié en version
  CORRECTIF avec un avis de sécurité GitHub.

Ce sont des objectifs : le projet est tenu par une seule personne, sans
contrat. L'auteur du signalement est cité dans l'avis s'il le souhaite.

## Périmètre

Le service `momentum-daemon`, les interfaces, `ctl`, les installateurs et
les paquets publiés ici. Le micrologiciel du casque n'en fait pas partie :
le signaler à Sennheiser. Ce que le service peut faire sur le système, et
ce qui le limite : [docs/SECURITE.md](docs/SECURITE.md).

---

*English: please report vulnerabilities privately through the repository's
**Security** tab > **Report a vulnerability**, not in a public issue. We
aim to acknowledge within 7 days and to fix serious issues within 30
days.*
