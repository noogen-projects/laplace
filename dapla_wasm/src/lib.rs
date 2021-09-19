pub extern crate borsh;

pub use self::{route::Route, slice::*};

pub mod database;
pub mod invoke;
pub mod process;
pub mod route;
pub mod slice;

#[no_mangle]
pub unsafe fn alloc(size: u32) -> u32 {
    std::alloc::alloc(std::alloc::Layout::from_size_align_unchecked(size as usize, 1)) as u32
}

#[no_mangle]
pub unsafe fn dealloc(ptr: u32, size: u32) {
    std::alloc::dealloc(
        ptr as *mut _,
        std::alloc::Layout::from_size_align_unchecked(size as usize, 1),
    );
}
