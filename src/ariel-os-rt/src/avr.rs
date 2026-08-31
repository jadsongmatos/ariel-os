#![expect(unsafe_code)]

// AVR-specific runtime initialization and entry point.
//
// On AVR (Harvard architecture), linkme's distributed_slice cannot be used
// because it relies on `__start_`/`__stop_` linker symbols and `> FLASH`
// memory regions that don't exist in the stock `avr5.x` linker script.
//
// Instead, we use .ctors-based registration: the macros generate
// `#[link_section = ".ctors"]` static function pointers in flash.
// Our `run_ctors()` function iterates `.ctors` using LPM + ICALL (inline asm),
// calling each registration function which stores its function pointer in a
// static array in SRAM.  After `run_ctors()` completes, `startup()` iterates
// these arrays to run init functions and spawn embassy tasks.
//
// We call `run_ctors()` from `main()` (the #[avr_device::entry] function),
// NOT from a `.init6` section, because:
//   - `.init6` naked functions get dead-stripped by the linker on AVR.
//   - `asm!` inside a regular function inherits target-cpu features,
//     while `global_asm!` does not.

use core::sync::atomic::{AtomicUsize, Ordering};

/// Maximum number of init functions (must match the array size below).
const INIT_FUNCS_MAX: usize = 16;

/// Runtime array of init functions, populated by .ctors registration.
/// Initialized to [None; 16] by .bss zeroing, then filled by run_ctors().
#[allow(static_mut_refs)]
static mut INIT_FUNCS_BUF: [Option<fn()>; INIT_FUNCS_MAX] = [None; INIT_FUNCS_MAX];

/// Number of init functions registered so far.
static INIT_FUNCS_LEN: AtomicUsize = AtomicUsize::new(0);

/// Register an init function at startup (called from .ctors registration).
///
/// # Safety
/// Must only be called from .ctors registration functions during run_ctors(),
/// before the executor starts.  Not safe to call concurrently or after startup.
pub fn register_init_func(f: fn()) {
    let idx = INIT_FUNCS_LEN.load(Ordering::Relaxed);
    if idx < INIT_FUNCS_MAX {
        unsafe {
            INIT_FUNCS_BUF[idx] = Some(f);
        }
        INIT_FUNCS_LEN.store(idx + 1, Ordering::Relaxed);
    }
}

/// Return an iterator over registered init functions (AVR runtime registry).
pub(crate) fn init_funcs() -> impl Iterator<Item = fn()> {
    let len = INIT_FUNCS_LEN.load(Ordering::Relaxed);
    (0..len).filter_map(move |i| unsafe { INIT_FUNCS_BUF[i] })
}

// Linker-provided symbols bounding the .ctors section (defined in avr5.x).
unsafe extern "C" {
    static __ctors_start: u8;
    static __ctors_end: u8;
}

/// Iterate the `.ctors` section (in flash) and call each registration function.
///
/// Each entry in `.ctors` is a 2-byte function pointer (word address in flash).
/// We read it with LPM (Load Program Memory), then ICALL (Indirect Call).
///
/// `asm!` inside a regular function inherits `-C target-cpu=atmega328p` features,
/// so `lpm` and `icall` are available without needing `global_asm!`.
///
/// # Safety
/// Must be called once, before the executor starts and before any task spawns.
pub(crate) unsafe fn run_ctors() {
    // Z (r31:r30) = __ctors_start (LPM pointer into flash)
    // Y (r29:r28) = __ctors_end (loop bound)
    // r24:r25 = scratch for LPM'd function pointer
    //
    // Flow:
    //   1. Z = __ctors_start, Y = __ctors_end
    //   2. If Z == Y, done
    //   3. LPM 2-byte fn pointer from flash (Z auto-increments)
    //   4. Save Z (call-clobbered), move fn ptr to Z, ICALL, restore Z
    //   5. Loop to step 2
    unsafe {
        core::arch::asm!(
            // Z (r31:r30) = __ctors_start (LPM pointer into flash)
            "ldi r30, lo8({start})",
            "ldi r31, hi8({start})",
            // Y (r29:r28) = __ctors_end (loop bound)
            "ldi r28, lo8({end})",
            "ldi r29, hi8({end})",
            // Loop: if Z == Y, done
            "2:",
            "cp  r30, r28",
            "cpc r31, r29",
            "breq 3f",
            // LPM 2-byte fn pointer from flash via Z+ (auto-increments)
            "lpm r24, Z+",
            "lpm r25, Z+",
            // Save Z (call-clobbered by ICALL)
            "push r30",
            "push r31",
            // Move fn ptr to Z for ICALL
            "mov r30, r24",
            "mov r31, r25",
            "icall",
            // Restore Z
            "pop r31",
            "pop r30",
            "rjmp 2b",
            "3:",
            start = sym __ctors_start,
            end = sym __ctors_end,
            options(nostack),
        );
        // The ICALL calls registration functions that write to static arrays in
        // SRAM.  The asm! block's implicit "~{memory}" clobber ensures the
        // optimizer does not assume these arrays are still zeroed after
        // run_ctors(), which would cause it to optimize away the executor.
    }
}

/// Entry point for AVR — called by the reset vector after .bss/.data init.
///
/// The `avr-device` crate with the `rt` feature provides the `#[entry]`
/// macro which generates the appropriate reset vector entry point and
/// performs standard startup initialization (zeroing .bss, copying .data
/// from flash, setting up the stack pointer).
#[cfg(not(feature = "embedded-test"))]
#[avr_device::entry]
fn main() -> ! {
    // Run .ctors registration functions to populate init/task arrays in SRAM.
    unsafe { run_ctors(); }
    crate::startup();
}

pub fn init() {}

/// Wait for interrupt — puts the AVR into sleep mode until an interrupt occurs.
///
/// This is the AVR equivalent of `wfi` (wait for interrupt) on ARM.
/// Used by the idle loop in `startup()` when the `threading` feature is not enabled.
pub fn wfi() {
    avr_device::asm::sleep();
}

pub(crate) fn sp() -> usize {
    0
}

use crate::stack::Stack;
pub(crate) fn stack() -> crate::stack::Stack {
    Stack::default()
}
