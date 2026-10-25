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

function modeTexte(etat: Etat): string {
  const bruit = etat.bruit;
  if (!bruit) return "inconnu";
  if (bruit.mode === "arret") return "désactivée";
  if (bruit.mode === "adaptatif") return "adaptative";
  if (bruit.niveau === 0) return "maximale";
  if (bruit.niveau === 100) return "transparence maximale";
  return `personnalisée (curseur ${bruit.niveau} %)`;
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
        toaster.toast({ title: libelle, body: r.erreur ?? r.resultat ?? "refusé" });
      }
    } finally {
      setEnvoi(false);
      await relire();
    }
  };

  if (etat === null) {
    return (
      <PanelSection>
        <PanelSectionRow>Lecture du casque…</PanelSectionRow>
      </PanelSection>
    );
  }
  if (!etat.connecte) {
    return (
      <PanelSection>
        <PanelSectionRow>{etat.erreur ?? "Casque non connecté"}</PanelSectionRow>
      </PanelSection>
    );
  }
  return (
    <>
      <PanelSection title="Casque">
        <PanelSectionRow>Batterie : {etat.batterie} %</PanelSectionRow>
        <PanelSectionRow>Réduction de bruit : {modeTexte(etat)}</PanelSectionRow>
      </PanelSection>
      <PanelSection title="Réduction de bruit">
        <PanelSectionRow>
          <ButtonItem layout="below" disabled={envoi} onClick={() => envoyer("niveau", 0, "ANC maximale")}>
            ANC maximale
          </ButtonItem>
        </PanelSectionRow>
        <PanelSectionRow>
          <ButtonItem layout="below" disabled={envoi} onClick={() => envoyer("mode", "adaptatif", "Adaptative")}>
            Adaptative
          </ButtonItem>
        </PanelSectionRow>
        <PanelSectionRow>
          <ButtonItem layout="below" disabled={envoi} onClick={() => envoyer("niveau", 100, "Transparence")}>
            Transparence
          </ButtonItem>
        </PanelSectionRow>
        <PanelSectionRow>
          <ButtonItem layout="below" disabled={envoi} onClick={() => envoyer("mode", "arret", "Désactivée")}>
            Désactivée
          </ButtonItem>
        </PanelSectionRow>
        <PanelSectionRow>
          <ToggleField
            label="Anti-vent"
            checked={etat.bruit?.anti_vent ?? false}
            disabled={envoi}
            onChange={(on: boolean) => envoyer("anti-vent", on ? "on" : "off", "Anti-vent")}
          />
        </PanelSectionRow>
      </PanelSection>
      {scenes.length > 0 && (
        <PanelSection title="Scènes">
          {scenes.map((nom) => (
            <PanelSectionRow key={nom}>
              <ButtonItem layout="below" disabled={envoi} onClick={() => envoyer("scene", nom, `Scène « ${nom} »`)}>
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
