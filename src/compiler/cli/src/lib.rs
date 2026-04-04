use std::{fs::File, io, path::Path};

pub fn open_file_or_create(path: &Path) -> io::Result<File> {
    match File::create(path) {
        Ok(f) => Ok(f),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            File::create(path)
        }
        Err(e) => Err(e),
    }
}
