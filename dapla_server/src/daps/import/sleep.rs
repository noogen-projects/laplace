use std::{thread, time::Duration};

pub fn invoke_sleep(millis: u64) {
    thread::sleep(Duration::from_millis(millis));
}
