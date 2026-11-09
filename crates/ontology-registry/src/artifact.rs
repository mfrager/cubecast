//! Content-addressed filesystem artifact store.

use std::fs;
use std::path::{Component, Path, PathBuf};

use ontology_core::error::{Error, Result};
use ontology_core::registry::ArtifactRef;
use sha2::{Digest, Sha256};

/// Artifact storage boundary. S3 can implement this later without changing the
/// registry service.
pub trait ArtifactStore: Send + Sync {
    /// Stores bytes and returns their content reference.
    fn put(&self, bytes: &[u8], filename: Option<&str>) -> Result<ArtifactRef>;
    /// Reads an artifact by SHA-256.
    fn read(&self, sha256: &str) -> Result<Vec<u8>>;
    /// Returns the absolute path for a stored artifact.
    fn path(&self, sha256: &str) -> Result<PathBuf>;
}

/// Filesystem artifact store with atomic writes and traversal checks.
#[derive(Debug, Clone)]
pub struct FilesystemArtifactStore {
    root: PathBuf,
}

impl FilesystemArtifactStore {
    /// Opens or creates the store root.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] for a path containing `..` and [`Error::Io`]
    /// when the directory cannot be created.
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();
        if root
            .components()
            .any(|component| component == Component::ParentDir)
        {
            return Err(Error::Config {
                detail: format!(
                    "artifact directory {} must not contain `..`",
                    root.display()
                ),
            });
        }
        fs::create_dir_all(root.join("sha256"))?;
        fs::create_dir_all(root.join("tmp"))?;
        let root = root.canonicalize().map_err(|error| Error::Io {
            detail: format!("could not canonicalise {}: {error}", root.display()),
        })?;
        let store = Self { root };
        store.cleanup_tmp();
        Ok(store)
    }

    /// Returns the root directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn cleanup_tmp(&self) {
        let tmp = self.root.join("tmp");
        let Ok(entries) = fs::read_dir(tmp) else {
            return;
        };
        for entry in entries.flatten() {
            let _ = fs::remove_file(entry.path());
        }
    }

    fn checked_hash(sha256: &str) -> Result<()> {
        if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(Error::validation(format!(
                "artifact hash {sha256:?} must be 64 hexadecimal characters"
            )));
        }
        Ok(())
    }
}

impl ArtifactStore for FilesystemArtifactStore {
    fn put(&self, bytes: &[u8], filename: Option<&str>) -> Result<ArtifactRef> {
        let digest = Sha256::digest(bytes);
        let mut hash = String::with_capacity(64);
        for byte in digest {
            use std::fmt::Write as _;
            let _ = write!(hash, "{byte:02x}");
        }
        let directory = self.root.join("sha256").join(&hash[..2]);
        fs::create_dir_all(&directory)?;
        let target = directory.join(&hash);
        if !target.exists() {
            let temporary = self
                .root
                .join("tmp")
                .join(format!("{}-{}", ulid::Ulid::new(), hash));
            {
                use std::io::Write as _;
                let mut file = fs::File::create(&temporary)?;
                file.write_all(bytes)?;
                file.sync_all()?;
            }
            match fs::rename(&temporary, &target) {
                Ok(()) => {}
                Err(error) if target.exists() => {
                    let _ = fs::remove_file(&temporary);
                    drop(error);
                }
                Err(error) => {
                    let _ = fs::remove_file(&temporary);
                    return Err(Error::Io {
                        detail: format!("could not publish artifact {hash}: {error}"),
                    });
                }
            }
        }
        Ok(ArtifactRef {
            sha256: hash,
            byte_size: bytes.len() as u64,
            filename: filename.map(ToOwned::to_owned),
        })
    }

    fn read(&self, sha256: &str) -> Result<Vec<u8>> {
        Self::checked_hash(sha256)?;
        let path = self.path(sha256)?;
        fs::read(&path).map_err(|error| Error::Io {
            detail: format!("could not read artifact {sha256}: {error}"),
        })
    }

    fn path(&self, sha256: &str) -> Result<PathBuf> {
        Self::checked_hash(sha256)?;
        let path = self.root.join("sha256").join(&sha256[..2]).join(sha256);
        if !path.starts_with(&self.root) {
            return Err(Error::validation("artifact path escaped the artifact root"));
        }
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_atomically_and_is_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let store = FilesystemArtifactStore::new(directory.path()).unwrap();
        let first = store.put(b"hello", Some("hello.ttl")).unwrap();
        let second = store.put(b"hello", Some("other.ttl")).unwrap();
        assert_eq!(first.sha256, second.sha256);
        assert_eq!(store.read(&first.sha256).unwrap(), b"hello");
    }

    #[test]
    fn rejects_traversal_and_bad_hashes() {
        let directory = tempfile::tempdir().unwrap();
        assert!(FilesystemArtifactStore::new(directory.path().join("../escape")).is_err());
        let store = FilesystemArtifactStore::new(directory.path()).unwrap();
        assert!(store.read("../../etc/passwd").is_err());
    }
}
