use borsh::BorshSerialize;
pub use dapla_wasm::{alloc, dealloc};
use dapla_wasm::{
    database::{execute, query, Value},
    WasmSlice,
};
use sql_builder::{quote, SqlBuilder, SqlBuilderError};
use thiserror::Error;
use todo_common::{Response, Task};

const TASKS_TABLE_NAME: &str = "Tasks";

#[no_mangle]
pub unsafe extern "C" fn init() -> WasmSlice {
    let result = execute(format!(
        r"CREATE TABLE IF NOT EXISTS {table}(
            description TEXT NOT NULL,
            completed INTEGER NOT NULL DEFAULT 0 CHECK(completed IN (0,1))
        );",
        table = TASKS_TABLE_NAME
    ));

    let data = result
        .map(drop)
        .try_to_vec()
        .expect("Init result should be serializable");
    WasmSlice::from(data)
}

#[no_mangle]
pub unsafe extern "C" fn get(uri: WasmSlice) -> WasmSlice {
    WasmSlice::from(do_get(uri.into_string_in_wasm()))
}

fn do_get(uri: String) -> String {
    let response = TodoRequest::parse(&uri, None)
        .map(|request| request.process())
        .unwrap_or_else(Response::Error);
    serde_json::to_string(&response).unwrap_or_else(Response::json_error_from)
}

#[no_mangle]
pub unsafe extern "C" fn post(uri: WasmSlice, body: WasmSlice) -> WasmSlice {
    WasmSlice::from(do_post(uri.into_string_in_wasm(), body.into_string_in_wasm()))
}

fn do_post(uri: String, body: String) -> String {
    let response = TodoRequest::parse(&uri, Some(&body))
        .map(|request| request.process())
        .unwrap_or_else(Response::Error);
    serde_json::to_string(&response).unwrap_or_else(Response::json_error_from)
}

#[derive(Debug, Error)]
enum TaskError {
    #[error("Invalid SQL query: {0}")]
    Sql(#[from] SqlBuilderError),

    #[error("Error: {0}")]
    AnyhowError(#[from] anyhow::Error),

    #[error("Error message: {0}")]
    ErrorMessage(String),
}

impl From<String> for TaskError {
    fn from(message: String) -> Self {
        Self::ErrorMessage(message)
    }
}

impl From<TaskError> for Response {
    fn from(err: TaskError) -> Self {
        Response::Error(format!("{}", err))
    }
}

enum TodoRequest {
    List,
    Add(Task),
    Update(u32, Task),
    Delete(u32),
    ClearCompleted,
}

impl TodoRequest {
    fn parse(uri: &str, body: Option<&str>) -> Result<Self, String> {
        let chunks: Vec<_> = uri.split(|c| c == '/').collect();
        match &chunks[..] {
            [.., "list"] => Ok(Self::List),
            [.., "add"] => {
                let body = body.ok_or_else(|| "Task not specified".to_string())?;
                parse_task(body).map(Self::Add)
            }
            [.., "update", idx] => {
                let idx = parse_idx(idx)?;
                let body = body.ok_or_else(|| "Task not specified".to_string())?;
                parse_task(body).map(|task| Self::Update(idx, task))
            }
            [.., "delete", idx] => parse_idx(idx).map(Self::Delete),
            [.., "clear_completed"] => Ok(Self::ClearCompleted),
            _ => Err(format!("Cannot parse uri {}, {:?}", uri, chunks)),
        }
    }

    fn process(self) -> Response {
        match self {
            Self::List => process_list().map(Response::List),
            Self::Add(task) => process_add(task).map(Response::List),
            Self::Update(idx, task) => process_update(idx, task).map(|_| Response::Empty),
            Self::Delete(idx) => process_delete(idx).map(Response::List),
            Self::ClearCompleted => process_clear_completed().map(Response::List),
        }
        .unwrap_or_else(Response::from)
    }
}

fn parse_idx(source: &str) -> Result<u32, String> {
    source
        .parse()
        .map_err(|err| format!("Parse task index error: {:?}", err))
}

fn parse_task(source: &str) -> Result<Task, String> {
    serde_json::from_str(source).map_err(|err| format!("Parse task error: {:?}", err))
}

fn process_list() -> Result<Vec<Task>, TaskError> {
    let sql = SqlBuilder::select_from(TASKS_TABLE_NAME).sql()?;
    let rows = query(sql)?;

    let mut tasks = Vec::with_capacity(rows.len());
    for row in rows {
        tasks.push(task_from(row.into_values())?);
    }
    Ok(tasks)
}

fn process_add(task: Task) -> Result<Vec<Task>, TaskError> {
    let sql = SqlBuilder::insert_into(TASKS_TABLE_NAME)
        .fields(&["description", "completed"])
        .values(&[quote(task.description), if task.completed { 1 } else { 0 }.to_string()])
        .sql()?;
    execute(sql)?;
    process_list()
}

fn process_update(idx: u32, update: Task) -> Result<(), TaskError> {
    let sql = SqlBuilder::update_table(TASKS_TABLE_NAME)
        .set("description", quote(update.description))
        .set("completed", update.completed)
        .and_where_eq("rowid", idx)
        .sql()?;
    execute(sql)?;
    execute("VACUUM")?;
    Ok(())
}

fn process_delete(idx: u32) -> Result<Vec<Task>, TaskError> {
    let sql = SqlBuilder::delete_from(TASKS_TABLE_NAME)
        .and_where_eq("rowid", idx)
        .sql()?;
    execute(sql)?;
    execute("VACUUM")?;
    process_list()
}

fn process_clear_completed() -> Result<Vec<Task>, TaskError> {
    let sql = SqlBuilder::delete_from(TASKS_TABLE_NAME)
        .and_where_ne("completed", 0)
        .sql()?;
    execute(sql)?;
    execute("VACUUM")?;
    process_list()
}

fn task_from(values: Vec<Value>) -> Result<Task, String> {
    let mut task = Task::default();
    let mut iter = values.into_iter();

    match iter.next() {
        Some(Value::Text(description)) => task.description = description,
        Some(value) => Err(format!("Incorrect task description value: {:?}", value))?,
        None => Err("Task description value does not exist".to_string())?,
    }

    match iter.next() {
        Some(Value::Integer(completed)) => task.completed = completed != 0,
        Some(value) => Err(format!("Incorrect task completed value: {:?}", value))?,
        None => Err("Task completed value does not exist".to_string())?,
    }

    Ok(task)
}
