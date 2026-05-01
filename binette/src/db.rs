use std::borrow::Cow;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::result;

use rusqlite::Connection;
use thiserror::Error;


/// The App ID is a user-defined i32 value set in a SQLite database headers.
/// We expect that if the `application_id` value is this one, it's a valid file
/// created by (or compatible with) our app.
const SQLITE_APP_ID: i32 = 0x2701_1990;

/// The user version is stored is an opaque 32 bits field in a SQLite database
/// headers. Used to track the schema version.
///
/// Currently, the schema version is 1. We will bump the schema version for each
/// new minor+ release of the app.
const SQLITE_USER_VERSION: u32 = 1;

/// The platform the database was created on.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Platform {
    Unknown = 0,
    Unix = 1,
    Windows = 2,
}

impl Platform {
    /// Returns the platform the current process is running on.
    pub fn current() -> Self {
        #[cfg(unix)]
        {
            Platform::Unix
        }
        #[cfg(windows)]
        {
            Platform::Windows
        }
        #[cfg(not(any(unix, windows)))]
        {
            Platform::Unknown
        }
    }
}

impl From<u32> for Platform {
    fn from(p: u32) -> Self {
        match p {
            0 => Platform::Unknown,
            1 => Platform::Unix,
            2 => Platform::Windows,
            _ => Platform::Unknown,
        }
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::Unknown => write!(f, "unknown"),
            Platform::Unix => write!(f, "unix"),
            Platform::Windows => write!(f, "windows"),
        }
    }
}

/// Returns the current database schema.
///
/// A few remarks:
/// * following SQLite recommentations (<https://www.sqlite.org/autoinc.html>),
///   we don't use autoincrement. `id` may reuse previously assigned values.
/// * To support cross platform sharing of the database, we store paths as BLOB
///   and a lossy UTF-8 representation (e.g. what `Path::to_string_lossy` returns)
///   as `path_lossy`. The latter is used for indexing and searching.
fn schema() -> String {
    format!(
        "PRAGMA application_id = {};

        CREATE TABLE app_metadata (
            id INTEGER PRIMARY KEY,
            platform INTEGER NOT NULL,
            version TEXT NOT NULL,
            upgraded_from TEXT
        );

        INSERT INTO app_metadata (platform, version, upgraded_from) VALUES ({}, '{}', NULL);

        CREATE TABLE root (
            id INTEGER PRIMARY KEY,
            path BLOB UNIQUE NOT NULL,
            path_lossy TEXT NOT NULL
        );

        CREATE TABLE file (
            id INTEGER PRIMARY KEY,
            root_id INTEGER NOT NULL REFERENCES root(id) ON DELETE CASCADE,
            path BLOB NOT NULL,
            path_lossy TEXT NOT NULL,
            missing BOOLEAN DEFAULT FALSE,
            track_id INTEGER
        );
        CREATE UNIQUE INDEX file_path_idx ON file(root_id, path);

        PRAGMA user_version = {};",
        SQLITE_APP_ID,
        Platform::current() as u32,
        env!("CARGO_PKG_VERSION"),
        SQLITE_USER_VERSION
    )
}

/// Error type for errors returned by this module.
#[derive(Error, Debug)]
pub enum DbError {
    /// Failed to open the database.
    #[error("failed to open file {path}")]
    OpenFailed {
        path: PathBuf,
        cause: rusqlite::Error,
    },

    #[error("failed to read")]
    ReadFailed { cause: rusqlite::Error },

    #[error("failed to write changes")]
    WriteFailed { cause: rusqlite::Error },

    #[error("failed to read the file, its format is invalid")]
    InvalidFileError,

    #[error("an operation failed: {details} (cause: {cause:?})")]
    RuntimeError {
        details: String,
        cause: Option<rusqlite::Error>,
    },

    #[error("incompatible database platform: {platform}")]
    InvalidPlatformError { platform: Platform },
}

/// Specialized result type for the db module.
type Result<T> = result::Result<T, DbError>;

/// Provide conversion helpers to `db::Result`.
trait IntoResult<T> {
    fn into_open_failed<P: Into<PathBuf>>(self, p: P) -> Result<T>;
    fn into_read_failed(self) -> Result<T>;
    fn into_write_failed(self) -> Result<T>;
    fn into_invalid_or_read_failed(self) -> Result<T>;
    fn into_runtime_error<S: Into<String>>(self, details: S) -> Result<T>;
}

impl<T> IntoResult<T> for rusqlite::Result<T> {
    fn into_open_failed<P: Into<PathBuf>>(self, p: P) -> Result<T> {
        match self {
            Err(e) => Err(DbError::OpenFailed {
                path: p.into(),
                cause: e,
            }),
            Ok(r) => Ok(r),
        }
    }

    fn into_read_failed(self) -> Result<T> {
        match self {
            Err(e) => Err(DbError::ReadFailed { cause: e }),
            Ok(r) => Ok(r),
        }
    }

    fn into_write_failed(self) -> Result<T> {
        match self {
            Err(e) => Err(DbError::WriteFailed { cause: e }),
            Ok(r) => Ok(r),
        }
    }

    fn into_invalid_or_read_failed(self) -> Result<T> {
        match self {
            Err(e) if e.sqlite_error_code() == Some(rusqlite::ffi::ErrorCode::NotADatabase) => {
                Err(DbError::InvalidFileError)
            }
            _ => self.into_read_failed(),
        }
    }

    fn into_runtime_error<S: Into<String>>(self, details: S) -> Result<T> {
        match self {
            Err(e) => Err(DbError::RuntimeError {
                details: details.into(),
                cause: Some(e),
            }),
            Ok(r) => Ok(r),
        }
    }
}

/// Converts Path to BLOB (safe on unix, owned on Windows).
#[cfg(unix)]
fn path_to_bytes(path: &Path) -> Cow<'_, [u8]> {
    use std::os::unix::ffi::OsStrExt;
    Cow::Borrowed(path.as_os_str().as_bytes())
}

#[cfg(windows)]
fn path_to_bytes(path: &Path) -> Cow<'_, [u8]> {
    use std::os::windows::ffi::OsStrExt;
    Cow::Owned(path.as_os_str().encode_wide().flat_map(u16::to_le_bytes).collect())
}

/// Builds a PathBuf from a byte slice.
///
/// We assume that the given bytes are from a valid path for the platform.
#[cfg(unix)]
fn bytes_to_path(b: &[u8]) -> PathBuf {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    PathBuf::from(OsString::from_vec(b.to_vec()))
}

#[cfg(windows)]
fn bytes_to_path(b: &[u8]) -> PathBuf {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStrExt;
    PathBuf::from(OsString::from_wide(
        b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect()
    ))
}

/// Represents an on-disk file containing the library database.
///
/// `AppFile` is currently not `Sync`, but concurrent writes from multiple
/// objects are safe, thanks to SQLite concurrency support. Concurrent writes
/// are blocking, and may timeout.
///
/// Currently, the timeout is set to 5000ms (rusqlite defaults).
#[derive(Debug)]
pub struct AppFile {
    /// A connection to the sqlite database.
    connection: Connection,
}

/// The version of the file is expected to match the current version and no
/// backward compatibility is enforced. When using an older or newer file
/// versionm, any operation may fail or return no data, while writes may fail,
/// lose data or even corrupt the file.
///
/// After opening, the file version should be checked and upgraded if needed.
impl AppFile {
    /// Tries creating an `AppFile` for the given path.
    ///
    /// If the file is new or empty, it will be initialized with the current
    /// format version.
    ///
    /// # Errors
    /// Returns an error if the file can't be read, initialized (write error)
    /// or is invalid (not a SQLite database or not with a matching app id).
    pub fn try_open<P: AsRef<Path>>(path: P) -> Result<Self> {
        static FK_SUPPORT_ERR: &str = "Failed to enable foreign keys with SQLite. \
                                 SQLite version is too old or compiled without support for foreign keys";

        let connection = Connection::open(path.as_ref()).into_open_failed(path.as_ref())?;

        let application_id: i32 = connection
            .query_row(
                "SELECT application_id FROM pragma_application_id()",
                [],
                |r| r.get(0),
            )
            .into_invalid_or_read_failed()?;

        if application_id != 0 && application_id != SQLITE_APP_ID {
            return Err(DbError::InvalidFileError);
        }

        // If there are no tables, this is a new/uninitialized database.
        let initialized = connection
            .prepare("SELECT tbl_name FROM sqlite_schema LIMIT 1")
            .and_then(|mut s| s.exists([]))
            .into_invalid_or_read_failed()?;

        let mut app_file = Self { connection };

        if !initialized {
            let transaction = app_file.connection.transaction().into_write_failed()?;
            transaction
                .execute_batch(schema().as_ref())
                .into_write_failed()?;
            transaction.commit().into_write_failed()?;
        } else {
            // check platform compatibility
            let platform = app_file.get_platform()?;

            if platform != Platform::current() {
                return Err(DbError::InvalidPlatformError { platform });
            }
        }

        app_file.connection
            .execute("PRAGMA foreign_keys = 1;", [])
            .into_runtime_error(FK_SUPPORT_ERR)?;

        match app_file.connection.query_row("SELECT foreign_keys FROM pragma_foreign_keys()", [], |r| {
            r.get::<_, i32>(0)
        }) {
            Err(_) | Ok(0) => Err(DbError::RuntimeError {
                details: FK_SUPPORT_ERR.into(),
                cause: None,
            }),
            Ok(_) => Ok(app_file),
        }
    }

    /// Returns the platform the database was created on.
    ///
    /// # Errors
    /// Returns a read error if the database can't be read.
    pub fn get_platform(&self) -> Result<Platform> {
        let platform: u32 = self
            .connection
            .query_row("SELECT platform FROM app_metadata", [], |r| r.get(0))
            .into_read_failed()?;

        Ok(Platform::from(platform))
    }

    /// Returns whether the version of the file matches the current version
    /// (Equal), is older (Less) or newer (Greater).
    ///
    /// # Errors
    /// Returns a read error if the database can't be read.
    pub fn compare_version(&self) -> Result<Ordering> {
        let user_version: u32 = self
            .connection
            .query_row("SELECT user_version FROM pragma_user_version", [], |r| {
                r.get(0)
            })
            .into_read_failed()?;

        Ok(user_version.cmp(&SQLITE_USER_VERSION))
    }

    /// Upgrades the file format to the current version (in place).
    ///
    /// Perform a manual copy of the file before the upgrade to avoid data loss.
    ///
    /// # Errors
    /// Currently never fails as no upgrade happens.
    #[allow(clippy::unnecessary_wraps, clippy::unused_self, unused_mut)] // for forward compat.
    pub fn upgrade(&mut self) -> Result<()> {
        // Currently a no-op since there is only one version.
        Ok(())
    }

    /// Inserts a new root into the database or returns its ID if it already exists.
    ///
    /// # Errors
    /// Returns a write error if the database write fails.
    /// Returns a read error if we can't read the root ID after insertion.
    pub fn insert_root<P: AsRef<Path>>(&self, path: P) -> Result<i64> {
        let path = path.as_ref();
        let path_bytes = path_to_bytes(path);
        self.connection
            .execute(
                "INSERT INTO root (path, path_lossy) VALUES (?1, ?2) ON CONFLICT(path) DO NOTHING",
                rusqlite::params![path_bytes, path.to_string_lossy()],
            )
            .into_write_failed()?;

        self.connection
            .query_row(
                "SELECT id FROM root WHERE path = ?1",
                rusqlite::params![path_bytes],
                |row| row.get(0),
            )
            .into_read_failed()
    }

    /// Inserts a new file into the database for a given root.
    ///
    /// # Errors
    /// Returns a write error if the database write fails.
    /// Returns a read error if we can't read the file id after insertion.
    pub fn insert_file<P: AsRef<Path>>(&self, root_id: i64, path: P) -> Result<i64> {
        let path = path.as_ref();
        let path_bytes = path_to_bytes(path);
        self.connection
            .execute(
                "INSERT INTO file (root_id, path, path_lossy) VALUES (?1, ?2, ?3) ON CONFLICT(root_id, path) DO NOTHING",
                rusqlite::params![root_id, path_bytes, path.to_string_lossy()],
            )
            .into_write_failed()?;

        self.connection
            .query_row(
                "SELECT id FROM file WHERE root_id = ?1 AND path = ?2",
                rusqlite::params![root_id, path_bytes],
                |row| row.get(0),
            )
            .into_read_failed()
    }
}

#[cfg(test)]
mod tests {
    use std::{cmp::Ordering, io::Write, path::PathBuf};

    use crate::db::SQLITE_USER_VERSION;

    use super::{AppFile, DbError, Platform};
    use googletest::prelude::*;
    use rusqlite::Connection;

    #[test]
    fn test_open_fail_on_invalid_path() {
        assert_that!(
            AppFile::try_open("/tmp/doesnt_exist/test.db"),
            err(pat!(DbError::OpenFailed {
                path: eq(&PathBuf::from("/tmp/doesnt_exist/test.db")),
                cause: anything()
            }))
        );
    }

    #[test]
    fn test_open_fail_on_not_a_db() -> Result<()> {
        let mut not_a_db = tempfile::NamedTempFile::new()?;
        not_a_db.write(b"arbitrary data")?;

        verify_that!(
            AppFile::try_open(not_a_db.path()),
            err(pat!(DbError::InvalidFileError))
        )
    }

    #[test]
    fn test_open_fail_on_incompatible_db() -> Result<()> {
        let incompatible_db = tempfile::NamedTempFile::new()?;
        let incompatible_conn = Connection::open(incompatible_db.path())?;
        let app_id = incompatible_conn.query_row(
            "SELECT application_id FROM pragma_application_id;",
            [],
            |r| r.get::<_, i32>(0),
        )?;

        println!("app id: {}", app_id);

        incompatible_conn.execute("PRAGMA application_id = 123", [])?;

        incompatible_conn
            .close()
            .expect("failed to close db in test");

        verify_that!(
            AppFile::try_open(incompatible_db.path()),
            err(pat!(DbError::InvalidFileError))
        )
    }

    #[test]
    fn test_open_fail_on_incompatible_platform() -> Result<()> {
        let incompatible_db = tempfile::NamedTempFile::new()?;
        // Initialize the database file.
        AppFile::try_open(incompatible_db.path())?;
        let incompatible_conn = Connection::open(incompatible_db.path())?;

        #[cfg(windows)]
        let incompatible_platform = Platform::Unix;
        #[cfg(unix)]
        let incompatible_platform = Platform::Windows;

        incompatible_conn
            .execute("UPDATE app_metadata SET platform = ?1;", [incompatible_platform as u32])?;

        incompatible_conn
            .close()
            .expect("failed to close db in test");

        verify_that!(
            AppFile::try_open(incompatible_db.path()),
            err(pat!(DbError::InvalidPlatformError { platform: eq(&incompatible_platform) }))
        )
    }

    #[test]
    fn test_open_new() -> Result<()> {
        let tmpfile = tempfile::NamedTempFile::new()?;
        let appfile = AppFile::try_open(tmpfile.path())?;
        // The file is initialized.
        verify_eq!(appfile.compare_version()?, Ordering::Equal)?;
        verify_eq!(appfile.get_platform()?, Platform::current())
    }
    #[test]
    fn test_open_existing() -> Result<()> {
        let tmpfile = tempfile::NamedTempFile::new()?;
        // initialize the file.
        AppFile::try_open(tmpfile.path())?;

        // re-open it as existing.
        let appfile = AppFile::try_open(tmpfile.path())?;
        verify_eq!(appfile.compare_version()?, Ordering::Equal)?;
        verify_eq!(appfile.get_platform()?, Platform::current())
    }

    #[test]
    fn test_compare_version() -> Result<()> {
        let file = tempfile::NamedTempFile::new()?;
        // Initializes the file.
        AppFile::try_open(file.path())?;

        {
            let con = rusqlite::Connection::open(file.path())?;
            con.execute(
                // we can't use a placeholder value (query param) in the PRAGMA statement.
                format!("PRAGMA user_version = {};", SQLITE_USER_VERSION - 1).as_str(),
                [],
            )?;
        }
        verify_eq!(
            AppFile::try_open(file.path())?.compare_version()?,
            Ordering::Less
        )?;

        {
            let con = rusqlite::Connection::open(file.path())?;
            con.execute(
                format!("PRAGMA user_version = {};", SQLITE_USER_VERSION + 1).as_str(),
                [],
            )?;
        }
        verify_eq!(
            AppFile::try_open(file.path())?.compare_version()?,
            Ordering::Greater
        )
    }

    #[test]
    fn test_upgrade() -> Result<()> {
        let tmpfile = tempfile::NamedTempFile::new()?;
        // initialize the file.
        let mut appfile = AppFile::try_open(tmpfile.path())?;
        appfile.upgrade()?;

        Ok(())
    }

    #[test]
    fn test_insert_root() -> Result<()> {
        let tmpfile = tempfile::NamedTempFile::new()?;
        let appfile = AppFile::try_open(tmpfile.path())?;

        let root_id = appfile.insert_root("/my/music")?;
        verify_ne!(root_id, 0)?;

        // Inserting the same root should return the same ID
        let same_root_id = appfile.insert_root("/my/music")?;
        verify_eq!(root_id, same_root_id)
    }

    #[test]
    fn test_insert_file() -> Result<()> {
        let tmpfile = tempfile::NamedTempFile::new()?;
        let appfile = AppFile::try_open(tmpfile.path())?;

        let root_id = appfile.insert_root("/my/music")?;

        let file_id = appfile.insert_file(root_id, "track.mp3")?;
        verify_ne!(file_id, 0)?;

        // Inserting the same file should return the same ID
        let same_file_id = appfile.insert_file(root_id, "track.mp3")?;
        verify_eq!(file_id, same_file_id)
    }
}
