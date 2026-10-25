//! Accessibilité des interfaces QML (client KDE, widget Plasma) : chaque
//! commande porte un nom lu par les lecteurs d'écran. Les curseurs, listes
//! déroulantes et compteurs n'ont pas de texte : `Accessible.name`
//! obligatoire. Les boutons, interrupteurs et cases ont leur `text`.

use std::path::Path;

const FICHIERS: &[&str] = &[
    "frontends/kirigami/qml/Main.qml",
    "plasma/org.openheadset.control.plasmoid/contents/ui/main.qml",
];

/// Commandes sans texte propre : nom accessible explicite.
const SANS_TEXTE: &[&str] = &["Slider", "ComboBox", "SpinBox"];
/// Commandes nommées par leur texte (ou par un nom accessible).
const AVEC_TEXTE: &[&str] = &["Button", "ToolButton", "Switch", "CheckBox", "RadioButton"];

/// Bloc `Type {` … `}` qui commence à `debut` (accolades équilibrées).
fn bloc(texte: &str, debut: usize) -> &str {
    let mut profondeur = 0usize;
    for (i, c) in texte[debut..].char_indices() {
        match c {
            '{' => profondeur += 1,
            '}' => {
                profondeur -= 1;
                if profondeur == 0 {
                    return &texte[debut..=debut + i];
                }
            }
            _ => {}
        }
    }
    &texte[debut..]
}

fn ligne(texte: &str, position: usize) -> usize {
    texte[..position].matches('\n').count() + 1
}

#[test]
fn chaque_commande_qml_a_un_nom_accessible() {
    let racine = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut fautes = Vec::new();
    for fichier in FICHIERS {
        let texte = std::fs::read_to_string(racine.join(fichier)).expect(fichier);
        for (type_, exige) in SANS_TEXTE
            .iter()
            .map(|t| (*t, "Accessible.name"))
            .chain(AVEC_TEXTE.iter().map(|t| (*t, "text")))
            .chain([("TextField", "placeholderText")])
        {
            for prefixe in ["QQC2.", "PlasmaComponents."] {
                let motif = format!("{prefixe}{type_} {{");
                for (debut, _) in texte.match_indices(&motif) {
                    let b = bloc(&texte, debut);
                    let nomme = b.contains("Accessible.name")
                        || b.lines()
                            .any(|l| l.trim_start().starts_with(&format!("{exige}:")))
                        || b.contains(&format!("{{ {exige}:"));
                    if !nomme {
                        fautes.push(format!(
                            "{fichier}:{} : {prefixe}{type_} sans {exige}",
                            ligne(&texte, debut)
                        ));
                    }
                }
            }
        }
    }
    assert!(
        fautes.is_empty(),
        "commandes sans nom accessible :\n{}",
        fautes.join("\n")
    );
}
