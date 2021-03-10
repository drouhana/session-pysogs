use rusqlite::params;
use warp::{Rejection, http::StatusCode};

use super::models;
use super::storage;

/// Inserts the given `message` into the database if it's valid.
pub async fn insert_message(mut message: models::Message, pool: storage::DatabaseConnectionPool) -> Result<impl warp::Reply, Rejection> {
    // Validate the message
    if !message.is_valid() { return Err(warp::reject::custom(models::ValidationError)); }
    // Get a connection and open a transaction
    let mut conn = storage::conn(&pool)?;
    let tx = storage::tx(&mut conn)?;
    // Insert the message
    storage::exec("INSERT INTO (?1) (text) VALUES (?2)", params![ storage::MESSAGES_TABLE, message.text ], &tx)?;
    let id = tx.last_insert_rowid();
    message.server_id = Some(id);
    // Commit
    tx.commit(); // TODO: Unwrap
    // Return
    return Ok(warp::reply::json(&message));
}

/// Returns either the last `limit` messages or all messages since `from_server_id, limited to `limit`.
pub async fn get_messages(options: models::QueryOptions, pool: storage::DatabaseConnectionPool) -> Result<impl warp::Reply, Rejection> {
    // Get a database connection
    let conn = storage::conn(&pool)?;
    // Unwrap parameters
    let from_server_id = options.from_server_id.unwrap_or(0);
    let limit = options.limit.unwrap_or(256); // Never return more than 256 messages at once
    // Query the database
    let raw_query: &str;
    if options.from_server_id.is_some() {
        raw_query = "SELECT id, text FROM (?1) WHERE rowid > (?2) LIMIT (?3)";
    } else {
        raw_query = "SELECT id, text FROM (?1) ORDER BY rowid DESC LIMIT (?3)";
    }
    let mut query = storage::query(&raw_query, &conn)?;
    let rows = match query.query_map(params![ storage::MESSAGES_TABLE, from_server_id, limit ], |row| {
        Ok(models::Message { server_id : row.get(0)?, text : row.get(1)? })
    }) {
        Ok(rows) => rows,
        Err(e) => {
            println!("Couldn't query database due to error: {:?}.", e);
            return Err(warp::reject::custom(storage::DatabaseError));
        }
    };
    // FIXME: It'd be cleaner to do the below using `collect()`, but the compiler has trouble
    // inferring the item type of `rows` in that case.
    let mut messages: Vec<models::Message> = Vec::new();
    for row in rows {
        match row {
            Ok(message) => messages.push(message),
            Err(e) => {
                println!("Excluding message from response due to database error: {:?}.", e);
                continue;
            }
        }
    }
    // Return the messages
    return Ok(warp::reply::json(&messages));
}

/// Deletes the message with the given `row_id` from the database, if it's present.
pub async fn delete_message(row_id: i64, pool: storage::DatabaseConnectionPool) -> Result<impl warp::Reply, Rejection> {
    // Get a connection and open a transaction
    let mut conn = storage::conn(&pool)?;
    let tx = storage::tx(&mut conn)?;
    // Delete the message if it's present
    let count = storage::exec("DELETE FROM (?1) WHERE rowid = (?2)", params![ storage::MESSAGES_TABLE, row_id ], &tx)?;
    // Update the deletions table if needed
    if count > 0 {
        storage::exec("INSERT INTO (?1) (id) VALUES (?2)", params![ storage::DELETED_MESSAGES_TABLE, row_id ], &tx)?;
    }
    // Commit
    tx.commit(); // TODO: Unwrap
    // Return
    return Ok(StatusCode::OK);
}

/// Returns either the last `limit` deleted messages or all deleted messages since `from_server_id, limited to `limit`.
pub async fn get_deleted_messages(options: models::QueryOptions, pool: storage::DatabaseConnectionPool) -> Result<impl warp::Reply, Rejection> {
    // Get a database connection
    let conn = storage::conn(&pool)?;
    // Unwrap parameters
    let from_server_id = options.from_server_id.unwrap_or(0);
    let limit = options.limit.unwrap_or(256); // Never return more than 256 deleted messages at once
    // Query the database
    let raw_query: &str;
    if options.from_server_id.is_some() {
        raw_query = "SELECT id FROM (?1) WHERE rowid > (?2) LIMIT (?3)";
    } else {
        raw_query = "SELECT id FROM (?1) ORDER BY rowid DESC LIMIT (?3)";
    }
    let mut query = storage::query(&raw_query, &conn)?;
    let rows = match query.query_map(params![ storage::DELETED_MESSAGES_TABLE, from_server_id, limit ], |row| {
        Ok(row.get(0)?)
    }) {
        Ok(rows) => rows,
        Err(e) => {
            println!("Couldn't query database due to error: {:?}.", e);
            return Err(warp::reject::custom(storage::DatabaseError));
        }
    };
    // FIXME: It'd be cleaner to do the below using `collect()`, but the compiler has trouble
    // inferring the item type of `rows` in that case.
    let mut ids: Vec<i64> = Vec::new();
    for row in rows {
        match row {
            Ok(id) => ids.push(id),
            Err(e) => {
                println!("Excluding deleted message from response due to database error: {:?}.", e);
                continue;
            }
        }
    }
    // Return the IDs
    return Ok(warp::reply::json(&ids));
}