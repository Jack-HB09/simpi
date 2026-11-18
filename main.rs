use std::io::{self, Write};

use simpi::{AppState, DatabaseError, ID, Table};
use uuid::Uuid;

fn main() {
    let mut app_state = AppState::new();
    run_menu(&mut app_state);
}

fn run_menu(app_state: &mut AppState) {
    loop {
        if app_state.current_database().is_none() {
            if database_menu(app_state) {
                break;
            }
            continue;
        }

        let table_is_open = match app_state.database() {
            Ok(database) => database.current_table().is_some(),
            Err(error) => {
                report_error(error);
                app_state.close_database();
                continue;
            }
        };

        let should_quit = if table_is_open {
            table_menu(app_state)
        } else {
            tables_menu(app_state)
        };
        if should_quit {
            break;
        }
    }

    println!("Goodbye.");
}

/// Menu shown when no database is active.
fn database_menu(app_state: &mut AppState) -> bool {
    println!("\n=== Databases ===");
    println!("1. List databases");
    println!("2. Create database");
    println!("3. Open database");
    println!("4. Delete database");
    println!("0. Quit");

    let Some(choice) = prompt("Choose an option: ") else {
        return true;
    };

    match choice.as_str() {
        "1" => match app_state.list_databases() {
            Ok(databases) => print_names("Databases", &databases),
            Err(error) => report_error(error),
        },
        "2" => {
            let Some(name) = prompt("New database name: ") else {
                return true;
            };
            match app_state.create_database(&name) {
                Ok(()) => println!("Created database '{name}'. Open it to manage its tables."),
                Err(error) => report_error(error),
            }
        }
        "3" => {
            let Some(name) = prompt("Database name to open: ") else {
                return true;
            };
            match app_state.open_database(&name) {
                Ok(database) => println!("Opened database '{}'.", database.name()),
                Err(error) => report_error(error),
            }
        }
        "4" => {
            let Some(name) = prompt("Database name to delete: ") else {
                return true;
            };
            if confirm(&format!(
                "Permanently delete database '{name}' and all its tables?"
            )) {
                match app_state.delete_database(&name) {
                    Ok(()) => println!("Deleted database '{name}'."),
                    Err(error) => report_error(error),
                }
            } else {
                println!("Deletion cancelled.");
            }
        }
        "0" => return true,
        _ => println!("Choose 0, 1, 2, 3, or 4."),
    }

    false
}

/// Menu shown when a database is open but no table is selected.
fn tables_menu(app_state: &mut AppState) -> bool {
    let database_name = app_state
        .current_database()
        .unwrap_or("unknown")
        .to_string();
    let auto_increment_ids = app_state
        .database()
        .map(|database| database.settings().auto_increment_ids())
        .unwrap_or(false);
    println!("\n=== Database: {database_name} ===");
    println!(
        "Auto-increment record IDs: {}",
        if auto_increment_ids { "on" } else { "off" }
    );
    println!("1. List tables");
    println!("2. Create table");
    println!("3. Open table");
    println!("4. Delete table");
    println!("5. Toggle auto-increment record IDs");
    println!("6. Close database");
    println!("0. Quit");

    let Some(choice) = prompt("Choose an option: ") else {
        return true;
    };

    match choice.as_str() {
        "1" => match app_state.database() {
            Ok(database) => match database.list_tables() {
                Ok(tables) => print_names("Tables", &tables),
                Err(error) => report_error(error),
            },
            Err(error) => report_error(error),
        },
        "2" => {
            let Some(name) = prompt("New table name: ") else {
                return true;
            };
            let result = match app_state.database_mut() {
                Ok(database) => database
                    .create_table(&name)
                    .map(|table| table.name().to_string()),
                Err(error) => Err(error),
            };
            match result {
                Ok(name) => println!("Created and opened table '{name}'."),
                Err(error) => report_error(error),
            }
        }
        "3" => {
            let Some(name) = prompt("Table name to open: ") else {
                return true;
            };
            let result = match app_state.database_mut() {
                Ok(database) => database
                    .open_table(&name)
                    .map(|table| table.name().to_string()),
                Err(error) => Err(error),
            };
            match result {
                Ok(name) => println!("Opened table '{name}'."),
                Err(error) => report_error(error),
            }
        }
        "4" => {
            let Some(name) = prompt("Table name to delete: ") else {
                return true;
            };
            if confirm(&format!("Permanently delete table '{name}'?")) {
                let result = match app_state.database_mut() {
                    Ok(database) => database.delete_table(&name),
                    Err(error) => Err(error),
                };
                match result {
                    Ok(()) => println!("Deleted table '{name}'."),
                    Err(error) => report_error(error),
                }
            } else {
                println!("Deletion cancelled.");
            }
        }
        "5" => {
            let result = match app_state.database_mut() {
                Ok(database) => {
                    let enabled = !database.settings().auto_increment_ids();
                    database.set_auto_increment_ids(enabled).map(|()| enabled)
                }
                Err(error) => Err(error),
            };
            match result {
                Ok(true) => println!("Auto-increment record IDs enabled."),
                Ok(false) => println!("Auto-increment record IDs disabled."),
                Err(error) => report_error(error),
            }
        }
        "6" => {
            app_state.close_database();
            println!("Closed database '{database_name}'.");
        }
        "0" => return true,
        _ => println!("Choose 0, 1, 2, 3, 4, 5, or 6."),
    }

    false
}

/// Menu shown when a table is selected.
fn table_menu(app_state: &mut AppState) -> bool {
    let database_name = app_state
        .current_database()
        .unwrap_or("unknown")
        .to_string();
    let table_name = app_state
        .database()
        .ok()
        .and_then(|database| database.current_table())
        .unwrap_or("unknown")
        .to_string();
    let auto_increment_ids = app_state
        .database()
        .map(|database| database.settings().auto_increment_ids())
        .unwrap_or(false);
    println!("\n=== Table: {database_name}/{table_name} ===");
    println!(
        "Auto-increment record IDs: {}",
        if auto_increment_ids { "on" } else { "off" }
    );
    println!("1. View table JSON");
    println!("2. Add record");
    println!("3. Set value by record ID");
    println!("4. Set value by UUID");
    println!("5. Add column");
    println!("6. Remove column");
    println!("7. Close table");
    println!("8. Close database");
    println!("9. Toggle auto-increment record IDs");
    println!("0. Quit");

    let Some(choice) = prompt("Choose an option: ") else {
        return true;
    };

    match choice.as_str() {
        "1" => match with_active_table(app_state, |table| table.read_json()) {
            Ok(json) => match serde_json::to_string_pretty(&json) {
                Ok(contents) => println!("{contents}"),
                Err(error) => eprintln!("Could not display table JSON: {error}"),
            },
            Err(error) => report_error(error),
        },
        "2" => {
            let auto_increment_ids = match app_state.database() {
                Ok(database) => database.settings().auto_increment_ids(),
                Err(error) => {
                    report_error(error);
                    return false;
                }
            };
            let record_id = if auto_increment_ids {
                None
            } else {
                let Some(record_id) = prompt("Record ID: ") else {
                    return true;
                };
                Some(record_id)
            };

            match with_active_table(app_state, |table| table.addrecord(record_id.as_deref())) {
                Ok((assigned_id, id)) => {
                    println!("Added record '{assigned_id}' with UUID {}.", id.id)
                }
                Err(error) => report_error(error),
            }
        }
        "3" => {
            let Some(record_id) = prompt("Record ID: ") else {
                return true;
            };
            let Some(column) = prompt("Column name: ") else {
                return true;
            };
            let Some(value) = prompt_json_value() else {
                return true;
            };
            report_unit(
                with_active_table(app_state, |table| table.add(&record_id, &column, value)),
                "Updated record.",
            );
        }
        "4" => {
            let Some(uuid_text) = prompt("Record UUID: ") else {
                return true;
            };
            let id = match Uuid::parse_str(&uuid_text) {
                Ok(id) => ID { id },
                Err(error) => {
                    eprintln!("Invalid UUID: {error}");
                    return false;
                }
            };
            let Some(column) = prompt("Column name: ") else {
                return true;
            };
            let Some(value) = prompt_json_value() else {
                return true;
            };
            report_unit(
                with_active_table(app_state, |table| table.add_using_uuid(id, &column, value)),
                "Updated record.",
            );
        }
        "5" => {
            let Some(column) = prompt("New column name: ") else {
                return true;
            };
            let Some(column_type) = prompt("Column type (string, number, bool): ") else {
                return true;
            };
            report_unit(
                with_active_table(app_state, |table| {
                    table.config_columns(vec![(column, column_type, '+')])
                }),
                "Added column.",
            );
        }
        "6" => {
            let Some(column) = prompt("Column name to remove: ") else {
                return true;
            };
            report_unit(
                with_active_table(app_state, |table| table.remove_column(&column)),
                "Removed column.",
            );
        }
        "7" => match app_state.database_mut() {
            Ok(database) => {
                database.close_table();
                println!("Closed table '{table_name}'.");
            }
            Err(error) => report_error(error),
        },
        "8" => {
            app_state.close_database();
            println!("Closed database '{database_name}'.");
        }
        "9" => {
            let result = match app_state.database_mut() {
                Ok(database) => {
                    let enabled = !database.settings().auto_increment_ids();
                    database.set_auto_increment_ids(enabled).map(|()| enabled)
                }
                Err(error) => Err(error),
            };
            match result {
                Ok(true) => println!("Auto-increment record IDs enabled."),
                Ok(false) => println!("Auto-increment record IDs disabled."),
                Err(error) => report_error(error),
            }
        }
        "0" => return true,
        _ => println!("Choose an option from 0 to 9."),
    }

    false
}

fn with_active_table<T>(
    app_state: &mut AppState,
    operation: impl FnOnce(&mut Table) -> Result<T, DatabaseError>,
) -> Result<T, DatabaseError> {
    let table_name = app_state
        .database()?
        .current_table()
        .ok_or(DatabaseError::NoTableOpen)?
        .to_string();
    let table = app_state.database_mut()?.open_table(&table_name)?;
    operation(table)
}

fn prompt(message: &str) -> Option<String> {
    print!("{message}");
    if let Err(error) = io::stdout().flush() {
        eprintln!("Could not write prompt: {error}");
        return None;
    }

    let mut input = String::new();
    match io::stdin().read_line(&mut input) {
        Ok(0) => None,
        Ok(_) => Some(input.trim().to_string()),
        Err(error) => {
            eprintln!("Could not read input: {error}");
            None
        }
    }
}

fn prompt_json_value() -> Option<serde_json::Value> {
    loop {
        let text = prompt("JSON value (for example \"hello\", 42, true, or null): ")?;
        match serde_json::from_str(&text) {
            Ok(value) => return Some(value),
            Err(error) => eprintln!("Invalid JSON value: {error}. Try again."),
        }
    }
}

fn confirm(message: &str) -> bool {
    prompt(&format!("{message} Type 'yes' to confirm: "))
        .is_some_and(|answer| answer.eq_ignore_ascii_case("yes"))
}

fn print_names(label: &str, names: &[String]) {
    if names.is_empty() {
        let label = label.to_lowercase();
        println!("No {label} found.");
        return;
    }
    println!("{label}:");
    for name in names {
        println!("- {name}");
    }
}

fn report_unit(result: Result<(), DatabaseError>, success_message: &str) {
    match result {
        Ok(()) => println!("{success_message}"),
        Err(error) => report_error(error),
    }
}

fn report_error(error: DatabaseError) {
    eprintln!("Error: {error}");
}
