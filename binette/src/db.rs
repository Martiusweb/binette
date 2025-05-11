use std::path::{Path, PathBuf};
use std::result;

use rusqlite::Connection;
use thiserror::Error;

/// The App ID is a user-defined i32 value set in a SQLite database headers.
/// We expect that if the application_id value is this one, it's a valid file
/// created by (or compatible with) our app.
const SQLITE_APP_ID: i32 = 0x27011990;

/// The user version is stored is an opaque 32 bits field in a SQLite database
/// headers. Used to track the schema version.
///
/// Currently, the schema version is 1. We will bump the schema version for each
/// new minor+ release of the app.
const SQLITE_USER_VERSION: u32 = 1;

/// Current database schema.
/// TODO use "user version"?
const SCHEMA: &str = concat!(
    "
CREATE TABLE app_metadata (
    id INTEGER PRIMARY KEY,
    version TEXT NOT NULL,
    upgraded_from TEXT
);

INSERT INTO app_metadata (version, upgrade_from) VALUES ('",
    env!("CARGO_PKG_VERSION"),
    "', NULL);
"
);

/// Error type for errors returned by this module.
#[derive(Error, Debug)]
pub enum DbError {
    /// Failed to open the database.
    /// TODO: better error handler for classic cases like io error, etc
    /// would open work when opening from dir that doesn't exist?
    #[error("failed to open file {path}")]
    OpenFailed {
        path: PathBuf,
        cause: rusqlite::Error,
    },
    #[error("failed to write changes")]
    WriteFailed { cause: rusqlite::Error },

    #[error("failed to read the file, its format is invalid")]
    InvalidFileError { cause: rusqlite::Error },
}

/// Specialized result type for the db module.
type Result<T> = result::Result<T, DbError>;

/// Represents the result of a version comparison.
pub enum VersionMatch {
    Unknown,
    Older,
    Current,
    Newer,
}

/// Represents an on-disk file containing the library database.
struct AppFile {
    /// Version of the file format (database schema). This is always set if the
    /// file is initialized.
    /// TODO should require set with from file?
    version: Option<String>,
    upgrade_from: Option<String>,
    connection: Connection,
}

/// The version of the file is expected to match the current version and no
/// backward compatibility is enforced. Reads may fail or return no data, while
/// writes may fail, lose data or even corrupt the file.
///
/// After opening, the file version should be checked and upgraded if needed.
impl AppFile {
    /// Tries creating an AppFile for the given path.
    ///
    /// If the file is new or empty, it will be initialized with the current
    /// format version.
    ///
    /// Returns an error if the file can't be read or is invalid (not a SQLite
    /// database or not with a matching AppId).
    fn try_open<P: AsRef<Path>>(path: P) -> Result<Self> {
        // open or create file (return OpenFailed)
        // TODO verify what happens if we try to open a file which isn't a sqlite db
        // If no db initialized (empty file) (query sqlite_master to check if there's something), init
        // Else check app id and return invalidformat if doesn't match
        // return initalized file
        todo!()
    }

    /// Returns whether the version of the file matches the current version, is
    /// older or newer.
    fn compare_version() -> VersionMatch {
        // read version and compare with current (metadata)
        VersionMatch::Unknown
    }

    /// Upgrades the file format to the current version (in place).
    ///
    /// Perform a manual copy of the file before the upgrade to avoid data loss.
    fn upgrade() -> Result<()> {
        // read version and proceed with upgrades as needed
        // if version == xxx
        // if version == xxx+1
        // if version == xxx+2
        // etc
        todo!()
    }

    // TODO everything after to be removed

    /// Creates an AppFile from a file stored on disk.
    ///
    /// If the file is new or empty, it will be initialized with the current
    /// format version.
    ///
    /// If the file is a valid file (form any version), it will be opened.
    ///
    /// Else, an error is returned.
    fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        match Connection::open(path.as_ref()) {
            // TODO
            /*
             * open file (raise OpenFailed)
             * check if file is new (query sqlite_master)
             *   if new: intialize new db (write error)
                 if not : check schema version is correct (raise InvalidFormat if not)
            */
            Ok(connection) => Ok(Self {
                version: None,
                upgrade_from: None,
                connection,
            }),
            Err(e) => Err(DbError::OpenFailed {
                path: path.as_ref().to_path_buf(),
                cause: e,
            }),
        }
    }

    /// Opens a file and populates the minimal metadata.
    ///
    /// Returns an error if the file can't be loaded, but not if the file
    /// verison is unsupported.
    fn load(&mut self) -> Result<()> {
        // TODO first check if file is initialized.

        // Metadata for the current version is always the last row of the
        // metadata table.
        self.connection
            .query_row(
                "SELECT version, upgraded_from FROM app_metadata ORDER BY id DESC LIMIT 1",
                (),
                |row| {
                    self.version = row.get(0)?;
                    self.upgrade_from = row.get(1)?;
                    Ok(())
                },
            )
            .or_else(|e| Err(DbError::InvalidFileError { cause: e }))
    }

    /// Creates and initializes a new database file.
    fn create(&mut self) -> Result<()> {
        match self.connection.execute(SCHEMA, ()) {
            Ok(_) => Ok(()),
            Err(e) => Err(DbError::WriteFailed { cause: e }),
        }

        // TODO commit?

        // TODO self.load()
    }
}

#[cfg(test)]
mod tests {
    use super::AppFile;

    #[test]
    fn test_create_new_file() {
        AppFile::from_file("/tmp/db").unwrap().create().unwrap();
    }
}
