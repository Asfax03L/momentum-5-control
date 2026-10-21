// Partagé par les `build.rs` des exécutables Windows (`include!`) : intègre
// le logo d'OpenHeadset Control à l'exécutable (Explorateur, menu Démarrer,
// barre des tâches, Alt+Tab). Demande `embed-resource` en dépendance de
// compilation.

/// Tailles du logo rangées dans le fichier .ico (PNG de `assets/logo/png`).
const SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];

/// Assemble `openheadset.ico` à partir des PNG du logo et l'intègre comme
/// première icône de l'exécutable (celle que Windows affiche).
fn embed_logo() {
    // Les exécutables sont à deux niveaux sous la racine (`frontends/…`).
    let manifest =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let pngs = manifest.join("../../assets/logo/png");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let ico = out.join("openheadset.ico");
    let images: Vec<(u32, Vec<u8>)> = SIZES
        .iter()
        .map(|&size| {
            let path = pngs.join(format!("{size}.png"));
            println!("cargo:rerun-if-changed={}", path.display());
            let data = std::fs::read(&path)
                .unwrap_or_else(|e| panic!("lecture de {} : {e}", path.display()));
            (size, data)
        })
        .collect();
    std::fs::write(&ico, ico_bytes(&images)).expect("écriture de openheadset.ico");

    let rc = out.join("logo.rc");
    std::fs::write(&rc, format!("1 ICON \"{}\"\n", rc_path(&ico))).expect("écriture de logo.rc");
    embed_resource::compile(&rc, embed_resource::NONE)
        .manifest_optional()
        .expect("intégration du logo");
}

/// Fichier .ico dont chaque image est un PNG (accepté depuis Windows Vista).
fn ico_bytes(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let count = u16::try_from(images.len()).expect("trop d'images");
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes()); // réservé
    out.extend_from_slice(&1u16.to_le_bytes()); // type : icône
    out.extend_from_slice(&count.to_le_bytes());
    let mut offset = 6 + 16 * images.len();
    for (size, data) in images {
        // 256 s'écrit 0 dans l'entrée.
        let side = if *size >= 256 { 0 } else { *size as u8 };
        out.extend_from_slice(&[side, side, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // plans
        out.extend_from_slice(&32u16.to_le_bytes()); // bits par pixel
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += data.len();
    }
    for (_, data) in images {
        out.extend_from_slice(data);
    }
    out
}

/// Chemin écrit dans le script de ressources : barres obliques, que `rc` et
/// `windres` acceptent toutes deux.
fn rc_path(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}
