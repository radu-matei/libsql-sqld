//! A library for communicating with a libSQL database over HTTP.
//!
//! libsql-client is a lightweight HTTP-based driver for sqld,
//! which is a server mode for libSQL, which is an open-contribution fork of SQLite.
//!
//! libsql-client compiles to wasm32-unknown-unknown target, which makes it a great
//! driver for environments that run on WebAssembly.
//!
//! It is expected to become a general-purpose driver for communicating with sqld/libSQL,
//! but the only backend implemented at the moment is for Cloudflare Workers environment.

use std::collections::HashMap;
use std::iter::IntoIterator;

use anyhow::{bail, Context, Result};
use base64::Engine;
use serde_json::Value;

pub mod statement;
pub use statement::Statement;

pub mod cell_value;
pub use cell_value::CellValue;

/// Metadata of a database request
#[derive(Clone, Debug, Default)]
pub struct Meta {
    pub duration: u64,
}

/// A database row
#[derive(Clone, Debug)]
pub struct Row {
    pub cells: HashMap<String, CellValue>,
}

/// Structure holding a set of rows returned from a query
/// and their corresponding column names
#[derive(Clone, Debug)]
pub struct ResultSet {
    pub columns: Vec<String>,
    pub rows: Vec<Row>,
}

/// Result of a database request - a set of rows or an error
#[derive(Clone, Debug)]
pub enum QueryResult {
    Error((String, Meta)),
    Success((ResultSet, Meta)),
}

/// Database connection. This is the main structure used to
/// communicate with the database.
#[derive(Clone, Debug)]
pub struct Connection {
    url: String,
    // auth: String,
}

fn parse_columns(columns: Vec<serde_json::Value>, result_idx: usize) -> Result<Vec<String>> {
    let mut result = Vec::with_capacity(columns.len());
    for (idx, column) in columns.into_iter().enumerate() {
        match column {
            serde_json::Value::String(column) => result.push(column),
            _ => {
                bail!(format!(
                    "Result {result_idx} column name {idx} not a string",
                ))
            }
        }
    }
    Ok(result)
}

fn parse_value(
    cell: serde_json::Value,
    result_idx: usize,
    row_idx: usize,
    cell_idx: usize,
) -> Result<CellValue> {
    match cell {
        serde_json::Value::Null => Ok(CellValue::Null),
        serde_json::Value::Bool(v) => Ok(CellValue::Bool(v)),
        serde_json::Value::Number(v) => match v.as_i64() {
            Some(v) => Ok(CellValue::Number(v)),
            None => match v.as_f64() {
                Some(v) => Ok(CellValue::Float(v)),
                None => bail!(format!(
                    "Result {result_idx} row {row_idx} cell {cell_idx} had unknown number value: {v}",
                )),
            },
        },
        serde_json::Value::String(v) => Ok(CellValue::Text(v)),
        _ => bail!(format!(
            "Result {result_idx} row {row_idx} cell {cell_idx} had unknown type",
        )),
    }
}

fn parse_rows(
    rows: Vec<serde_json::Value>,
    columns: &Vec<String>,
    result_idx: usize,
) -> Result<Vec<Row>> {
    let mut result = Vec::with_capacity(rows.len());
    for (idx, row) in rows.into_iter().enumerate() {
        match row {
            serde_json::Value::Array(row) => {
                if row.len() != columns.len() {
                    bail!(format!(
                        "Result {result_idx} row {idx} had wrong number of cells",
                    ));
                }
                let mut cells = HashMap::with_capacity(columns.len());
                for (cell_idx, value) in row.into_iter().enumerate() {
                    cells.insert(
                        columns[cell_idx].clone(),
                        parse_value(value, result_idx, idx, cell_idx)?,
                    );
                }
                result.push(Row { cells })
            }
            _ => {
                bail!(format!("Result {result_idx} row {idx} was not an array",))
            }
        }
    }
    Ok(result)
}

fn parse_query_result(result: serde_json::Value, idx: usize) -> Result<QueryResult> {
    match result {
        serde_json::Value::Object(obj) => {
            if let Some(err) = obj.get("error") {
                return match err {
                    serde_json::Value::Object(obj) => match obj.get("message") {
                        Some(serde_json::Value::String(msg)) => {
                            Ok(QueryResult::Error((msg.clone(), Meta::default())))
                        }
                        _ => bail!(format!("Result {idx} error message was not a string",)),
                    },
                    _ => bail!(format!("Result {idx} results was not an object",)),
                };
            }

            let results = obj.get("results");
            match results {
                Some(serde_json::Value::Object(obj)) => {
                    let columns = obj
                        .get("columns")
                        .context(format!("Result {idx} had no columns"))?;
                    let rows = obj
                        .get("rows")
                        .context(format!("Result {idx} had no rows"))?;
                    match (rows, columns) {
                        (serde_json::Value::Array(rows), serde_json::Value::Array(columns)) => {
                            let columns = parse_columns(columns.to_vec(), idx)?;
                            let rows = parse_rows(rows.to_vec(), &columns, idx)?;
                            Ok(QueryResult::Success((
                                ResultSet { columns, rows },
                                Meta::default(),
                            )))
                        }
                        _ => bail!(format!(
                            "Result {idx} had rows or columns that were not an array",
                        )),
                    }
                }
                Some(_) => bail!(format!("Result {idx} was not an object",)),
                None => bail!(format!("Result {idx} did not contain results or error",)),
            }
        }
        _ => bail!(format!("Result {idx} was not an object",)),
    }
}

impl Connection {
    /// Establishes a database connection.
    ///
    /// # Arguments
    /// * `url` - URL of the database endpoint
    /// * `username` - database username
    /// * `pass` - user's password
    pub fn connect(
        url: impl Into<String>,
        // username: impl Into<String>,
        // pass: impl Into<String>,
    ) -> Self {
        // let username = username.into();
        // let pass = pass.into();
        // let url = url.into();
        // // Auto-update the URL to start with https:// if no protocol was specified
        // let url = if !url.contains("://") {
        //     "https://".to_owned() + &url
        // } else {
        //     url
        // };
        Self {
            url: url.into(),
            // auth: format!(
            //     "Basic {}",
            //     base64::engine::general_purpose::STANDARD.encode(format!("{username}:{pass}"))
            // ),
        }
    }

    /// Executes a single SQL statement
    ///
    /// # Arguments
    /// * `stmt` - the SQL statement
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f() {
    /// let db = libsql_client::Connection::connect("https://example.com", "admin", "s3cr3tp4ss");
    /// let result = db.execute("SELECT * FROM sqlite_master").await;
    /// let result_params = db
    ///     .execute(libsql_client::Statement::with_params(
    ///         "UPDATE t SET v = ? WHERE key = ?",
    ///         &[libsql_client::CellValue::Number(5), libsql_client::CellValue::Text("five".to_string())],
    ///     ))
    ///     .await;
    /// # }
    /// ```
    pub async fn execute(&self, stmt: impl Into<Statement>) -> Result<QueryResult> {
        let mut results = self.batch(std::iter::once(stmt)).await?;
        Ok(results.remove(0))
    }

    /// Executes a batch of SQL statements.
    /// Each statement is going to run in its own transaction,
    /// unless they're wrapped in BEGIN and END
    ///
    /// # Arguments
    /// * `stmts` - SQL statements
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f() {
    /// let db = libsql_client::Connection::connect("https://example.com", "admin", "s3cr3tp4ss");
    /// let result = db
    ///     .batch(["CREATE TABLE t(id)", "INSERT INTO t VALUES (42)"])
    ///     .await;
    /// # }
    /// ```
    pub async fn batch(
        &self,
        stmts: impl IntoIterator<Item = impl Into<Statement>>,
    ) -> Result<Vec<QueryResult>> {
        // FIXME: serialize and deserialize with existing routines from sqld
        let mut body = "{\"statements\": [".to_string();
        let mut stmts_count = 0;
        for stmt in stmts {
            body += &format!("{},", stmt.into());
            stmts_count += 1;
        }
        if stmts_count > 0 {
            body.pop();
        }
        body += "]}";

        let req = http::Request::builder()
            .uri(&self.url)
            .method("POST")
            // .header("Authorization", &self.auth)
            .body(Some(body.clone().into()))?;

        let res = match spin_sdk::outbound_http::send_request(req) {
            Ok(res) => res,
            Err(e) => bail!(format!("Error sending request to database: {}", e)),
        };

        match res.body() {
            Some(b) => {
                let response_json: Value = serde_json::from_slice(b)?;

                match response_json {
                    serde_json::Value::Array(results) => {
                        if results.len() != stmts_count {
                            bail!(format!(
                                "Response array did not contain expected {stmts_count} results"
                            ))
                        } else {
                            let mut query_results: Vec<QueryResult> =
                                Vec::with_capacity(stmts_count);
                            for (idx, result) in results.into_iter().enumerate() {
                                query_results.push(parse_query_result(result, idx)?);
                            }

                            Ok(query_results)
                        }
                    }
                    e => bail!(format!("Error: {} ({:?})", e, body)),
                }
            }
            None => bail!("response from database was empty"),
        }
    }

    /// Executes an SQL transaction.
    /// Does not support nested transactions - do not use BEGIN or END
    /// inside a transaction.
    ///
    /// # Arguments
    /// * `stmts` - SQL statements
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f() {
    /// let db = libsql_client::Connection::connect("https://example.com", "admin", "s3cr3tp4ss");
    /// let result = db
    ///     .transaction(["CREATE TABLE t(id)", "INSERT INTO t VALUES (42)"])
    ///     .await;
    /// # }
    /// ```
    pub async fn transaction(
        &self,
        stmts: impl IntoIterator<Item = impl Into<Statement>>,
    ) -> Result<Vec<QueryResult>> {
        // TODO: Vec is not a good fit for popping the first element,
        // let's return a templated collection instead and let the user
        // decide where to store the result.
        let mut ret: Vec<QueryResult> = self
            .batch(
                std::iter::once(Statement::new("BEGIN"))
                    .chain(stmts.into_iter().map(|s| s.into()))
                    .chain(std::iter::once(Statement::new("END"))),
            )
            .await?
            .into_iter()
            .skip(1)
            .collect();
        ret.pop();
        Ok(ret)
    }
}
