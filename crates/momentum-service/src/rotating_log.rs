//! Journal fichier à taille bornée, même quand l'icône tourne des semaines :
//! au-delà de `max` octets, le fichier courant devient l'archive (`.old.log`,
//! qui remplace la précédente) et un fichier neuf est ouvert.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;

pub struct RotatingFile {
    path: PathBuf,
    archive: PathBuf,
    max: u64,
    file: File,
    size: u64,
}

impl RotatingFile {
    /// Ouvre `path` en ajout, après archivage s'il dépasse déjà `max`.
    pub fn open(path: PathBuf, archive: PathBuf, max: u64) -> io::Result<Self> {
        if fs::metadata(&path).is_ok_and(|m| m.len() > max) {
            let _ = fs::rename(&path, &archive);
        }
        let file = File::options().create(true).append(true).open(&path)?;
        let size = file.metadata()?.len();
        Ok(Self {
            path,
            archive,
            max,
            file,
            size,
        })
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.file.flush()?;
        // Le renommage d'un fichier ouvert est permis (Rust ouvre avec
        // FILE_SHARE_DELETE) ; l'archive précédente est remplacée.
        fs::rename(&self.path, &self.archive)?;
        self.file = File::options().create(true).append(true).open(&self.path)?;
        self.size = 0;
        Ok(())
    }
}

impl Write for RotatingFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.size + buf.len() as u64 > self.max && self.size > 0 {
            // Un échec d'archivage ne doit pas faire perdre le journal : on
            // continue dans le fichier courant et on réessaiera plus tard.
            if self.rotate().is_err() {
                self.size = 0;
            }
        }
        let written = self.file.write(buf)?;
        self.size += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_while_running_and_keeps_one_archive() {
        let dir = std::env::temp_dir().join(format!("momentum-log-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let (path, archive) = (dir.join("t.log"), dir.join("t.old.log"));
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&archive);

        let mut log = RotatingFile::open(path.clone(), archive.clone(), 100).unwrap();
        log.write_all(&[b'a'; 80]).unwrap();
        // 80 + 30 > 100 : archivage, puis écriture dans un fichier neuf.
        log.write_all(&[b'b'; 30]).unwrap();
        assert_eq!(fs::read(&archive).unwrap(), [b'a'; 80]);
        assert_eq!(fs::read(&path).unwrap(), [b'b'; 30]);
        log.write_all(&[b'c'; 80]).unwrap();
        // L'archive précédente est remplacée : jamais plus de deux fichiers.
        assert_eq!(fs::read(&archive).unwrap(), [b'b'; 30]);
        assert_eq!(fs::read(&path).unwrap(), [b'c'; 80]);

        drop(log);
        fs::remove_dir_all(&dir).unwrap();
    }
}
