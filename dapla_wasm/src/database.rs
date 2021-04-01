use crate::WasmSlice;
use borsh::{BorshDeserialize, BorshSerialize};

extern "C" {
    fn db_execute(sql_query: WasmSlice) -> WasmSlice;
    fn db_query(sql_query: WasmSlice) -> WasmSlice;
    fn db_query_row(sql_query: WasmSlice) -> WasmSlice;
}

pub fn execute(sql: impl Into<String>) -> Result<u64, String> {
    let bytes = unsafe { db_execute(WasmSlice::from(sql.into())).into_vec_in_wasm() };
    BorshDeserialize::try_from_slice(&bytes).expect("Execution result should be deserializable")
}

pub fn query(sql: impl Into<String>) -> Result<Vec<Row>, String> {
    let bytes = unsafe { db_query(WasmSlice::from(sql.into())).into_vec_in_wasm() };
    BorshDeserialize::try_from_slice(&bytes).expect("Query result should be deserializable")
}

pub fn query_row(sql: impl Into<String>) -> Result<Option<Row>, String> {
    let bytes = unsafe { db_query_row(WasmSlice::from(sql.into())).into_vec_in_wasm() };
    BorshDeserialize::try_from_slice(&bytes).expect("Query row result should be deserializable")
}

#[derive(Debug, Clone, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum Value {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct Column {
    name: String,
    decl_type: Option<String>,
}

impl Column {
    pub fn new(name: impl Into<String>, decl_type: impl Into<Option<String>>) -> Self {
        Self {
            name: name.into(),
            decl_type: decl_type.into(),
        }
    }

    /// Returns the name of the column.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the type of the column (`None` for expression).
    pub fn decl_type(&self) -> Option<&str> {
        self.decl_type.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, BorshSerialize, BorshDeserialize)]
pub struct Row {
    values: Vec<Value>,
}

impl Row {
    pub fn new(values: Vec<Value>) -> Self {
        Self { values: values.into() }
    }

    pub fn into_values(self) -> Vec<Value> {
        self.values
    }
}
