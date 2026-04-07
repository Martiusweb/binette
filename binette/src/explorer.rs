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
/// Symbolic links are not traversed.
#[derive(Debug)]
pub struct LibraryIterator {
    root: PathBuf,
    dir_it: fs::ReadDir,
    to_visit: Vec<PathBuf>,
}

impl LibraryIterator {
    pub fn try_read<P: Into<PathBuf>>(path: P) -> Result<Self> {
        let root = path.into();
        let dir_it = fs::read_dir(&root)?;
        let to_visit = vec![];

        Ok(Self {
            root,
            dir_it,
            to_visit,
        })
    }

    fn is_music<P: AsRef<Path>>(&self, path: P) -> bool {
        // We currently use a very crude filter based on file extensions.
        // We should probably make this customizable or smarter.
        if let Some(extension) = path.as_ref().extension() {
            if extension == "mp3" || extension == "flac" {
                return true;
            }
        }

        false
    }

    fn visit(&mut self, entry: io::Result<DirEntry>) -> Result<Option<PathBuf>> {
        let entry = entry?;

        if entry.file_type()?.is_dir() {
            self.to_visit.push(entry.path());
            Ok(None)
        } else {
            let path = entry.path();
            if self.is_music(path.as_path()) {
                Ok(Some(path.strip_prefix(&self.root).unwrap().into()))
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
    use assert_fs::prelude::*;
    use googletest::prelude::*;

    use super::{ExplorerError, LibraryIterator};

    #[test]
    fn test_try_read_directory_doesnt_exist() {
        assert_that!(
            LibraryIterator::try_read("/non/existing/library/directory"),
            err(pat!(ExplorerError::IoError { cause: anything() }))
        );
    }

    #[cfg(target_family = "unix")]
    #[test]
    fn test_subdirectory_is_not_readable() -> Result<()> {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let library = assert_fs::TempDir::new()?;
        let sub_dir = library.path().join("unreadable");
        fs::create_dir(&sub_dir)?;
        fs::set_permissions(&sub_dir, fs::Permissions::from_mode(0o000))?;

        let it = LibraryIterator::try_read(library.path())?;
        let res: Vec<_> = it.collect();

        // reset permissions for cleanup.
        fs::set_permissions(&sub_dir, fs::Permissions::from_mode(0o775))?;

        verify_that!(
            res,
            elements_are![err(pat!(ExplorerError::IoError { cause: anything() }))]
        )
    }

    // Test iterator over file system.
    #[test]
    fn test_explore_library() -> Result<()> {
        let tmp_dir = assert_fs::TempDir::new()?;
        tmp_dir.child("incomming/new_track.mp3").touch()?;
        tmp_dir.child("incomming/not_music.txt").touch()?;

        let album = tmp_dir.child("Four Tet/2017 - New Energy/");
        album.child("01 - Alap.mp3").touch()?;
        album.child("02 - Thousand and Seventeen.mp3").touch()?;
        album.child("03 - LA Trance.mp3").touch()?;
        album.child("04 - Tremper.mp3").touch()?;
        album.child("05 - Lush.mp3").touch()?;
        album.child("06 - Scientists.mp3").touch()?;
        album.child("07 - Falls 2.mp3").touch()?;
        album.child("08 - You Are Loved.mp3").touch()?;
        album.child("09 - SW9 9SL.mp3").touch()?;
        album.child("10 - 10 Midi.mp3").touch()?;
        album.child("11 - Memories.mp3").touch()?;
        album.child("12 - Daughter.mp3").touch()?;
        album.child("13 - Gentle Soul.mp3").touch()?;
        album.child("14 - Planet.mp3").touch()?;
        album.child("cover.jpg").touch()?;

        let library = LibraryIterator::try_read(tmp_dir.path())?;

        let all_found: Vec<_> = library.collect::<super::Result<Vec<_>>>()?;

        verify_that!(all_found, {
            "incomming/new_track.mp3",
            "Four Tet/2017 - New Energy/01 - Alap.mp3",
            "Four Tet/2017 - New Energy/02 - Thousand and Seventeen.mp3",
            "Four Tet/2017 - New Energy/03 - LA Trance.mp3",
            "Four Tet/2017 - New Energy/04 - Tremper.mp3",
            "Four Tet/2017 - New Energy/05 - Lush.mp3",
            "Four Tet/2017 - New Energy/06 - Scientists.mp3",
            "Four Tet/2017 - New Energy/07 - Falls 2.mp3",
            "Four Tet/2017 - New Energy/08 - You Are Loved.mp3",
            "Four Tet/2017 - New Energy/09 - SW9 9SL.mp3",
            "Four Tet/2017 - New Energy/10 - 10 Midi.mp3",
            "Four Tet/2017 - New Energy/11 - Memories.mp3",
            "Four Tet/2017 - New Energy/12 - Daughter.mp3",
            "Four Tet/2017 - New Energy/13 - Gentle Soul.mp3",
            "Four Tet/2017 - New Energy/14 - Planet.mp3",
        })
    }
}
