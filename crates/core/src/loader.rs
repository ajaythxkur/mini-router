use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use thiserror::Error;

use crate::connector::Connector;

#[derive(Debug, Error)]
pub enum LoadError {
    #[error("failed to readd {}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid connector file {}", path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("duplicate connector id `{0}`")]
    DuplicateId(String),
}

fn load_file(path: &Path) -> Result<Connector, LoadError> {
    let text = fs::read_to_string(path).map_err(|source| LoadError::Io {
        path: path.to_path_buf(),
        source,
    })?;

    toml::from_str(&text).map_err(|source| LoadError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

pub fn load_connectors(dir: &Path) -> Result<Vec<Connector>, LoadError> {
    let mut connectors = Vec::new();
    let mut seen = HashSet::new();

    // Iterating a directory
    for entry in fs::read_dir(dir).map_err(|source| LoadError::Io {
        path: dir.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| LoadError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        for file_entry in fs::read_dir(&path).map_err(|source| LoadError::Io {
            path: path.to_path_buf(),
            source,
        })? {
            let file_entry = file_entry.map_err(|source| LoadError::Io {
                path: path.to_path_buf(),
                source,
            })?;

            let path = file_entry.path();

            // Checking the extension
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }

            let connector = load_file(&path)?;

            // insert returns fals when the value was already there
            if !seen.insert(connector.id.clone()) {
                return Err(LoadError::DuplicateId(connector.id));
            }

            connectors.push(connector);
        }
    }

    // read_dir order is not guaranteed (differs between macOS and Linux)
    connectors.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(connectors)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"
        id = "dup#same"
        display_name = "T"
        description = "T"
        categories = []
        [request]
        method = "GET"
        base_url = "https://example.com"
        path = "/"
        [pricing]
        kind = "per_call"
        amount = "0"
        [input]
        type = "object"
    "#;

    #[test]
    fn rejects_duplicate_ids() {
        let tmp = tempfile::tempdir().unwrap();
        let provider = tmp.path().join("prov");
        fs::create_dir(&provider).unwrap();
        fs::write(provider.join("a.toml"), VALID).unwrap();
        fs::write(provider.join("b.toml"), VALID).unwrap();

        let err = load_connectors(tmp.path()).unwrap_err();
        assert!(matches!(err, LoadError::DuplicateId(id) if id == "dup#same"));
    }

    #[test]
    fn loads_valid_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let provider = tmp.path().join("prov");
        fs::create_dir(&provider).unwrap();
        fs::write(provider.join("a.toml"), VALID).unwrap();
        let other = VALID.replace("dup#same", "dup#diff");
        fs::write(provider.join("b.toml"), &other).unwrap();

        let connectors = load_connectors(tmp.path()).expect("should load");
        assert_eq!(connectors.len(), 2);
        assert_eq!(connectors[0].id, "dup#diff");
        assert_eq!(connectors[1].id, "dup#same");
    }

    #[test]
    fn reports_bad_file_path() {
        let tmp = tempfile::tempdir().unwrap();
        let provider = tmp.path().join("prov");
        fs::create_dir(&provider).unwrap();
        fs::write(provider.join("broken.toml"), "this is not a = = toml").unwrap();
        let err = load_connectors(tmp.path()).unwrap_err();

        assert!(matches!(err, LoadError::Parse { ref path, .. } if path.ends_with("broken.toml")));
    }
}
