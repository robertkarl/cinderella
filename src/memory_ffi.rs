/// macOS memory pressure FFI bindings.
///
/// On macOS: uses `DISPATCH_SOURCE_TYPE_MEMORYPRESSURE` to receive kernel memory
/// pressure notifications via a GCD dispatch source. Events are forwarded
/// through a tokio unbounded channel so async code can react.
///
/// On Linux: provides the same public API with a no-op listener (channel never sends).

/// Memory pressure level reported by macOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressureLevel {
    Normal,
    Warn,
    Critical,
}

// ── macOS implementation ──────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod platform {
    use super::PressureLevel;
    use std::os::raw::{c_char, c_ulong, c_void};

    #[allow(non_camel_case_types)]
    type dispatch_queue_t = *mut c_void;
    #[allow(non_camel_case_types)]
    type dispatch_source_t = *mut c_void;
    #[allow(non_camel_case_types)]
    type dispatch_source_type_t = *const c_void;

    #[repr(C)]
    #[allow(non_camel_case_types)]
    struct dispatch_source_type_s {
        _opaque: [u8; 0],
    }

    const DISPATCH_MEMORYPRESSURE_NORMAL: c_ulong = 0x01;
    const DISPATCH_MEMORYPRESSURE_WARN: c_ulong = 0x02;
    const DISPATCH_MEMORYPRESSURE_CRITICAL: c_ulong = 0x04;

    extern "C" {
        static _dispatch_source_type_memorypressure: dispatch_source_type_s;
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

    impl PressureLevel {
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

    struct HandlerContext {
        tx: tokio::sync::mpsc::UnboundedSender<PressureLevel>,
        source: dispatch_source_t,
    }

    extern "C" fn pressure_handler(ctx: *mut c_void) {
        unsafe {
            let ctx = &*(ctx as *const HandlerContext);
            let flags = dispatch_source_get_data(ctx.source);
            let level = PressureLevel::from_raw(flags);
            let _ = ctx.tx.send(level);
        }
    }

    pub fn start_pressure_listener() -> tokio::sync::mpsc::UnboundedReceiver<PressureLevel> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        unsafe {
            let label = b"com.glass-slipper.memory-pressure\0";
            let queue = dispatch_queue_create(label.as_ptr() as *const c_char, std::ptr::null());

            let mask = DISPATCH_MEMORYPRESSURE_WARN | DISPATCH_MEMORYPRESSURE_CRITICAL;
            let source = dispatch_source_create(
                &_dispatch_source_type_memorypressure as *const _ as dispatch_source_type_t,
                0,
                mask,
                queue,
            );

            let ctx = Box::new(HandlerContext { tx, source });
            let ctx_ptr = Box::into_raw(ctx) as *mut c_void;

            dispatch_set_context(source as *mut c_void, ctx_ptr);
            dispatch_source_set_event_handler_f(source, pressure_handler);
            dispatch_resume(source as *mut c_void);
        }

        rx
    }

    #[cfg(test)]
    pub(super) const TEST_DISPATCH_MEMORYPRESSURE_NORMAL: c_ulong = DISPATCH_MEMORYPRESSURE_NORMAL;
    #[cfg(test)]
    pub(super) const TEST_DISPATCH_MEMORYPRESSURE_WARN: c_ulong = DISPATCH_MEMORYPRESSURE_WARN;
    #[cfg(test)]
    pub(super) const TEST_DISPATCH_MEMORYPRESSURE_CRITICAL: c_ulong = DISPATCH_MEMORYPRESSURE_CRITICAL;
}

// ── Linux stub ────────────────────────────────────────────────────────

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::PressureLevel;

    pub fn start_pressure_listener() -> tokio::sync::mpsc::UnboundedReceiver<PressureLevel> {
        let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
        // tx is dropped immediately, so rx will never receive anything.
        // The memory monitor's select! loop will just never hit this branch.
        rx
    }
}

// ── Re-exports ────────────────────────────────────────────────────────

pub use platform::start_pressure_listener;

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn pressure_level_from_raw_warn() {
        assert_eq!(
            PressureLevel::from_raw(platform::TEST_DISPATCH_MEMORYPRESSURE_WARN),
            PressureLevel::Warn,
        );
    }

    #[test]
    fn pressure_level_from_raw_critical() {
        assert_eq!(
            PressureLevel::from_raw(platform::TEST_DISPATCH_MEMORYPRESSURE_CRITICAL),
            PressureLevel::Critical,
        );
    }

    #[test]
    fn pressure_level_from_raw_normal() {
        assert_eq!(
            PressureLevel::from_raw(platform::TEST_DISPATCH_MEMORYPRESSURE_NORMAL),
            PressureLevel::Normal,
        );
    }

    #[test]
    fn pressure_level_from_raw_unknown_falls_back_to_normal() {
        assert_eq!(PressureLevel::from_raw(0x00), PressureLevel::Normal);
        assert_eq!(PressureLevel::from_raw(0xFF00), PressureLevel::Normal);
    }
}
