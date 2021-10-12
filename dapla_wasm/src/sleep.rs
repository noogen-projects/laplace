extern "C" {
    fn invoke_sleep(millis: u64);
}

pub fn invoke(millis: u64) {
    unsafe { invoke_sleep(millis) }
}
