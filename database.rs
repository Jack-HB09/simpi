use std::error::Error;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::table::Table;

/// A generated identifier for a record.
pub struct ID {
    pub id: Uuid,
}

#[derive(Debug)]
pub enum DatabaseError {
    NoDatabaseOpen,
    NoTableOpen,
    DatabaseAlreadyExists(String),
    DatabaseNotFound(String),
    DatabaseAlreadyOpen(String),
    TableAlreadyExists(String),
    TableNotFound(String),
    TableAlreadyOpen(String),
    InvalidDatabaseName(String),
    InvalidTableName(String),
    InvalidDatabaseSettings(String),
    RecordIdRequired,
    ManualRecordIdNotAllowed,
    AutoIncrementOverflow,
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    Json(serde_json::Error),
    InvalidTableFormat,
    DuplicateRecordId(String),
    RecordNotFound(String),
    InvalidRecordUuid(String),
    InvalidColumnName(String),
    ColumnAlreadyExists(String),
    ColumnNotFound(String),
    InvalidColumnType(String),
    InvalidColumnAction(char),
}

impl fmt::Display for DatabaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDatabaseOpen => write!(formatter, "no database is open"),
            Self::NoTableOpen => write!(formatter, "no table is open"),
            Self::DatabaseAlreadyExists(name) => {
                write!(formatter, "database '{name}' already exists")
            }
            Self::DatabaseNotFound(name) => write!(formatter, "database '{name}' was not found"),
            Self::DatabaseAlreadyOpen(name) => {
                write!(formatter, "database '{name}' is open; close it first")
            }
            Self::TableAlreadyExists(name) => write!(formatter, "table '{name}' already exists"),
            Self::TableNotFound(name) => write!(formatter, "table '{name}' was not found"),
            Self::TableAlreadyOpen(name) => {
                write!(formatter, "table '{name}' is open; close it first")
            }
            Self::InvalidDatabaseName(name) => {
                write!(formatter, "'{name}' is not a valid database name")
            }
            Self::InvalidTableName(name) => write!(formatter, "'{name}' is not a valid table name"),
            Self::InvalidDatabaseSettings(message) => {
                write!(formatter, "invalid database settings: {message}")
            }
            Self::RecordIdRequired => {
                write!(
                    formatter,
                    "a record ID is required while auto-increment is disabled"
                )
            }
            Self::ManualRecordIdNotAllowed => write!(
                formatter,
                "record IDs are generated automatically by this database"
            ),
            Self::AutoIncrementOverflow => write!(formatter, "auto-increment record ID overflowed"),
            Self::Io {
                action,
                path,
                source,
            } => write!(
                formatter,
                "could not {action} '{}': {source}",
                path.display()
            ),
            Self::Json(source) => write!(
                formatter,
                "could not parse or serialize table JSON: {source}"
            ),
            Self::InvalidTableFormat => write!(
                formatter,
                "table JSON must contain record, ID, and column objects at indices 0, 1, and 2"
            ),
            Self::DuplicateRecordId(record_id) => {
                write!(formatter, "record ID '{record_id}' already exists")
            }
            Self::RecordNotFound(record_id) => {
                write!(formatter, "record ID '{record_id}' was not found")
            }
            Self::InvalidRecordUuid(record_id) => {
                write!(formatter, "record ID '{record_id}' has an invalid UUID")
            }
            Self::InvalidColumnName(name) if name.trim().is_empty() => {
                write!(formatter, "column name cannot be empty")
            }
            Self::InvalidColumnName(name) => {
                write!(formatter, "'{name}' is not a valid column name")
            }
            Self::ColumnAlreadyExists(name) => write!(formatter, "column '{name}' already exists"),
            Self::ColumnNotFound(name) => write!(formatter, "column '{name}' does not exist"),
            Self::InvalidColumnType(column_type) => write!(
                formatter,
                "column type '{column_type}' is invalid; expected string, number, or bool"
            ),
            Self::InvalidColumnAction(action) => write!(
                formatter,
                "column action '{action}' is invalid; expected '+' or '-'"
            ),
        }
    }
}

impl Error for DatabaseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json(source) => Some(source),
            Self::NoDatabaseOpen
            | Self::NoTableOpen
            | Self::DatabaseAlreadyExists(_)
            | Self::DatabaseNotFound(_)
            | Self::DatabaseAlreadyOpen(_)
            | Self::TableAlreadyExists(_)
            | Self::TableNotFound(_)
            | Self::TableAlreadyOpen(_)
            | Self::InvalidDatabaseName(_)
            | Self::InvalidTableName(_)
            | Self::InvalidDatabaseSettings(_)
            | Self::RecordIdRequired
            | Self::ManualRecordIdNotAllowed
            | Self::AutoIncrementOverflow
            | Self::InvalidTableFormat
            | Self::DuplicateRecordId(_)
            | Self::RecordNotFound(_)
            | Self::InvalidRecordUuid(_)
            | Self::InvalidColumnName(_)
            | Self::ColumnAlreadyExists(_)
            | Self::ColumnNotFound(_)
            | Self::InvalidColumnType(_)
            | Self::InvalidColumnAction(_) => None,
        }
    }
}

/// Settings stored beside a database's table files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseSettings {
    auto_increment_ids: bool,
}

impl DatabaseSettings {
    pub fn auto_increment_ids(&self) -> bool {
        self.auto_increment_ids
    }
}

impl Default for DatabaseSettings {
    fn default() -> Self {
        Self {
            auto_increment_ids: false,
        }
    }
}

/// The application-level owner of the selected database.
/// Use one `AppState` as the source of truth for the application session.
pub struct AppState {
    storage_root: PathBuf,
    active_database: Option<Database>,
}

impl AppState {
    /// Uses `data/` as the directory that stores all databases.
    pub fn new() -> Self {
        Self::with_storage_root("data")
    }

    fn with_storage_root(root: impl Into<PathBuf>) -> Self {
        Self {
            storage_root: root.into(),
            active_database: None,
        }
    }

    /// Returns the names of database directories. Root-level files are ignored.
    pub fn list_databases(&self) -> Result<Vec<String>, DatabaseError> {
        let mut databases = Vec::new();
        let entries = match fs::read_dir(&self.storage_root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(databases),
            Err(source) => {
                return Err(io_error(
                    "list databases",
                    self.storage_root.clone(),
                    source,
                ));
            }
        };

        for entry in entries {
            let entry = entry
                .map_err(|source| io_error("list databases", self.storage_root.clone(), source))?;
            let file_type = entry
                .file_type()
                .map_err(|source| io_error("inspect database entry", entry.path(), source))?;
            if file_type.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    databases.push(name.to_string());
                }
            }
        }
        databases.sort();
        Ok(databases)
    }

    /// Creates a database directory without opening it.
    pub fn create_database(&self, name: &str) -> Result<(), DatabaseError> {
        validate_name(name, true)?;
        fs::create_dir_all(&self.storage_root).map_err(|source| {
            io_error(
                "create the database storage directory",
                self.storage_root.clone(),
                source,
            )
        })?;

        let path = self.storage_root.join(name);
        match fs::create_dir(&path) {
            Ok(()) => {
                let settings = DatabaseSettings::default();
                if let Err(error) = write_database_settings(&path, &settings) {
                    let _ = fs::remove_dir_all(&path);
                    return Err(error);
                }
                Ok(())
            }
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {
                Err(DatabaseError::DatabaseAlreadyExists(name.to_string()))
            }
            Err(source) => Err(io_error("create database", path, source)),
        }
    }

    /// Opens an existing database. Close the active database before switching.
    pub fn open_database(&mut self, name: &str) -> Result<&mut Database, DatabaseError> {
        validate_name(name, true)?;

        if self
            .active_database
            .as_ref()
            .is_some_and(|database| database.name == name)
        {
            return self
                .active_database
                .as_mut()
                .ok_or(DatabaseError::NoDatabaseOpen);
        }
        if let Some(database) = &self.active_database {
            return Err(DatabaseError::DatabaseAlreadyOpen(database.name.clone()));
        }

        let path = self.storage_root.join(name);
        match fs::metadata(&path) {
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => return Err(DatabaseError::DatabaseNotFound(name.to_string())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(DatabaseError::DatabaseNotFound(name.to_string()));
            }
            Err(source) => return Err(io_error("open database", path, source)),
        }

        let settings = load_database_settings(&path)?;
        self.active_database = Some(Database::new(name, path, settings));
        self.active_database
            .as_mut()
            .ok_or(DatabaseError::NoDatabaseOpen)
    }

    /// Closes the active database, including its selected table.
    pub fn close_database(&mut self) {
        self.active_database = None;
    }

    /// Deletes a database directory. The active database must be closed first.
    pub fn delete_database(&self, name: &str) -> Result<(), DatabaseError> {
        validate_name(name, true)?;
        if self
            .active_database
            .as_ref()
            .is_some_and(|database| database.name == name)
        {
            return Err(DatabaseError::DatabaseAlreadyOpen(name.to_string()));
        }

        let path = self.storage_root.join(name);
        match fs::metadata(&path) {
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => return Err(DatabaseError::DatabaseNotFound(name.to_string())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(DatabaseError::DatabaseNotFound(name.to_string()));
            }
            Err(source) => return Err(io_error("inspect database", path, source)),
        }
        fs::remove_dir_all(&path).map_err(|source| io_error("delete database", path, source))
    }

    /// Returns the currently active database name, if any.
    pub fn current_database(&self) -> Option<&str> {
        self.active_database.as_ref().map(Database::name)
    }

    /// Provides access to the active database or an error when none is open.
    pub fn database(&self) -> Result<&Database, DatabaseError> {
        self.active_database
            .as_ref()
            .ok_or(DatabaseError::NoDatabaseOpen)
    }

    /// Provides mutable access to the active database or an error when none is open.
    pub fn database_mut(&mut self) -> Result<&mut Database, DatabaseError> {
        self.active_database
            .as_mut()
            .ok_or(DatabaseError::NoDatabaseOpen)
    }
}

/// A database contains table files in its own directory.
pub struct Database {
    name: String,
    path: PathBuf,
    settings: DatabaseSettings,
    active_table: Option<Table>,
}

impl Database {
    fn new(name: &str, path: PathBuf, settings: DatabaseSettings) -> Self {
        Self {
            name: name.to_string(),
            path,
            settings,
            active_table: None,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn settings(&self) -> &DatabaseSettings {
        &self.settings
    }

    /// Updates the database setting and writes it to `settings.json`.
    pub fn set_auto_increment_ids(&mut self, enabled: bool) -> Result<(), DatabaseError> {
        let updated_settings = DatabaseSettings {
            auto_increment_ids: enabled,
        };
        write_database_settings(&self.path, &updated_settings)?;
        self.settings = updated_settings;
        if let Some(table) = self.active_table.as_mut() {
            table.set_auto_increment_ids(enabled);
        }
        Ok(())
    }

    /// Lists JSON table files in this database directory.
    pub fn list_tables(&self) -> Result<Vec<String>, DatabaseError> {
        let mut tables = Vec::new();
        let entries = fs::read_dir(&self.path)
            .map_err(|source| io_error("list tables", self.path.clone(), source))?;

        for entry in entries {
            let entry =
                entry.map_err(|source| io_error("list tables", self.path.clone(), source))?;
            let file_type = entry
                .file_type()
                .map_err(|source| io_error("inspect table entry", entry.path(), source))?;
            if file_type.is_file()
                && entry.file_name() != "settings.json"
                && entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "json")
            {
                if let Some(table_name) = entry.path().file_stem().and_then(|stem| stem.to_str()) {
                    tables.push(table_name.to_string());
                }
            }
        }
        tables.sort();
        Ok(tables)
    }

    /// Creates and selects a new table with empty records, ID lookup, and columns.
    pub fn create_table(&mut self, name: &str) -> Result<&mut Table, DatabaseError> {
        validate_name(name, false)?;
        let path = table_path(&self.path, name);
        match fs::metadata(&path) {
            Ok(_) => return Err(DatabaseError::TableAlreadyExists(name.to_string())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => return Err(io_error("inspect table", path, source)),
        }
        if let Some(table) = &self.active_table {
            return Err(DatabaseError::TableAlreadyOpen(table.name().to_string()));
        }

        // [0] maps generated UUIDs to record objects.
        let records = serde_json::json!({});
        // [1] maps human-readable record IDs to generated UUIDs.
        let record_ids = serde_json::json!({});
        // [2] maps column names to their declared types.
        let column_definitions = serde_json::json!({});
        // Keep the sections in this order; Table methods use these array indices.
        let initial_table = serde_json::json!([records, record_ids, column_definitions]);
        let contents = serde_json::to_vec_pretty(&initial_table).map_err(DatabaseError::Json)?;

        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {
                return Err(DatabaseError::TableAlreadyExists(name.to_string()));
            }
            Err(source) => return Err(io_error("create table", path, source)),
        };
        if let Err(source) = file.write_all(&contents) {
            drop(file);
            let _ = fs::remove_file(&path);
            return Err(io_error("write table", path, source));
        }

        self.active_table = Some(Table::new(name, path, self.settings.auto_increment_ids));
        self.active_table.as_mut().ok_or(DatabaseError::NoTableOpen)
    }

    /// Opens and selects an existing table. Close the selected table before switching.
    pub fn open_table(&mut self, name: &str) -> Result<&mut Table, DatabaseError> {
        validate_name(name, false)?;
        if self
            .active_table
            .as_ref()
            .is_some_and(|table| table.name() == name)
        {
            return self.active_table.as_mut().ok_or(DatabaseError::NoTableOpen);
        }
        if let Some(table) = &self.active_table {
            return Err(DatabaseError::TableAlreadyOpen(table.name().to_string()));
        }

        let path = table_path(&self.path, name);
        match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => return Err(DatabaseError::TableNotFound(name.to_string())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(DatabaseError::TableNotFound(name.to_string()));
            }
            Err(source) => return Err(io_error("open table", path, source)),
        }

        self.active_table = Some(Table::new(name, path, self.settings.auto_increment_ids));
        self.active_table.as_mut().ok_or(DatabaseError::NoTableOpen)
    }

    /// Closes the selected table, if one is open.
    pub fn close_table(&mut self) {
        self.active_table = None;
    }

    /// Returns the selected table name, if one is open.
    pub fn current_table(&self) -> Option<&str> {
        self.active_table.as_ref().map(Table::name)
    }

    /// Deletes a table file. The selected table must be closed first.
    pub fn delete_table(&mut self, name: &str) -> Result<(), DatabaseError> {
        validate_name(name, false)?;
        if self
            .active_table
            .as_ref()
            .is_some_and(|table| table.name() == name)
        {
            return Err(DatabaseError::TableAlreadyOpen(name.to_string()));
        }

        let path = table_path(&self.path, name);
        match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => return Err(DatabaseError::TableNotFound(name.to_string())),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(DatabaseError::TableNotFound(name.to_string()));
            }
            Err(source) => return Err(io_error("inspect table", path, source)),
        }
        fs::remove_file(&path).map_err(|source| io_error("delete table", path, source))
    }
}

fn validate_name(name: &str, database: bool) -> Result<(), DatabaseError> {
    let invalid = name.is_empty()
        || name == "."
        || name == ".."
        || (!database && name == "settings")
        || name.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*'
                )
        });
    if !invalid {
        return Ok(());
    }
    if database {
        Err(DatabaseError::InvalidDatabaseName(name.to_string()))
    } else {
        Err(DatabaseError::InvalidTableName(name.to_string()))
    }
}

fn table_path(database_path: &Path, table_name: &str) -> PathBuf {
    database_path.join(format!("{table_name}.json"))
}

fn io_error(action: &'static str, path: PathBuf, source: io::Error) -> DatabaseError {
    DatabaseError::Io {
        action,
        path,
        source,
    }
}

fn settings_path(database_path: &Path) -> PathBuf {
    database_path.join("settings.json")
}

fn write_database_settings(
    database_path: &Path,
    settings: &DatabaseSettings,
) -> Result<(), DatabaseError> {
    let path = settings_path(database_path);
    let contents = serde_json::to_string_pretty(&serde_json::json!({
        "auto_increment_ids": settings.auto_increment_ids,
    }))
    .map_err(DatabaseError::Json)?;
    fs::write(&path, contents).map_err(|source| io_error("write database settings", path, source))
}

fn load_database_settings(database_path: &Path) -> Result<DatabaseSettings, DatabaseError> {
    let path = settings_path(database_path);
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let settings = DatabaseSettings::default();
            write_database_settings(database_path, &settings)?;
            return Ok(settings);
        }
        Err(source) => return Err(io_error("read database settings", path, source)),
    };

    let value: serde_json::Value = serde_json::from_str(&contents).map_err(DatabaseError::Json)?;
    let settings = value
        .as_object()
        .ok_or_else(|| DatabaseError::InvalidDatabaseSettings("expected a JSON object".into()))?;
    let auto_increment_ids = match settings.get("auto_increment_ids") {
        Some(value) => value.as_bool().ok_or_else(|| {
            DatabaseError::InvalidDatabaseSettings(
                "'auto_increment_ids' must be true or false".into(),
            )
        })?,
        None => false,
    };
    Ok(DatabaseSettings { auto_increment_ids })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    struct TestStorage(PathBuf);

    impl TestStorage {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("simpi-test-{}", Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestStorage {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn database_and_table_lifecycle_obeys_active_state_rules() {
        let storage = TestStorage::new();
        let mut app = AppState::with_storage_root(&storage.0);

        app.create_database("alpha").unwrap();
        assert_eq!(app.current_database(), None);
        assert!(matches!(app.database(), Err(DatabaseError::NoDatabaseOpen)));
        assert_eq!(app.list_databases().unwrap(), vec!["alpha"]);
        assert!(matches!(
            app.create_database("alpha"),
            Err(DatabaseError::DatabaseAlreadyExists(_))
        ));

        {
            let database = app.open_database("alpha").unwrap();
            assert_eq!(database.list_tables().unwrap(), Vec::<String>::new());
            let table = database.create_table("people").unwrap();
            assert_eq!(table.name(), "people");
            assert_eq!(database.current_table(), Some("people"));
            assert!(matches!(
                database.create_table("people"),
                Err(DatabaseError::TableAlreadyExists(_))
            ));
            assert!(matches!(
                database.open_table("other"),
                Err(DatabaseError::TableAlreadyOpen(_))
            ));
        }

        assert!(matches!(
            app.open_database("beta"),
            Err(DatabaseError::DatabaseAlreadyOpen(_))
        ));
        assert!(matches!(
            app.delete_database("alpha"),
            Err(DatabaseError::DatabaseAlreadyOpen(_))
        ));

        {
            let database = app.database_mut().unwrap();
            assert_eq!(database.list_tables().unwrap(), vec!["people"]);
            assert!(matches!(
                database.delete_table("people"),
                Err(DatabaseError::TableAlreadyOpen(_))
            ));
            database.close_table();
            database.delete_table("people").unwrap();
            assert!(matches!(
                database.open_table("people"),
                Err(DatabaseError::TableNotFound(_))
            ));
            assert_eq!(
                database.create_table("remaining").unwrap().name(),
                "remaining"
            );
            database.close_table();
        }

        app.close_database();
        assert!(matches!(
            app.open_database("beta"),
            Err(DatabaseError::DatabaseNotFound(_))
        ));
        app.delete_database("alpha").unwrap();
        assert!(app.list_databases().unwrap().is_empty());
        assert!(!storage.0.join("alpha").exists());
    }

    #[test]
    fn table_operations_persist_in_the_existing_three_part_json_format() {
        let storage = TestStorage::new();
        let mut app = AppState::with_storage_root(&storage.0);
        app.create_database("alpha").unwrap();

        let id_text;
        {
            let database = app.open_database("alpha").unwrap();
            assert_eq!(database.name(), "alpha");
            let table = database.create_table("people").unwrap();
            let initial: Value = serde_json::from_slice(
                &fs::read(storage.0.join("alpha").join("people.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(initial, serde_json::json!([{}, {}, {}]));

            table.add_column("notes").unwrap();
            table
                .config_columns(vec![
                    ("name".to_string(), "string".to_string(), '+'),
                    ("age".to_string(), "number".to_string(), '+'),
                ])
                .unwrap();
            let (assigned_record_id, id) = table.addrecord(Some("person-1")).unwrap();
            assert_eq!(assigned_record_id, "person-1");
            id_text = id.id.to_string();
            table
                .add("person-1", "name", serde_json::json!("Ada"))
                .unwrap();
            table
                .add_using_uuid(
                    ID {
                        id: Uuid::parse_str(&id_text).unwrap(),
                    },
                    "age",
                    serde_json::json!(36),
                )
                .unwrap();
            table.remove_column("notes").unwrap();
        }

        app.close_database();
        let database = app.open_database("alpha").unwrap();
        let table = database.open_table("people").unwrap();
        let stored: Value =
            serde_json::from_slice(&fs::read(storage.0.join("alpha").join("people.json")).unwrap())
                .unwrap();
        assert_eq!(stored[0][&id_text]["name"], "Ada");
        assert_eq!(stored[0][&id_text]["age"], 36);
        assert!(stored[0][&id_text].get("notes").is_none());
        assert_eq!(stored[1]["person-1"], id_text);
        assert_eq!(stored[2]["name"], "string");
        assert_eq!(stored[2]["age"], "number");
        assert!(matches!(
            table.addrecord(Some("person-1")),
            Err(DatabaseError::DuplicateRecordId(_))
        ));
    }

    #[test]
    fn root_level_json_files_are_preserved_and_not_listed_as_databases() {
        let storage = TestStorage::new();
        let legacy_file = storage.0.join("name.json");
        fs::write(&legacy_file, "legacy sample").unwrap();
        let app = AppState::with_storage_root(&storage.0);

        assert!(app.list_databases().unwrap().is_empty());
        app.create_database("alpha").unwrap();
        assert_eq!(fs::read_to_string(legacy_file).unwrap(), "legacy sample");
        assert_eq!(app.list_databases().unwrap(), vec!["alpha"]);
    }

    #[test]
    fn malformed_table_json_is_reported_by_table_operations() {
        let storage = TestStorage::new();
        let mut app = AppState::with_storage_root(&storage.0);
        app.create_database("alpha").unwrap();
        let database = app.open_database("alpha").unwrap();
        database.create_table("broken").unwrap();
        database.close_table();
        fs::write(storage.0.join("alpha").join("broken.json"), "not json").unwrap();

        let table = database.open_table("broken").unwrap();
        assert!(matches!(
            table.addrecord(Some("record-1")),
            Err(DatabaseError::Json(_))
        ));
    }

    #[test]
    fn auto_increment_setting_is_saved_and_generates_per_table_ids() {
        let storage = TestStorage::new();
        let mut app = AppState::with_storage_root(&storage.0);
        app.create_database("alpha").unwrap();
        app.create_database("beta").unwrap();

        {
            let database = app.open_database("alpha").unwrap();
            assert!(!database.settings().auto_increment_ids());

            {
                let table = database.create_table("items").unwrap();
                assert!(matches!(
                    table.addrecord(None),
                    Err(DatabaseError::RecordIdRequired)
                ));
                assert!(matches!(
                    table.addrecord(Some("  ")),
                    Err(DatabaseError::RecordIdRequired)
                ));
                assert_eq!(table.addrecord(Some("8")).unwrap().0, "8");
            }

            database.set_auto_increment_ids(true).unwrap();
            assert!(database.settings().auto_increment_ids());
            let table = database.open_table("items").unwrap();
            assert!(matches!(
                table.addrecord(Some("manual")),
                Err(DatabaseError::ManualRecordIdNotAllowed)
            ));
            assert_eq!(table.addrecord(None).unwrap().0, "9");
            assert_eq!(table.addrecord(None).unwrap().0, "10");

            database.close_table();
            database.create_table("other_items").unwrap();
            assert_eq!(
                database
                    .open_table("other_items")
                    .unwrap()
                    .addrecord(None)
                    .unwrap()
                    .0,
                "1"
            );
        }

        let settings: Value = serde_json::from_slice(
            &fs::read(storage.0.join("alpha").join("settings.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(settings["auto_increment_ids"], true);

        app.close_database();
        {
            let database = app.open_database("alpha").unwrap();
            assert!(database.settings().auto_increment_ids());
            let table = database.open_table("items").unwrap();
            assert_eq!(table.addrecord(None).unwrap().0, "11");
        }

        app.close_database();
        assert!(
            !app.open_database("beta")
                .unwrap()
                .settings()
                .auto_increment_ids()
        );
    }

    #[test]
    fn adding_data_reports_unknown_record_ids_and_column_names() {
        let storage = TestStorage::new();
        let mut app = AppState::with_storage_root(&storage.0);
        app.create_database("alpha").unwrap();
        let database = app.open_database("alpha").unwrap();
        let table = database.create_table("items").unwrap();
        table
            .config_columns(vec![("name".to_string(), "string".to_string(), '+')])
            .unwrap();
        table.addrecord(Some("known-id")).unwrap();

        let missing_record = table
            .add("missing-id", "name", serde_json::json!("value"))
            .unwrap_err();
        assert!(matches!(
            missing_record,
            DatabaseError::RecordNotFound(ref id) if id == "missing-id"
        ));
        assert!(missing_record.to_string().contains("was not found"));

        let missing_column = table
            .add("known-id", "missing-column", serde_json::json!("value"))
            .unwrap_err();
        assert!(matches!(
            missing_column,
            DatabaseError::ColumnNotFound(ref name) if name == "missing-column"
        ));
        assert!(missing_column.to_string().contains("does not exist"));

        let empty_column = table
            .add("known-id", "", serde_json::json!("value"))
            .unwrap_err();
        assert!(matches!(
            empty_column,
            DatabaseError::InvalidColumnName(ref name) if name.is_empty()
        ));
        assert!(empty_column.to_string().contains("cannot be empty"));
    }

    #[test]
    fn invalid_database_settings_are_rejected_without_opening_database() {
        let storage = TestStorage::new();
        let mut app = AppState::with_storage_root(&storage.0);
        app.create_database("alpha").unwrap();
        fs::write(
            storage.0.join("alpha").join("settings.json"),
            r#"{"auto_increment_ids":"yes"}"#,
        )
        .unwrap();

        assert!(matches!(
            app.open_database("alpha"),
            Err(DatabaseError::InvalidDatabaseSettings(_))
        ));
        assert_eq!(app.current_database(), None);
    }

    #[test]
    fn auto_increment_reports_overflow_instead_of_reusing_an_id() {
        let storage = TestStorage::new();
        let mut app = AppState::with_storage_root(&storage.0);
        app.create_database("alpha").unwrap();

        {
            let database = app.open_database("alpha").unwrap();
            let table = database.create_table("items").unwrap();
            table.addrecord(Some(&u64::MAX.to_string())).unwrap();
            database.set_auto_increment_ids(true).unwrap();
            assert!(matches!(
                database.open_table("items").unwrap().addrecord(None),
                Err(DatabaseError::AutoIncrementOverflow)
            ));
        }
    }
}
