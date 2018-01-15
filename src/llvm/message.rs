use std::ptr;
use std::fmt;
use std::ffi::CStr;
use cty::c_char;

use super::ffi::LLVMDisposeMessage;

/// Convinient LLVM Message pointer wrapper.
/// Does not own the ptr, so we have to call `LLVMDisposeMessage` to free message memory.
#[repr(C)]
pub struct Message {
    pub ptr: *mut c_char,
}

impl Message {
    pub fn new() -> Self {
        Message {
            ptr: ptr::null_mut(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.ptr == ptr::null_mut()
    }
}

impl Drop for Message {
    fn drop(&mut self) {
        if !self.is_empty() {
            unsafe {
                LLVMDisposeMessage(self.ptr);
            }
        }
    }
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if !self.is_empty() {
            let contents = unsafe { CStr::from_ptr(self.ptr).to_str().unwrap() };
            write!(f, "{}", contents)
        } else {
            write!(f, "(empty)")
        }
    }
}
