use fs2::FileExt;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

pub struct LockedState {
    file: File,
    values: HashMap<String, String>,
}

impl LockedState {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("create state directory: {error}"))?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)
            .map_err(|error| format!("open state: {error}"))?;
        file.lock_exclusive()
            .map_err(|error| format!("lock state: {error}"))?;
        let mut content = Vec::new();
        file.read_to_end(&mut content)
            .map_err(|error| format!("read state: {error}"))?;
        let values = if content.is_empty() {
            HashMap::new()
        } else {
            serde_json::from_slice(&content).unwrap_or_default()
        };
        Ok(Self { file, values })
    }

    pub fn values(&self) -> impl Iterator<Item = &String> {
        self.values.values()
    }

    pub fn insert(&mut self, key: String, value: String) {
        self.values.insert(key, value);
    }

    pub fn save(&mut self) -> Result<(), String> {
        self.file
            .seek(SeekFrom::Start(0))
            .and_then(|_| self.file.set_len(0))
            .map_err(|error| format!("truncate state: {error}"))?;
        serde_json::to_writer_pretty(&mut self.file, &self.values)
            .map_err(|error| format!("encode state: {error}"))?;
        self.file
            .write_all(b"\n")
            .and_then(|_| self.file.sync_all())
            .map_err(|error| format!("write state: {error}"))
    }
}
