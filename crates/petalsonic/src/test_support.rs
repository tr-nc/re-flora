use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct RealtimeProbeAllocator;

thread_local! {
    static PROBE_ACTIVE: Cell<bool> = const { Cell::new(false) };
    static PROBE_ACTIVITY: Cell<usize> = const { Cell::new(0) };
    static ACOUSTIC_LOOKUP_PROBE_ACTIVE: Cell<bool> = const { Cell::new(false) };
    static ACOUSTIC_LOOKUP_COUNT: Cell<usize> = const { Cell::new(0) };
    static ACOUSTIC_COMPARISON_PROBE_ACTIVE: Cell<bool> = const { Cell::new(false) };
    static ACOUSTIC_COMPARISON_COUNT: Cell<usize> = const { Cell::new(0) };
    static SPATIAL_FRAME_COMPARISON_PROBE_ACTIVE: Cell<bool> = const { Cell::new(false) };
    static SPATIAL_FRAME_COMPARISON_COUNT: Cell<usize> = const { Cell::new(0) };
    static RENDER_COMPLETION_STEP_PROBE_ACTIVE: Cell<bool> = const { Cell::new(false) };
    static RENDER_COMPLETION_STEP_COUNT: Cell<usize> = const { Cell::new(0) };
    static ACOUSTIC_INPUT_COMPARISON_PROBE_ACTIVE: Cell<bool> = const { Cell::new(false) };
    static ACOUSTIC_INPUT_COMPARISON_COUNT: Cell<usize> = const { Cell::new(0) };
}

#[global_allocator]
static REALTIME_PROBE_ALLOCATOR: RealtimeProbeAllocator = RealtimeProbeAllocator;

unsafe impl GlobalAlloc for RealtimeProbeAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        PROBE_ACTIVE.with(|active| {
            if active.get() {
                PROBE_ACTIVITY.with(|count| count.set(count.get() + 1));
            }
        });
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        PROBE_ACTIVE.with(|active| {
            if active.get() {
                PROBE_ACTIVITY.with(|count| count.set(count.get() + 1));
            }
        });
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        PROBE_ACTIVE.with(|active| {
            if active.get() {
                PROBE_ACTIVITY.with(|count| count.set(count.get() + 1));
            }
        });
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

pub(crate) fn realtime_memory_activity(operation: impl FnOnce()) -> usize {
    PROBE_ACTIVITY.with(|count| count.set(0));
    PROBE_ACTIVE.with(|active| active.set(true));
    operation();
    PROBE_ACTIVE.with(|active| active.set(false));
    PROBE_ACTIVITY.with(Cell::get)
}

pub(crate) fn realtime_probe_is_active() -> bool {
    PROBE_ACTIVE.with(Cell::get)
}

pub(crate) fn record_acoustic_response_lookup() {
    ACOUSTIC_LOOKUP_PROBE_ACTIVE.with(|active| {
        if active.get() {
            ACOUSTIC_LOOKUP_COUNT.with(|count| count.set(count.get() + 1));
        }
    });
}

pub(crate) fn acoustic_response_lookup_activity(operation: impl FnOnce()) -> usize {
    ACOUSTIC_LOOKUP_COUNT.with(|count| count.set(0));
    ACOUSTIC_LOOKUP_PROBE_ACTIVE.with(|active| active.set(true));
    operation();
    ACOUSTIC_LOOKUP_PROBE_ACTIVE.with(|active| active.set(false));
    ACOUSTIC_LOOKUP_COUNT.with(Cell::get)
}

pub(crate) fn record_acoustic_publication_comparison() {
    ACOUSTIC_COMPARISON_PROBE_ACTIVE.with(|active| {
        if active.get() {
            ACOUSTIC_COMPARISON_COUNT.with(|count| count.set(count.get() + 1));
        }
    });
}

pub(crate) fn acoustic_publication_comparison_activity(operation: impl FnOnce()) -> usize {
    ACOUSTIC_COMPARISON_COUNT.with(|count| count.set(0));
    ACOUSTIC_COMPARISON_PROBE_ACTIVE.with(|active| active.set(true));
    operation();
    ACOUSTIC_COMPARISON_PROBE_ACTIVE.with(|active| active.set(false));
    ACOUSTIC_COMPARISON_COUNT.with(Cell::get)
}

pub(crate) fn record_spatial_frame_comparison() {
    SPATIAL_FRAME_COMPARISON_PROBE_ACTIVE.with(|active| {
        if active.get() {
            SPATIAL_FRAME_COMPARISON_COUNT.with(|count| count.set(count.get() + 1));
        }
    });
}

pub(crate) fn spatial_frame_comparison_activity(operation: impl FnOnce()) -> usize {
    SPATIAL_FRAME_COMPARISON_COUNT.with(|count| count.set(0));
    SPATIAL_FRAME_COMPARISON_PROBE_ACTIVE.with(|active| active.set(true));
    operation();
    SPATIAL_FRAME_COMPARISON_PROBE_ACTIVE.with(|active| active.set(false));
    SPATIAL_FRAME_COMPARISON_COUNT.with(Cell::get)
}

pub(crate) fn record_render_completion_step() {
    RENDER_COMPLETION_STEP_PROBE_ACTIVE.with(|active| {
        if active.get() {
            RENDER_COMPLETION_STEP_COUNT.with(|count| count.set(count.get() + 1));
        }
    });
}

pub(crate) fn render_completion_step_activity(operation: impl FnOnce()) -> usize {
    RENDER_COMPLETION_STEP_COUNT.with(|count| count.set(0));
    RENDER_COMPLETION_STEP_PROBE_ACTIVE.with(|active| active.set(true));
    operation();
    RENDER_COMPLETION_STEP_PROBE_ACTIVE.with(|active| active.set(false));
    RENDER_COMPLETION_STEP_COUNT.with(Cell::get)
}

pub(crate) fn record_acoustic_input_comparison() {
    ACOUSTIC_INPUT_COMPARISON_PROBE_ACTIVE.with(|active| {
        if active.get() {
            ACOUSTIC_INPUT_COMPARISON_COUNT.with(|count| count.set(count.get() + 1));
        }
    });
}

pub(crate) fn acoustic_input_comparison_activity(operation: impl FnOnce()) -> usize {
    ACOUSTIC_INPUT_COMPARISON_COUNT.with(|count| count.set(0));
    ACOUSTIC_INPUT_COMPARISON_PROBE_ACTIVE.with(|active| active.set(true));
    operation();
    ACOUSTIC_INPUT_COMPARISON_PROBE_ACTIVE.with(|active| active.set(false));
    ACOUSTIC_INPUT_COMPARISON_COUNT.with(Cell::get)
}
