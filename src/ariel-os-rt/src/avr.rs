#![expect(unsafe_code)]

// AVR-specific runtime initialization and entry point.
//
// On AVR (Harvard architecture), linkme's distributed_slice cannot be used
// because it relies on `__start_`/`__stop_` linker symbols and `> FLASH`
// memory regions that don't exist in the stock `avr5.x` linker script.
//
// Instead, we use .ctors-based registration: the macros generate
// `#[link_section = ".ctors"]` static function pointers in flash.
// libgcc's `__do_global_ctors`, which avr-gcc uses for C++ constructors,
// calls each registration function at startup, in the `.init6` section,
// before `main`; each one stores its function pointer in a static array in
// SRAM. `startup()` then iterates these arrays to run init functions and
// spawn embassy tasks.
//
// `__do_global_ctors` walks the table from its end, so the registrations run
// in reverse link order; like the order of linkme's slices, it is
// unspecified.

use core::sync::atomic::{AtomicUsize, Ordering};

/// Maximum number of init functions (must match the array size below).
const INIT_FUNCS_MAX: usize = 16;

/// Runtime array of init functions, populated by .ctors registration.
/// Initialized to [None; 16] by .bss zeroing, then filled by the registration
/// functions.
#[allow(static_mut_refs)]
static mut INIT_FUNCS_BUF: [Option<fn()>; INIT_FUNCS_MAX] = [None; INIT_FUNCS_MAX];

/// Number of init functions registered so far.
static INIT_FUNCS_LEN: AtomicUsize = AtomicUsize::new(0);

/// Register an init function at startup (called from .ctors registration).
///
/// # Safety
/// Must only be called from .ctors registration functions, which
/// `__do_global_ctors` runs before `main`.  Not safe to call concurrently or
/// after startup.
pub fn register_init_func(f: fn()) {
    let idx = INIT_FUNCS_LEN.load(Ordering::Relaxed);
    if idx < INIT_FUNCS_MAX {
        let buf = core::ptr::addr_of_mut!(INIT_FUNCS_BUF).cast::<Option<fn()>>();
        // SAFETY: `idx` is below `INIT_FUNCS_MAX`, so `buf.add(idx)` stays inside
        // `INIT_FUNCS_BUF`. This runs only from the `.ctors` registration functions,
        // before the executor starts; see the `# Safety` section. `addr_of_mut!`
        // gives a raw pointer, so no reference to the mutable static is created.
        unsafe { buf.add(idx).write(Some(f)) };
        INIT_FUNCS_LEN.store(idx + 1, Ordering::Relaxed);
    }
}

/// Return an iterator over registered init functions (AVR runtime registry).
pub(crate) fn init_funcs() -> impl Iterator<Item = fn()> {
    let len = INIT_FUNCS_LEN.load(Ordering::Relaxed);
    let buf = core::ptr::addr_of!(INIT_FUNCS_BUF).cast::<Option<fn()>>();
    // SAFETY: `i` is below `len`, which never exceeds `INIT_FUNCS_MAX`, and the
    // registry is filled before the executor starts.
    (0..len).filter_map(move |i| unsafe { *buf.add(i) })
}

unsafe extern "C" {
    /// The constructor runner of libgcc, in the `.init6` startup section.
    ///
    /// It calls every function of the `.ctors` table, from the last to the first, after the
    /// startup code initialized `.data` and `.bss`, and before `main`.
    fn __do_global_ctors();
}

/// Makes the linker take [`__do_global_ctors`] from libgcc, which it only does for a referenced
/// symbol. Nothing calls the function: it runs as part of the startup code.
///
/// `.progmem.data` keeps the pointer in flash; other read-only data would be copied to RAM.
#[used]
#[unsafe(link_section = ".progmem.data")]
static DO_GLOBAL_CTORS: unsafe extern "C" fn() = __do_global_ctors;

/// Entry point for AVR, called at the end of the startup code.
///
/// The startup code of avr-libc and libgcc runs first: it zeroes `r1`, copies `.data` from
/// flash, zeroes `.bss`, and runs the `.ctors` registrations. The `#[entry]` macro of the
/// `avr-device` crate, with its `rt` feature, places `main` in `.init9`, after all of them.
#[cfg(not(feature = "embedded-test"))]
#[avr_device::entry]
fn main() -> ! {
    crate::startup();
}

/// Paints the stack, so that [`Stack::peak_usage()`] measures it from here on.
pub fn init() {
    Stack::get().repaint();
}

/// Wait for interrupt — puts the AVR into sleep mode until an interrupt occurs.
///
/// This is the AVR equivalent of `wfi` (wait for interrupt) on ARM.
/// Used by the idle loop in `startup()` when the `threading` feature is not enabled.
pub fn wfi() {
    avr_device::asm::sleep();
}

/// Returns the address of the last byte pushed on the stack.
///
/// The AVR stack pointer points to the next free byte: a push stores the byte, then decrements
/// the pointer.
pub(crate) fn sp() -> usize {
    /// Data-space addresses of `SPL` and `SPH`, the two halves of the stack pointer.
    const SPL: *const u8 = 0x5d as *const u8;
    const SPH: *const u8 = 0x5e as *const u8;
    // SAFETY: `SPL` and `SPH` are always readable, and reading them has no side effects.
    let (low, high) = unsafe { (SPL.read_volatile(), SPH.read_volatile()) };
    usize::from(u16::from_le_bytes([low, high])) + 1
}

use crate::stack::Stack;
/// Returns the stack, which grows down from the end of the RAM to the end of the static data.
pub(crate) fn stack() -> Stack {
    unsafe extern "C" {
        /// End of `.data`, `.bss` and `.noinit`, from the AVR linker scripts.
        static _end: u8;
        /// Last byte of the RAM, where the stack starts.
        static __stack: u8;
    }
    let lowest = &raw const _end as usize;
    let highest = &raw const __stack as usize + 1;
    Stack::new(lowest, highest)
}
