use std::sync::Arc;

use borsh::BorshSerialize;
use laplace_wasm::database::{Row, Value};
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OptionalExtension};
use tokio::sync::Mutex;
use wasmtime::Caller;

use crate::lapps::wasm_interop::BoxedSendFuture;
use crate::lapps::Ctx;

pub struct DatabaseCtx {
    pub connection: Arc<Mutex<Connection>>,
}

impl DatabaseCtx {
    pub fn new(connection: Connection) -> Self {
        Self {
            connection: Arc::new(Mutex::new(connection)),
        }
    }
}

pub fn execute(caller: Caller<Ctx>, (sql_query_slice,): (u64,)) -> BoxedSendFuture<u64> {
    Box::new(run(caller, sql_query_slice, do_execute))
}

pub fn query(caller: Caller<Ctx>, (sql_query_slice,): (u64,)) -> BoxedSendFuture<u64> {
    Box::new(run(caller, sql_query_slice, do_query))
}

pub fn query_row(caller: Caller<Ctx>, (sql_query_slice,): (u64,)) -> BoxedSendFuture<u64> {
    Box::new(run(caller, sql_query_slice, do_query_row))
}

pub fn do_execute(connection: &Connection, sql: String) -> Result<u64, String> {
    let updated_rows = connection.execute(&sql, []).map_err(|err| format!("{}", err))?;
    Ok(updated_rows as _)
}

pub fn do_query(connection: &Connection, sql: String) -> Result<Vec<Row>, String> {
    connection
        .prepare(&sql)
        .and_then(|mut stmt| {
            let mut rows = Vec::new();
            let mut provider = stmt.query([])?;
            while let Some(row) = provider.next()? {
                rows.push(to_row(row)?);
            }
            Ok(rows)
        })
        .map_err(|err| format!("{:?}", err))
}

pub fn do_query_row(connection: &Connection, sql: String) -> Result<Option<Row>, String> {
    connection
        .query_row(&sql, [], to_row)
        .optional()
        .map_err(|err| format!("{:?}", err))
}

async fn run<T: BorshSerialize + Send>(
    mut caller: Caller<'_, Ctx>,
    sql_query_slice: u64,
    fun: impl Fn(&Connection, String) -> Result<T, String>,
) -> u64 {
    let memory_data = caller.data().memory_data().clone();

    let sql = memory_data
        .to_manager(&mut caller)
        .wasm_slice_to_string(sql_query_slice)
        .await
        .expect("SQL query should be converted to string");

    let result = match caller.data().database.as_ref() {
        Some(database_ctx) => {
            let connection = database_ctx.connection.lock().await;
            fun(&connection, sql)
        },
        None => Err("Database context not found".to_string()),
    };

    let serialized = borsh::to_vec(&result).expect("Result should be serializable");
    memory_data
        .to_manager(&mut caller)
        .bytes_to_wasm_slice(&serialized)
        .await
        .expect("Result should be to move to WASM")
        .into()
}

fn to_row(source: &rusqlite::Row<'_>) -> rusqlite::Result<Row> {
    (0..source.as_ref().column_count())
        .map(|idx| source.get_ref(idx).map(to_value))
        .collect::<Result<_, _>>()
        .map(Row::new)
}

fn to_value(source: ValueRef<'_>) -> Value {
    match source {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(val) => Value::Integer(val),
        ValueRef::Real(val) => Value::Real(val),
        ValueRef::Text(val) => Value::Text(String::from_utf8_lossy(val).into()),
        ValueRef::Blob(val) => Value::Blob(val.into()),
    }
}
