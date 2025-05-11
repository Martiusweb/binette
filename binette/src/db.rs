use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::result;

use rusqlite::Connection;
use thiserror::Error;

/// The App ID is a user-defined i32 value set in a SQLite database headers.
/// We expect that if the `application_id` value is this one, it's a valid file
/// created by (or compatible with) our app.
const SQLITE_APP_ID: i32 = 0x27011990;

/// The user version is stored is an opaque 32 bits field in a SQLite database
/// headers. Used to track the schema version.
///
/// Currently, the schema version is 1. We will bump the schema version for each
/// new minor+ release of the app.
const SQLITE_USER_VERSION: u32 = 1;

/// Returns the current database schema.
fn schema() -> String {
    format!(
        "PRAGMA application_id = {};

        CREATE TABLE app_metadata (
            id INTEGER PRIMARY KEY,
            version TEXT NOT NULL,
            upgraded_from TEXT
        );

        INSERT INTO app_metadata (version, upgraded_from) VALUES ('{}', NULL);

        PRAGMA user_version = {};",
        SQLITE_APP_ID,
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
}

/// Specialized result type for the db module.
type Result<T> = result::Result<T, DbError>;

/// Provide conversion helpers to `db::Result`.
trait IntoResult<T> {
    fn into_open_failed<P: Into<PathBuf>>(self, p: P) -> Result<T>;
    fn into_read_failed(self) -> Result<T>;
    fn into_write_failed(self) -> Result<T>;
    fn into_invalid_or_read_failed(self) -> Result<T>;
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
    /// Returns an error if the file can't be read, initialized (write error)
    /// or is invalid (not a SQLite database or not with a matching app id).
    pub fn try_open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut connection = Connection::open(path.as_ref()).into_open_failed(path.as_ref())?;

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

        if !initialized {
            let transaction = connection.transaction().into_write_failed()?;
            transaction
                .execute_batch(schema().as_ref())
                .into_write_failed()?;
            transaction.commit().into_write_failed()?;
        }

        Ok(Self { connection })
    }

    /// Returns whether the version of the file matches the current version
    /// (Equal), is older (Less) or newer (Greater).
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
    #[allow(clippy::unnecessary_wraps, unused_self, unused_mut)] // for forward compat.
    pub fn upgrade(&mut self) -> Result<()> {
        // Currently a no-op since there is only one version.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{cmp::Ordering, io::Write, path::PathBuf};

    use crate::db::SQLITE_USER_VERSION;

    use super::{AppFile, DbError};
    use googletest::prelude::*;
    use rusqlite::Connection;

    #[test]
    fn test_open_fail_on_invalid_path() {
        assert_that!(
            AppFile::try_open("/tmp/doesnt_exist/test.db"),
            err(matches_pattern!(DbError::OpenFailed {
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
    fn test_open_new() -> Result<()> {
        let tmpfile = tempfile::NamedTempFile::new()?;
        let appfile = AppFile::try_open(tmpfile.path())?;
        // The file is initialized.
        verify_eq!(appfile.compare_version()?, Ordering::Equal)
        // TODO do a query to the file (app_metadata).
    }
    #[test]
    fn test_open_existing() -> Result<()> {
        let tmpfile = tempfile::NamedTempFile::new()?;
        // initialize the file.
        AppFile::try_open(tmpfile.path())?;

        // re-open it as existing.
        let appfile = AppFile::try_open(tmpfile.path())?;
        verify_eq!(appfile.compare_version()?, Ordering::Equal)
        // TODO do a query to the file (app_metadata).
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
}
