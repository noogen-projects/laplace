use std::collections::HashMap;
use std::ffi::OsString;
use std::io::BufRead;
use std::time::Duration;
use std::{fs, io, thread};

use log::{debug, error};
use subprocess::{make_pipe, Exec, Popen, Redirection, Result as PopenResult};

use crate::port::next_free_local_port;
use crate::{target_build_dir, LaplaceClient};

pub mod env {
    pub const SSL_ENABLED: &str = "LAPLACE__SSL__ENABLED";
    pub const HTTP_HOST: &str = "LAPLACE__HTTP__HOST";
    pub const HTTP_PORT: &str = "LAPLACE__HTTP__PORT";
}

pub struct LaplaceService {
    test_name: String,
    subprocess: Option<Popen>,
    envs: HashMap<String, OsString>,
    args: Vec<String>,
    http_host: String,
    http_port: u16,
}

impl LaplaceService {
    pub fn new(test_name: impl Into<String>) -> Self {
        Self {
            test_name: test_name.into(),
            subprocess: None,
            envs: HashMap::new(),
            args: Vec::new(),
            http_host: "127.0.0.1".to_string(),
            http_port: next_free_local_port(),
        }
    }

    pub fn with_arg(mut self, arg: impl ToString) -> Self {
        self.args.push(arg.to_string());
        self
    }

    pub fn with_var(mut self, key: &str, val: impl Into<OsString>) -> Self {
        self.add_var(key, val);
        self
    }

    pub fn with_vars(mut self, env: &[(&str, &str)]) -> Self {
        self.add_vars(env);
        self
    }

    pub fn with_host(mut self, host: impl ToString) -> Self {
        self.http_host = host.to_string();
        self
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.http_port = port;
        self
    }

    pub fn add_var(&mut self, key: &str, val: impl Into<OsString>) {
        self.envs.insert(key.into(), val.into());
    }

    pub fn add_vars(&mut self, env: &[(&str, &str)]) {
        for (key, val) in env {
            self.envs.insert((*key).into(), val.into());
        }
    }

    fn run_exec(&mut self) -> PopenResult<fs::File> {
        let working_dir = std::env::current_dir().expect("Cannot get working dir");
        let bin_path = target_build_dir().join("laplace_server");

        debug!("Starting process {:?}", bin_path);

        self.add_var(env::HTTP_HOST, self.http_host.clone());
        self.add_var(env::HTTP_PORT, self.http_port.to_string());

        let config_path = working_dir.join("config").join("settings.toml");
        let envs: Vec<_> = self.envs.iter().collect();
        let (pipe_read, pipe_write) = make_pipe()?;

        let subprocess = Exec::cmd(bin_path)
            .env_extend(&envs)
            .arg("--settings-path")
            .arg(config_path)
            .args(self.args.as_slice())
            .stdout(Redirection::Pipe)
            .stderr(Redirection::File(pipe_write))
            .detached()
            .popen()?;

        let pid = subprocess.pid().expect("PID must be present");
        debug!("Started process PID {pid} for test {}", self.test_name);

        self.subprocess = Some(subprocess);
        Ok(pipe_read)
    }

    pub fn start(mut self) -> Self {
        let stdout = self.run_exec().expect("Fail to run service");

        thread::spawn(move || {
            let reader = io::BufReader::new(stdout);

            for line in reader.lines() {
                let line = line.unwrap_or_else(|err| err.to_string());
                println!("{line}");
            }
        });

        self
    }

    pub async fn http_client(&self) -> LaplaceClient {
        let client = LaplaceClient::http(&self.http_host, self.http_port)
            .build()
            .expect("Cannot build laplace client");
        client
            .wait_to_ready(Duration::from_secs(60))
            .await
            .expect("Connection error");
        client
    }

    pub async fn https_client(&self) -> LaplaceClient {
        let client = LaplaceClient::https(&self.http_host, self.http_port)
            .build()
            .expect("Cannot build laplace client");
        client
            .wait_to_ready(Duration::from_secs(60))
            .await
            .expect("Connection error");
        client
    }
}

impl Drop for LaplaceService {
    fn drop(&mut self) {
        if let Some(ref mut subprocess) = self.subprocess {
            if let Some(pid) = subprocess.pid() {
                debug!("Stopping service process for {:?}, PID = {pid}", self.test_name);

                for _ in 0..20 {
                    debug!("Try terminate subprocess for {:?}, PID = {pid}", self.test_name);
                    if let Err(err) = subprocess.terminate() {
                        error!("Fail to terminate subprocess: {err}");
                    }

                    match subprocess.wait_timeout(Duration::from_secs(1)) {
                        Err(err) => {
                            error!("Unable to stop process {pid}: {err:?}");
                        },
                        Ok(None) => {
                            continue;
                        },
                        Ok(Some(_)) => {
                            break;
                        },
                    }
                }

                if let Some(exit_status) = subprocess.poll() {
                    debug!(
                        "Service process for {:?} stopped with {exit_status:?}, PID = {pid}",
                        self.test_name
                    );
                } else {
                    debug!("Kill the service process for {:?}, PID = {pid}", self.test_name);
                    subprocess.kill().expect("Cannot kill subprocess");
                    panic!("Wait too long for the process to terminate, PID = {}", pid);
                }
            }
        }
    }
}
