use dapla_wasm::WasmSlice;
pub use dapla_wasm::{alloc, dealloc};
use thiserror::Error;
use todo_common::{Response, Task};

#[no_mangle]
pub unsafe extern "C" fn get(uri: WasmSlice) -> WasmSlice {
    WasmSlice::from(do_get(uri.into_string()))
}

fn do_get(uri: String) -> String {
    let response = TodoRequest::parse(&uri, None)
        .map(|request| request.process())
        .unwrap_or_else(Response::Error);
    serde_json::to_string(&response).unwrap_or_else(Response::json_error_from)
}

#[no_mangle]
pub unsafe extern "C" fn post(uri: WasmSlice, body: WasmSlice) -> WasmSlice {
    WasmSlice::from(do_post(uri.into_string(), body.into_string()))
}

fn do_post(uri: String, body: String) -> String {
    let response = TodoRequest::parse(&uri, Some(&body))
        .map(|request| request.process())
        .unwrap_or_else(Response::Error);
    serde_json::to_string(&response).unwrap_or_else(Response::json_error_from)
}

static mut TASK_LIST: Vec<Task> = Vec::new();

#[derive(Debug, Error)]
enum TaskError {
    #[error("File name is not valid utf-8 string")]
    WrongFileName,
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
    Ok(unsafe { TASK_LIST.clone() })
}

fn process_add(task: Task) -> Result<Vec<Task>, TaskError> {
    unsafe {
        TASK_LIST.push(task);
    }
    process_list()
}

fn process_update(idx: u32, update: Task) -> Result<(), TaskError> {
    unsafe {
        TASK_LIST[idx as usize] = update;
    }
    Ok(())
}

fn process_delete(idx: u32) -> Result<Vec<Task>, TaskError> {
    unsafe {
        TASK_LIST.remove(idx as usize);
    }
    process_list()
}

fn process_clear_completed() -> Result<Vec<Task>, TaskError> {
    unsafe {
        TASK_LIST = TASK_LIST.drain(..).filter(|task| !task.completed).collect();
    }
    process_list()
}
