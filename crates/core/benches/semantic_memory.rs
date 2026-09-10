mod support;

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};

use support::{array_documents, array_schema, compiled, guide};

struct CountingAllocator;

static ALLOCATION_COUNT: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record_allocation(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            record_allocation(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let replacement = unsafe { System.realloc(pointer, layout, new_size) };
        if !replacement.is_null() {
            LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
            record_allocation(new_size);
        }
        replacement
    }
}

fn record_allocation(bytes: usize) {
    ALLOCATION_COUNT.fetch_add(1, Ordering::Relaxed);
    ALLOCATED_BYTES.fetch_add(bytes, Ordering::Relaxed);
    let live = LIVE_BYTES.fetch_add(bytes, Ordering::Relaxed) + bytes;
    PEAK_LIVE_BYTES.fetch_max(live, Ordering::Relaxed);
}

fn profile(name: &str, operation: impl FnOnce()) {
    let allocations = ALLOCATION_COUNT.load(Ordering::Relaxed);
    let allocated = ALLOCATED_BYTES.load(Ordering::Relaxed);
    let live = LIVE_BYTES.load(Ordering::Relaxed);
    PEAK_LIVE_BYTES.store(live, Ordering::Relaxed);
    operation();
    let retained = LIVE_BYTES.load(Ordering::Relaxed) as i128 - live as i128;
    println!(
        "profile={name} allocations={} allocated_bytes={} peak_live_delta_bytes={} retained_delta_bytes={retained}",
        ALLOCATION_COUNT.load(Ordering::Relaxed) - allocations,
        ALLOCATED_BYTES.load(Ordering::Relaxed) - allocated,
        PEAK_LIVE_BYTES.load(Ordering::Relaxed).saturating_sub(live),
    );
}

fn main() {
    let documents = array_documents(1_000);
    let compiled = compiled(
        &array_schema(
            1_000,
            r#","uniqueItems":true,"contains":{"type":"string"},"minContains":1"#,
        ),
        &documents,
    );
    profile("guide-construction", || {
        black_box(guide(compiled.clone(), 128));
    });

    let mut mask_guide = guide(compiled.clone(), 128);
    let mut mask = vec![0_u32; mask_guide.model_width().div_ceil(32)];
    profile("mask", || {
        black_box(mask_guide.write_mask(&mut mask).expect("mask succeeds"));
    });

    for depth in [0, 1, 8, 32, 128] {
        let mut guide = guide(compiled.clone(), depth);
        if depth != 0 {
            guide.advance(0).expect("advance succeeds");
        }
        println!(
            "rollback_depth={depth} available={} retained_rollback_bytes={}",
            guide.rollback_available(),
            guide.retained_rollback_bytes()
        );
    }
}
