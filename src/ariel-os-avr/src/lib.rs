#![expect(
    unsafe_code,
    reason = "constructs pin markers via `core::mem::zeroed()` and takes the EEPROM singleton"
)]
//! Items specific to AVR microcontrollers (ATmega328P / Arduino Uno).

#![no_std]
#![cfg_attr(nightly, feature(doc_cfg))]
#![cfg_attr(target_arch = "avr", feature(abi_avr_interrupt))]
// The pin-change handlers in `gpio` are `extern "avr-interrupt"`, like those of `embassy-avr`.
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

#[cfg(feature = "uart")]
pub mod uart;

/// EEPROM-backed storage support.
#[cfg(feature = "storage")]
pub mod storage;

/// Initialize AVR hardware.
///
/// Starts the Timer0 time driver from `embassy-avr` (1 kHz tick) and returns the set
/// of HAL-owned peripherals: PB5 (D13, the `L` LED), PD7 (D7), PC0 (A0), PD0/PD1
/// (D0/D1, the USB serial RX/TX), and PD2/PD3 (D2/D3, the external interrupt pins).
#[doc(hidden)]
#[must_use]
pub fn init() -> dummy::OptionalPeripherals {
    embassy_avr::time_driver::init();
    #[cfg(feature = "arduino-uno")]
    {
        // The Arduino Uno LED (L) is wired to PB5 / digital pin 13.
        // Pin markers are ZSTs with a private `_private: ()` field that cannot be
        // constructed outside of `atmega-hal`.
        // SAFETY: `core::mem::zeroed()` is sound because the markers carry no state — all
        // routing info comes from `PinOps::into_dynamic()` — and this runs once at startup.
        let pb5: crate::peripherals::PB5 = unsafe { core::mem::zeroed() };
        // SAFETY: as for `pb5`.
        let pd7: crate::peripherals::PD7 = unsafe { core::mem::zeroed() };
        // SAFETY: as for `pb5`.
        let pc0: crate::peripherals::PC0 = unsafe { core::mem::zeroed() };
        // SAFETY: as for `pb5`.
        let pd0: crate::peripherals::PD0 = unsafe { core::mem::zeroed() };
        // SAFETY: as for `pb5`.
        let pd1: crate::peripherals::PD1 = unsafe { core::mem::zeroed() };
        // SAFETY: as for `pd1`.
        let pd2: crate::peripherals::PD2 = unsafe { core::mem::zeroed() };
        // SAFETY: as for `pd1`.
        let pd3: crate::peripherals::PD3 = unsafe { core::mem::zeroed() };
        #[cfg(feature = "storage")]
        // SAFETY: the EEPROM singleton is not taken by `ariel-os-avr::init()`
        // through the PAC, so this is its only owner.
        let eeprom = unsafe { embassy_avr::mcu::pac::EEPROM::steal() };
        dummy::OptionalPeripherals {
            PB5: Some(Peri::new(pb5)),
            PD7: Some(Peri::new(pd7)),
            PC0: Some(Peri::new(pc0)),
            PD0: Some(Peri::new(pd0)),
            PD1: Some(Peri::new(pd1)),
            PD2: Some(Peri::new(pd2)),
            PD3: Some(Peri::new(pd3)),
            #[cfg(feature = "storage")]
            EEPROM: Some(Peri::new(eeprom)),
        }
    }
    #[cfg(not(feature = "arduino-uno"))]
    {
        dummy::OptionalPeripherals
    }
}
