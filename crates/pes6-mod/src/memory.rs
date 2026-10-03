//! Reading PES's memory from inside the process, without ever faulting:
//! every read is checked with `VirtualQuery` first.

use std::ffi::c_void;

#[repr(C)]
#[derive(Default)]
struct MemoryBasicInformation {
    base_address: usize,
    allocation_base: usize,
    allocation_protect: u32,
    region_size: usize,
    state: u32,
    protect: u32,
    kind: u32,
}

const MEM_COMMIT: u32 = 0x1000;
const PAGE_NOACCESS: u32 = 0x01;
const PAGE_GUARD: u32 = 0x100;
const READABLE: u32 = 0x02 | 0x04 | 0x08 | 0x20 | 0x40 | 0x80;
const WRITABLE: u32 = 0x04 | 0x08 | 0x40 | 0x80;

unsafe extern "system" {
    fn VirtualQuery(
        address: *const c_void,
        info: *mut MemoryBasicInformation,
        length: usize,
    ) -> usize;
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
}

/// A committed region of the address space.
#[derive(Debug, Clone, Copy)]
pub struct Region {
    pub base: usize,
    pub size: usize,
    pub writable: bool,
}

fn query(address: usize) -> Option<(Region, bool)> {
    let mut info = MemoryBasicInformation::default();
    // SAFETY: VirtualQuery only fills `info`.
    let written = unsafe {
        VirtualQuery(
            address as *const c_void,
            &mut info,
            std::mem::size_of::<MemoryBasicInformation>(),
        )
    };
    if written == 0 {
        return None;
    }
    let readable = info.state == MEM_COMMIT
        && info.protect & READABLE != 0
        && info.protect & (PAGE_GUARD | PAGE_NOACCESS) == 0;
    Some((
        Region {
            base: info.base_address,
            size: info.region_size,
            writable: info.protect & WRITABLE != 0,
        },
        readable,
    ))
}

/// True if `len` bytes from `address` can be read.
pub fn readable(address: usize, len: usize) -> bool {
    let Some(end) = address.checked_add(len) else {
        return false;
    };
    let mut at = address;
    while at < end {
        match query(at) {
            Some((region, true)) => at = region.base + region.size,
            _ => return false,
        }
    }
    address != 0
}

/// Reads a plain value, if the memory is readable.
pub fn read<T: Copy>(address: usize) -> Option<T> {
    // SAFETY: the range was just checked readable; unaligned read.
    readable(address, std::mem::size_of::<T>())
        .then(|| unsafe { (address as *const T).read_unaligned() })
}

/// Copies `len` bytes, if readable.
pub fn read_bytes(address: usize, len: usize) -> Option<Vec<u8>> {
    // SAFETY: as for `read`.
    readable(address, len)
        .then(|| unsafe { std::slice::from_raw_parts(address as *const u8, len).to_vec() })
}

/// Committed, readable regions of the user address space.
pub fn readable_regions() -> Vec<Region> {
    let mut regions = Vec::new();
    let mut at = 0x1_0000usize;
    while at < 0x8000_0000 {
        let Some((region, readable)) = query(at) else {
            break;
        };
        if readable {
            regions.push(region);
        }
        let next = region.base.saturating_add(region.size);
        if next <= at {
            break;
        }
        at = next;
    }
    regions
}

/// Base address of PES6.exe.
pub fn exe_base() -> usize {
    // SAFETY: null asks for the main executable's module.
    unsafe { GetModuleHandleW(std::ptr::null()) as usize }
}
