use std::{
    fs::{self, DirEntry},
    io,
    path::{Path, PathBuf},
    result,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExplorerError {
    /// Wraps an IO error.
    #[error("operation failed due to an IO error")]
    IoError {
        #[from]
        cause: std::io::Error,
    },
}

type Result<T> = result::Result<T, ExplorerError>;

/// Iterator visiting the filesystem recursively.
///
/// Supports both Windows and Unix-like file systems.
///
/// Symbolic links are followed, and cycles are not detected.
/// TODO do we follow shortcuts on windows?
pub struct LibraryIterator {
    dir_it: fs::ReadDir,
    to_visit: Vec<PathBuf>,
}

impl LibraryIterator {
    pub fn try_read<P: AsRef<Path>>(path: P) -> Result<Self> {
        Ok(Self {
            dir_it: fs::read_dir(path)?,
            to_visit: vec![],
        })
    }

    fn is_music<P: AsRef<Path>>(&self, path: P) -> bool {
        // TODO filter if we don't want to return this entry.
        true
    }

    fn visit(&mut self, entry: io::Result<DirEntry>) -> Result<Option<PathBuf>> {
        let entry = entry?;

        // TODO we need to prefix the path with the parents
        if entry.file_type()?.is_dir() {
            self.to_visit.push(entry.path());
            Ok(None)
        } else {
            let path = entry.path();
            if self.is_music(path.as_path()) {
                Ok(Some(entry.path()))
            } else {
                Ok(None)
            }
        }
    }
}

impl Iterator for LibraryIterator {
    type Item = Result<PathBuf>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Do we have something to visit in the current directory?
            if let Some(entry) = self.dir_it.next() {
                match self.visit(entry) {
                    Err(e) => return Some(Err(e)),
                    Ok(Some(p)) => return Some(Ok(p)),
                    // We need to explore more.
                    Ok(None) => (),
                };
            } else if let Some(next) = self.to_visit.pop() {
                // We have a child dir to visit.
                match fs::read_dir(next) {
                    Err(e) => return Some(Err(e.into())),
                    // We have another directory to visit.
                    Ok(iterator) => {
                        self.dir_it = iterator;
                    }
                }
            } else {
                // Nothing left in any of our queues
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    fn create_fs() -> io::Result<()> {
        todo!()
    }

    // TODO tests
    // * root doesnt exist, try_read fails
    // * root exists, a subdirectory is not readable (how to simulate this?)
}
