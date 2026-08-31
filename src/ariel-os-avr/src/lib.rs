#![expect(unsafe_code, reason = "constructs PB5 marker via `core::mem::zeroed()` (ZST, private field)")]
//! Items specific to AVR microcontrollers (ATmega328P / Arduino Uno).

#![no_std]
#![cfg_attr(nightly, feature(doc_cfg))]
#![deny(missing_docs)]

/// Real GPIO access for AVR pins (input/output wrappers around `avr_hal_generic::port::Pin`).
#[expect(unsafe_code, reason = "constructs PB5 marker via `core::mem::zeroed()` (ZST, private field)")]
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
/// of HAL-owned peripherals (currently just PB5, the Arduino Uno built-in LED on
/// digital pin 13).
#[doc(hidden)]
#[must_use]
pub fn init() -> dummy::OptionalPeripherals {
    embassy_avr::time_driver::init();
    #[cfg(feature = "arduino-uno")]
    {
        // The Arduino Uno LED (L) is wired to PB5 / digital pin 13.
        // PB5 is a ZST marker with a private `_private: ()` field that we cannot
        // construct outside of `atmega-hal`. `core::mem::zeroed()` is sound because the
        // marker has no state — all routing info comes from `PinOps::into_dynamic()`.
        let pb5: crate::peripherals::PB5 = unsafe { core::mem::zeroed() };
        dummy::OptionalPeripherals {
            PB5: Some(Peri::new(pb5)),
        }
    }
    #[cfg(not(feature = "arduino-uno"))]
    {
        dummy::OptionalPeripherals
    }
}
