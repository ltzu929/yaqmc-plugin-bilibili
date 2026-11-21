use alloc::alloc::{alloc, dealloc, realloc, Layout};
#[global_allocator]
static ALLOCATOR: dlmalloc::GlobalDlmalloc = dlmalloc::GlobalDlmalloc;

#[unsafe(export_name = "cabi_realloc")]
unsafe extern "C" fn canonical_realloc(
    old_ptr: *mut u8,
    old_len: usize,
    align: usize,
    new_len: usize,
) -> *mut u8 {
    if old_len == 0 {
        if new_len == 0 {
            return align as *mut u8;
        }
        let layout = Layout::from_size_align_unchecked(new_len, align);
        let ptr = alloc(layout);
        if ptr.is_null() {
            core::arch::wasm32::unreachable();
        }
        return ptr;
    }

    let layout = Layout::from_size_align_unchecked(old_len, align);
    if new_len == 0 {
        dealloc(old_ptr, layout);
        return align as *mut u8;
    }
    let ptr = realloc(old_ptr, layout, new_len);
    if ptr.is_null() {
        core::arch::wasm32::unreachable();
    }
    ptr
}
