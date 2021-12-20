use std::{
    borrow::Borrow,
    convert::TryFrom,
    sync::{Arc, Mutex},
};

use arc_swap::ArcSwapOption;
use borsh::BorshSerialize;
use laplace_wasm::database::{Row, Value};
use rusqlite::{types::ValueRef, Connection, OptionalExtension};
use wasmer::{Instance, WasmerEnv};

use crate::lapps::ExpectedInstance;

#[derive(WasmerEnv, Clone)]
pub struct DatabaseEnv {
    pub instance: Arc<ArcSwapOption<Instance>>,
    pub connection: Arc<Mutex<Connection>>,
}

impl<T: Borrow<DatabaseEnv>> From<T> for ExpectedDatabaseEnv {
    fn from(env: T) -> Self {
        let env = env.borrow();
        let instance =
            ExpectedInstance::try_from(env.instance.load_full().expect("Lapp instance should be initialized"))
                .expect("Memory should be presented");

        Self {
            instance,
            connection: env.connection.clone(),
        }
    }
}

#[derive(Clone)]
pub struct ExpectedDatabaseEnv {
    pub instance: ExpectedInstance,
    pub connection: Arc<Mutex<Connection>>,
}

pub fn execute(env: &DatabaseEnv, sql_query_slice: u64) -> u64 {
    run(env, sql_query_slice, do_execute)
}

pub fn query(env: &DatabaseEnv, sql_query_slice: u64) -> u64 {
    run(env, sql_query_slice, do_query)
}

pub fn query_row(env: &DatabaseEnv, sql_query_slice: u64) -> u64 {
    run(env, sql_query_slice, do_query_row)
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
        .query_row(&sql, [], |row| to_row(row))
        .optional()
        .map_err(|err| format!("{:?}", err))
}

fn run<T: BorshSerialize>(
    env: &DatabaseEnv,
    sql_query_slice: u64,
    fun: impl Fn(&Connection, String) -> Result<T, String>,
) -> u64 {
    let env = ExpectedDatabaseEnv::from(env);
    let sql = unsafe {
        env.instance
            .wasm_slice_to_string(sql_query_slice)
            .expect("SQL query should be converted to string")
    };

    let result = env
        .connection
        .try_lock()
        .map_err(|err| format!("{:?}", err))
        .and_then(|connection| fun(&connection, sql));

    let serialized = result.try_to_vec().expect("Result should be serializable");
    env.instance
        .bytes_to_wasm_slice(&serialized)
        .expect("Result should be to move to WASM")
        .into()
}

fn to_row(source: &rusqlite::Row<'_>) -> rusqlite::Result<Row> {
    (0..source.column_count())
        .into_iter()
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
