//! Exclusion mutuelle des outils d'essai (proposition R07 de Codex) : un
//! outil d'essai ne parle au casque que si aucun autre client OpenHeadset
//! (service, interfaces, `ctl`) ni aucun autre outil d'essai ne tourne. Le
//! contrôle est fait juste avant d'ouvrir la connexion ; rien n'est arrêté
//! par l'outil, qui refuse seulement de démarrer.

/// Programmes qui peuvent tenir ou utiliser le canal du casque.
const CONCURRENTS: [&str; 8] = [
    "momentum-daemon",
    "momentum-winui",
    "momentum-kirigami",
    "momentum-gtk",
    "ctl",
    "probe",
    "essai_port",
    "essai_sidetone",
];

/// Nom de programme sans extension `.exe`, en minuscules.
fn base(name: &str) -> String {
    let name = name.to_ascii_lowercase();
    name.strip_suffix(".exe").unwrap_or(&name).to_string()
}

/// Concurrents parmi les processus listés (nom, numéro), hors celui-ci.
pub fn concurrents(processes: &[(String, u32)], own_pid: u32) -> Vec<String> {
    processes
        .iter()
        .filter(|(_, pid)| *pid != own_pid)
        .filter(|(name, _)| CONCURRENTS.contains(&base(name).as_str()))
        .map(|(name, pid)| format!("{} (PID {pid})", base(name)))
        .collect()
}

#[cfg(windows)]
fn processes() -> anyhow::Result<Vec<(String, u32)>> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    // SAFETY: instantané des processus fermé en fin de lecture ; la
    // structure est initialisée avec sa taille, comme l'exige l'API.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut list = Vec::new();
        let mut ok = Process32FirstW(snapshot, &mut entry).is_ok();
        while ok {
            let len = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            list.push((
                String::from_utf16_lossy(&entry.szExeFile[..len]),
                entry.th32ProcessID,
            ));
            ok = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
        Ok(list)
    }
}

#[cfg(target_os = "linux")]
fn processes() -> anyhow::Result<Vec<(String, u32)>> {
    let mut list = Vec::new();
    for entry in std::fs::read_dir("/proc")?.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        if let Ok(comm) = std::fs::read_to_string(entry.path().join("comm")) {
            list.push((comm.trim().to_string(), pid));
        }
    }
    Ok(list)
}

/// Refuse de continuer si un concurrent tourne (liste dans l'erreur).
pub fn verifier() -> anyhow::Result<()> {
    let found = concurrents(&processes()?, std::process::id());
    if found.is_empty() {
        Ok(())
    } else {
        anyhow::bail!(
            "programme(s) OpenHeadset actif(s) : {} ; essai refusé sans rien envoyer au casque (les quitter d'abord)",
            found.join(", ")
        )
    }
}

#[cfg(test)]
mod tests_exclusion {
    use super::*;

    #[test]
    fn concurrents_are_found_except_this_process() {
        let list = vec![
            ("momentum-daemon.exe".to_string(), 10),
            ("explorer.exe".to_string(), 11),
            ("essai_port.exe".to_string(), 12),
            ("essai_port".to_string(), 99),
            ("MOMENTUM-WINUI.EXE".to_string(), 13),
            ("ctlx".to_string(), 14),
        ];
        assert_eq!(
            concurrents(&list, 99),
            vec![
                "momentum-daemon (PID 10)",
                "essai_port (PID 12)",
                "momentum-winui (PID 13)"
            ]
        );
        assert!(concurrents(&[("essai_port".to_string(), 7)], 7).is_empty());
    }

    #[test]
    fn listing_processes_works_here() {
        let list = processes().unwrap();
        assert!(list.iter().any(|(_, pid)| *pid == std::process::id()));
    }
}
