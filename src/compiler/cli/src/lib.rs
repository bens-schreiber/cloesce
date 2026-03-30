use std::path::Path;

pub fn open_file_or_create(path: &Path) -> std::fs::File {
    if path.exists() {
        return std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path)
            .expect("file to be opened for writing");
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent directories to be created");
    }
    std::fs::File::create(path).expect("file to be created")
}
