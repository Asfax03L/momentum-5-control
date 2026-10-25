// Panneau du mode jeu : batterie, réduction de bruit et scènes du casque.
// Tout passe par le backend (main.py), client de l'API du service : une
// valeur n'est affichée qu'une fois relue sur le casque, un refus est
// montré tel que le service l'a donné.
import {
  ButtonItem,
  PanelSection,
  PanelSectionRow,
  ToggleField,
  staticClasses,
} from "@decky/ui";
import { callable, definePlugin, toaster } from "@decky/api";
import { useEffect, useState } from "react";

interface Etat {
  ok?: boolean;
  erreur?: string;
  connecte?: boolean;
  batterie?: number;
  bruit?: { mode: string; niveau: number | null; anti_vent: boolean };
  audio_spatial?: boolean | null;
}

interface Resultat {
  ok: boolean;
  erreur?: string;
  resultat?: string | null;
}

const lireEtat = callable<[], Etat>("etat");
const lireScenes = callable<[], string[]>("scenes");
const appliquer = callable<[action: string, valeur: string | number], Resultat>("appliquer");

// Relecture pendant que le panneau est ouvert.
const RELECTURE_MS = 5000;

// Langue de Steam : français si elle commence par « fr », anglais sinon.
const FRANCAIS = (navigator.language || "").toLowerCase().startsWith("fr");
const ANGLAIS: Record<string, string> = {
  "inconnu": "unknown",
  "désactivée": "off",
  "adaptative": "adaptive",
  "maximale": "maximum",
  "transparence maximale": "maximum transparency",
  "personnalisée (curseur {} %)": "custom (slider {} %)",
  "refusé": "refused",
  "Lecture du casque…": "Reading the headphones…",
  "Casque non connecté": "Headphones not connected",
  "Casque": "Headphones",
  "Batterie : {} %": "Battery: {} %",
  "Réduction de bruit : {}": "Noise cancelling: {}",
  "Réduction de bruit": "Noise cancelling",
  "ANC maximale": "Maximum ANC",
  "Adaptative": "Adaptive",
  "Transparence": "Transparency",
  "Désactivée": "Off",
  "Anti-vent": "Anti-wind",
  "Scènes": "Scenes",
  "Scène « {} »": "Scene “{}”",
  // Messages du backend (main.py).
  "OpenHeadset Control n'est pas installé (ctl introuvable)": "OpenHeadset Control is not installed (ctl not found)",
  "pas de réponse du service": "no answer from the service",
  "réponse illisible de ctl": "unreadable reply from ctl",
  "action refusée par l'extension": "action refused by the extension",
};

// Texte dans la langue de Steam ; chaque « {} » reçoit la valeur suivante.
function t(texte: string, ...valeurs: (string | number)[]): string {
  let modele = FRANCAIS ? texte : (ANGLAIS[texte] ?? texte);
  for (const v of valeurs) modele = modele.replace("{}", String(v));
  return modele;
}

function modeTexte(etat: Etat): string {
  const bruit = etat.bruit;
  if (!bruit) return t("inconnu");
  if (bruit.mode === "arret") return t("désactivée");
  if (bruit.mode === "adaptatif") return t("adaptative");
  if (bruit.niveau === 0) return t("maximale");
  if (bruit.niveau === 100) return t("transparence maximale");
  return t("personnalisée (curseur {} %)", bruit.niveau ?? 0);
}

function Contenu() {
  const [etat, setEtat] = useState<Etat | null>(null);
  const [scenes, setScenes] = useState<string[]>([]);
  const [envoi, setEnvoi] = useState(false);

  const relire = async () => {
    setEtat(await lireEtat());
  };

  useEffect(() => {
    relire();
    lireScenes().then(setScenes);
    const minuteur = setInterval(relire, RELECTURE_MS);
    return () => clearInterval(minuteur);
  }, []);

  const envoyer = async (action: string, valeur: string | number, libelle: string) => {
    setEnvoi(true);
    try {
      const r = await appliquer(action, valeur);
      if (!r.ok) {
        // Refus du service (via ctl) : transmis tel quel.
        toaster.toast({ title: libelle, body: t(r.erreur ?? r.resultat ?? "refusé") });
      }
    } finally {
      setEnvoi(false);
      await relire();
    }
  };

  if (etat === null) {
    return (
      <PanelSection>
        <PanelSectionRow>{t("Lecture du casque…")}</PanelSectionRow>
      </PanelSection>
    );
  }
  if (!etat.connecte) {
    return (
      <PanelSection>
        <PanelSectionRow>{t(etat.erreur ?? "Casque non connecté")}</PanelSectionRow>
      </PanelSection>
    );
  }
  return (
    <>
      <PanelSection title={t("Casque")}>
        <PanelSectionRow>{t("Batterie : {} %", etat.batterie ?? "?")}</PanelSectionRow>
        <PanelSectionRow>{t("Réduction de bruit : {}", modeTexte(etat))}</PanelSectionRow>
      </PanelSection>
      <PanelSection title={t("Réduction de bruit")}>
        <PanelSectionRow>
          <ButtonItem layout="below" disabled={envoi} onClick={() => envoyer("niveau", 0, t("ANC maximale"))}>
            {t("ANC maximale")}
          </ButtonItem>
        </PanelSectionRow>
        <PanelSectionRow>
          <ButtonItem layout="below" disabled={envoi} onClick={() => envoyer("mode", "adaptatif", t("Adaptative"))}>
            {t("Adaptative")}
          </ButtonItem>
        </PanelSectionRow>
        <PanelSectionRow>
          <ButtonItem layout="below" disabled={envoi} onClick={() => envoyer("niveau", 100, t("Transparence"))}>
            {t("Transparence")}
          </ButtonItem>
        </PanelSectionRow>
        <PanelSectionRow>
          <ButtonItem layout="below" disabled={envoi} onClick={() => envoyer("mode", "arret", t("Désactivée"))}>
            {t("Désactivée")}
          </ButtonItem>
        </PanelSectionRow>
        <PanelSectionRow>
          <ToggleField
            label={t("Anti-vent")}
            checked={etat.bruit?.anti_vent ?? false}
            disabled={envoi}
            onChange={(on: boolean) => envoyer("anti-vent", on ? "on" : "off", t("Anti-vent"))}
          />
        </PanelSectionRow>
      </PanelSection>
      {scenes.length > 0 && (
        <PanelSection title={t("Scènes")}>
          {scenes.map((nom) => (
            <PanelSectionRow key={nom}>
              <ButtonItem layout="below" disabled={envoi} onClick={() => envoyer("scene", nom, t("Scène « {} »", nom))}>
                {nom}
              </ButtonItem>
            </PanelSectionRow>
          ))}
        </PanelSection>
      )}
    </>
  );
}

function Icone() {
  // Casque stylisé, sans dépendance d'icônes.
  return (
    <svg viewBox="0 0 24 24" width="1em" height="1em" fill="currentColor" aria-hidden="true">
      <path d="M12 3a8 8 0 0 0-8 8v6a3 3 0 0 0 3 3h1v-7H6v-2a6 6 0 0 1 12 0v2h-2v7h1a3 3 0 0 0 3-3v-6a8 8 0 0 0-8-8z" />
    </svg>
  );
}

export default definePlugin(() => ({
  name: "OpenHeadset Control",
  titleView: <div className={staticClasses.Title}>OpenHeadset Control</div>,
  content: <Contenu />,
  icon: <Icone />,
}));
