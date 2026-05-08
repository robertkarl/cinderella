/// macOS memory pressure FFI bindings.
///
/// Uses `DISPATCH_SOURCE_TYPE_MEMORYPRESSURE` to receive kernel memory
/// pressure notifications via a GCD dispatch source. Events are forwarded
/// through a tokio unbounded channel so async code can react.
///
/// The dispatch source and channel sender are intentionally leaked — they
/// live for the entire process lifetime.

use std::os::raw::{c_char, c_ulong, c_void};

// ── Opaque GCD types (names mirror macOS C conventions) ────────────────

#[allow(non_camel_case_types)]
type dispatch_queue_t = *mut c_void;
#[allow(non_camel_case_types)]
type dispatch_source_t = *mut c_void;
#[allow(non_camel_case_types)]
type dispatch_source_type_t = *const c_void;

// ── Memory-pressure flag constants ─────────────────────────────────────

const DISPATCH_MEMORYPRESSURE_NORMAL: c_ulong = 0x01;
const DISPATCH_MEMORYPRESSURE_WARN: c_ulong = 0x02;
const DISPATCH_MEMORYPRESSURE_CRITICAL: c_ulong = 0x04;

// ── libdispatch FFI ────────────────────────────────────────────────────

extern "C" {
    static _dispatch_source_type_memorypressure: dispatch_source_type_t;

    fn dispatch_queue_create(label: *const c_char, attr: *const c_void) -> dispatch_queue_t;
    fn dispatch_source_create(
        type_: dispatch_source_type_t,
        handle: c_ulong,
        mask: c_ulong,
        queue: dispatch_queue_t,
    ) -> dispatch_source_t;
    fn dispatch_source_set_event_handler_f(
        source: dispatch_source_t,
        handler: extern "C" fn(*mut c_void),
    );
    fn dispatch_set_context(object: *mut c_void, context: *mut c_void);
    fn dispatch_source_get_data(source: dispatch_source_t) -> c_ulong;
    fn dispatch_resume(object: *mut c_void);
}

// ── PressureLevel enum ─────────────────────────────────────────────────

/// Memory pressure level reported by macOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressureLevel {
    Normal,
    Warn,
    Critical,
}

impl PressureLevel {
    /// Convert raw `dispatch_source_get_data` flags to a `PressureLevel`.
    pub fn from_raw(flags: c_ulong) -> Self {
        if flags & DISPATCH_MEMORYPRESSURE_CRITICAL != 0 {
            PressureLevel::Critical
        } else if flags & DISPATCH_MEMORYPRESSURE_WARN != 0 {
            PressureLevel::Warn
        } else {
            PressureLevel::Normal
        }
    }
}

// ── Context passed into the C handler ──────────────────────────────────

/// Boxed context leaked into the dispatch source so the C callback can
/// reach both the tokio sender and the source pointer.
struct HandlerContext {
    tx: tokio::sync::mpsc::UnboundedSender<PressureLevel>,
    source: dispatch_source_t,
}

/// C-callable event handler. Reads flags from the dispatch source and
/// sends the corresponding `PressureLevel` through the channel.
extern "C" fn pressure_handler(ctx: *mut c_void) {
    unsafe {
        let ctx = &*(ctx as *const HandlerContext);
        let flags = dispatch_source_get_data(ctx.source);
        let level = PressureLevel::from_raw(flags);
        // Best-effort send; if the receiver is dropped we just ignore it.
        let _ = ctx.tx.send(level);
    }
}

// ── Public API ─────────────────────────────────────────────────────────

/// Start listening for macOS memory pressure events.
///
/// Returns a tokio channel receiver that yields `PressureLevel` values
/// whenever the kernel signals a change in memory pressure. The dispatch
/// source and sender are intentionally leaked (process-lifetime).
pub fn start_pressure_listener() -> tokio::sync::mpsc::UnboundedReceiver<PressureLevel> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

    unsafe {
        let label = b"com.glass-slipper.memory-pressure\0";
        let queue = dispatch_queue_create(label.as_ptr() as *const c_char, std::ptr::null());

        let mask = DISPATCH_MEMORYPRESSURE_WARN | DISPATCH_MEMORYPRESSURE_CRITICAL;
        let source = dispatch_source_create(
            _dispatch_source_type_memorypressure,
            0,
            mask,
            queue,
        );

        let ctx = Box::new(HandlerContext { tx, source });
        let ctx_ptr = Box::into_raw(ctx) as *mut c_void; // intentionally leaked

        dispatch_set_context(source as *mut c_void, ctx_ptr);
        dispatch_source_set_event_handler_f(source, pressure_handler);
        dispatch_resume(source as *mut c_void);
    }

    rx
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressure_level_from_raw_warn() {
        assert_eq!(
            PressureLevel::from_raw(DISPATCH_MEMORYPRESSURE_WARN),
            PressureLevel::Warn,
        );
    }

    #[test]
    fn pressure_level_from_raw_critical() {
        assert_eq!(
            PressureLevel::from_raw(DISPATCH_MEMORYPRESSURE_CRITICAL),
            PressureLevel::Critical,
        );
    }

    #[test]
    fn pressure_level_from_raw_normal() {
        assert_eq!(
            PressureLevel::from_raw(DISPATCH_MEMORYPRESSURE_NORMAL),
            PressureLevel::Normal,
        );
    }

    #[test]
    fn pressure_level_from_raw_unknown_falls_back_to_normal() {
        assert_eq!(PressureLevel::from_raw(0x00), PressureLevel::Normal);
        assert_eq!(PressureLevel::from_raw(0xFF00), PressureLevel::Normal);
    }
}
