use db::AppFile;
use explorer::LibraryIterator;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub mod db;
pub mod explorer;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("database error: {0}")]
    DbError(#[from] db::DbError),
    #[error("explorer error: {0}")]
    ExplorerError(#[from] explorer::ExplorerError),
}

pub type Result<T> = std::result::Result<T, AppError>;

/// A root directory in which Music can be found.
///
/// Roots can be backups or synchronized folders between hard drives, or
/// different music libraries used by different software.
///
/// Tracks are indexed relative to their root.
pub struct Root {
    pub id: i64,
    pub path: PathBuf,
}

/// A music libary is a single global view of all the music files we manage.
pub struct MusicLibrary {
    db: AppFile,
}

impl MusicLibrary {
    /// Creates a new MusicLibrary holding the database connection.
    pub fn new(db: AppFile) -> Self {
        Self { db }
    }

    /// Adds or updates a root directory in the library.
    ///
    /// It explores the directory and stores all found tracks in the database.
    pub fn update_root<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let root_id = self.db.insert_root(path.as_ref())?;

        let iterator = LibraryIterator::try_read(path.as_ref())?;

        for file_path in iterator {
            let file_path = file_path?;
            self.db.insert_file(root_id, &file_path)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::prelude::*;

    #[test]
    fn test_update_root() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let tmp_dir = assert_fs::TempDir::new()?;
        tmp_dir.child("track1.mp3").touch()?;
        tmp_dir.child("folder/track2.flac").touch()?;

        let db_file = tempfile::NamedTempFile::new()?;
        let app_file = AppFile::try_open(db_file.path())?;

        let mut library = MusicLibrary::new(app_file);

        library.update_root(tmp_dir.path()).expect("failed to update root");

        // TODO: Could add a query method to AppFile to verify inserted records.

        Ok(())
    }
}