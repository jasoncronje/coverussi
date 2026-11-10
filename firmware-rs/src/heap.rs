//! Rust's heap, in PSRAM. Left to malloc, every allocation up to
//! CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL bytes takes internal RAM while there is
//! any: the settings, the songs and each answer's JSON filled it, and then
//! whatever can only live there (a thread's stack, a socket's queues) failed.
//! Internal RAM is left to ESP-IDF and the drivers.

use std::alloc::{GlobalAlloc, Layout};
use std::ops::{Deref, DerefMut};
use std::ptr::NonNull;

use esp_idf_svc::sys;

const PSRAM: u32 = sys::MALLOC_CAP_SPIRAM | sys::MALLOC_CAP_8BIT;
const INTERNAL: u32 = sys::MALLOC_CAP_INTERNAL | sys::MALLOC_CAP_8BIT;
/// What the heap aligns to without being asked.
const ALIGN: usize = 4;

struct Psram;

#[global_allocator]
static HEAP: Psram = Psram;

impl Psram {
    unsafe fn alloc_in(l: Layout, caps: u32) -> *mut u8 {
        if l.align() <= ALIGN {
            sys::heap_caps_malloc(l.size(), caps).cast()
        } else {
            sys::heap_caps_aligned_alloc(l.align(), l.size(), caps).cast()
        }
    }
}

// Anywhere else when PSRAM has no room (or isn't there: QEMU).
unsafe impl GlobalAlloc for Psram {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = Self::alloc_in(l, PSRAM);
        if p.is_null() {
            Self::alloc_in(l, sys::MALLOC_CAP_DEFAULT)
        } else {
            p
        }
    }

    unsafe fn dealloc(&self, p: *mut u8, _: Layout) {
        sys::heap_caps_free(p.cast());
    }

    unsafe fn realloc(&self, p: *mut u8, l: Layout, size: usize) -> *mut u8 {
        if l.align() <= ALIGN {
            let q = sys::heap_caps_realloc(p.cast(), size, PSRAM);
            return if q.is_null() { sys::heap_caps_realloc(p.cast(), size, sys::MALLOC_CAP_DEFAULT).cast() } else { q.cast() };
        }
        let q = self.alloc(Layout::from_size_align_unchecked(size, l.align()));
        if !q.is_null() {
            core::ptr::copy_nonoverlapping(p, q, l.size().min(size));
            self.dealloc(p, l);
        }
        q
    }
}

/// A buffer for data on its way to flash, in internal RAM: a flash write copies
/// what's in PSRAM through 32 bytes at a time, which makes an update crawl.
/// In PSRAM all the same when internal RAM has no room.
pub struct FlashBuffer(NonNull<u8>, usize);

impl FlashBuffer {
    pub fn new(len: usize) -> Self {
        let mut p = unsafe { sys::heap_caps_calloc(1, len, INTERNAL) };
        if p.is_null() {
            p = unsafe { sys::heap_caps_calloc(1, len, sys::MALLOC_CAP_DEFAULT) };
        }
        Self(NonNull::new(p.cast()).expect("memory for a buffer"), len)
    }
}

impl Deref for FlashBuffer {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.0.as_ptr(), self.1) }
    }
}

impl DerefMut for FlashBuffer {
    fn deref_mut(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.0.as_ptr(), self.1) }
    }
}

impl Drop for FlashBuffer {
    fn drop(&mut self) {
        unsafe { sys::heap_caps_free(self.0.as_ptr().cast()) };
    }
}
