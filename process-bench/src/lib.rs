use core::ffi;

pub mod worker;

pub trait Allocator: Sized {
    fn open(name: &str, size: usize) -> Self;
    fn allocate(&mut self, size: usize) -> *mut ffi::c_void;
    unsafe fn deallocate(&mut self, pointer: *mut ffi::c_void);
    unsafe fn address_to_offset(&mut self, address: *mut ffi::c_void) -> u64;
    fn offset_to_address(&mut self, offset: u64) -> *mut ffi::c_void;
}
