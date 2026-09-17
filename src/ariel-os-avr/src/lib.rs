#![expect(unsafe_code, reason = "constructs pin markers via `core::mem::zeroed()` (ZSTs, private fields)")]
//! Items specific to AVR microcontrollers (ATmega328P / Arduino Uno).

#![no_std]
#![cfg_attr(nightly, feature(doc_cfg))]
#![deny(missing_docs)]

/// Real GPIO access for AVR pins (input/output wrappers around `avr_hal_generic::port::Pin`).
pub mod gpio;

/// Dummy module providing the compile-time surface for `ariel_os_hal::peripheral`.
#[doc(hidden)]
pub mod dummy;

// Re-export dummy peripheral module items at crate root.
#[doc(hidden)]
pub use dummy::peripheral;
#[doc(hidden)]
pub use dummy::peripheral::IntoPeripheral;
#[doc(hidden)]
pub use dummy::peripheral::OptionalPeripherals;
#[doc(hidden)]
pub use dummy::peripheral::Peri;

// Re-export pin types from embassy-avr board crate (when arduino-uno feature is enabled).
#[cfg(feature = "arduino-uno")]
#[doc(hidden)]
pub mod peripherals {
    // Pin types (PB5 etc.) come from atmega-hal's port module.
    pub use embassy_avr::mcu::port::*;
}

/// Time driver integration — delegates to `embassy_avr::time_driver`.
pub mod time_driver;

/// Initialize AVR hardware.
///
/// Starts the Timer0 time driver from `embassy-avr` (1 kHz tick) and returns the set
/// of HAL-owned peripherals: PB5 (D13, the `L` LED), PD7 (D7) and PC0 (A0).
#[doc(hidden)]
#[must_use]
pub fn init() -> dummy::OptionalPeripherals {
    embassy_avr::time_driver::init();
    #[cfg(feature = "arduino-uno")]
    {
        // The Arduino Uno LED (L) is wired to PB5 / digital pin 13.
        // Pin markers are ZSTs with a private `_private: ()` field that cannot be
        // constructed outside of `atmega-hal`. `core::mem::zeroed()` is sound because the
        // markers carry no state — all routing info comes from `PinOps::into_dynamic()`.
        let pb5: crate::peripherals::PB5 = unsafe { core::mem::zeroed() };
        let pd7: crate::peripherals::PD7 = unsafe { core::mem::zeroed() };
        let pc0: crate::peripherals::PC0 = unsafe { core::mem::zeroed() };
        dummy::OptionalPeripherals {
            PB5: Some(Peri::new(pb5)),
            PD7: Some(Peri::new(pd7)),
            PC0: Some(Peri::new(pc0)),
        }
    }
    #[cfg(not(feature = "arduino-uno"))]
    {
        dummy::OptionalPeripherals
    }
}
