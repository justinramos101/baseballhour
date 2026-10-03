use std::{
    fs,
    io::{self, Write},
    path::Path,
};

pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
    fs::create_dir_all(directory)?;
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::write_atomic;

    #[test]
    fn overwrites_atomically_and_cleans_up_after_failure() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("value");
        write_atomic(&destination, b"first").unwrap();
        write_atomic(&destination, b"second").unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), b"second");

        let blocked = directory.path().join("blocked");
        std::fs::create_dir(&blocked).unwrap();
        assert!(write_atomic(&blocked, b"nope").is_err());
        let entries: Vec<_> = std::fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(entries.len(), 2);
        assert!(entries.contains(&"value".into()));
        assert!(entries.contains(&"blocked".into()));
    }
}
