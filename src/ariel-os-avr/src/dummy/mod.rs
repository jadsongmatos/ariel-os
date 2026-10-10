//! Dummy module used to satisfy ariel-os-hal's GPIO builder macros on AVR.
//!
//! AVR GPIO integration is deferred; this module provides the compile-time
//! surface needed for the rest of ariel-os to compile when `context = "avr"`
//! is set. Concrete GPIO usage should use `embassy_avr::*` directly.

#![allow(
    clippy::missing_errors_doc,
    reason = "this module's items are hidden in the docs"
)]
#![allow(
    clippy::module_name_repetitions,
    reason = "this dummy module mimics manufacturer-specific crates"
)]
#![allow(
    clippy::needless_pass_by_value,
    reason = "this dummy module mimics manufacturer-specific crates"
)]
#![allow(unused, reason = "used by documentation only")]

mod executor;

#[doc(hidden)]
#[doc(hidden)]
pub mod peripheral;

#[doc(hidden)]
pub mod identity;

#[doc(hidden)]
#[cfg(feature = "i2c")]
pub mod i2c;

#[doc(hidden)]
#[cfg(feature = "spi")]
pub mod spi;

pub use executor::{Executor, Spawner};
pub use peripheral::{IntoPeripheral, OptionalPeripherals};

#[doc(hidden)]
#[must_use]
pub fn init() -> OptionalPeripherals {
    embassy_avr::time_driver::init();
    #[cfg(feature = "arduino-uno")]
    {
        OptionalPeripherals {
            PB5: None,
            PD7: None,
            PC0: None,
            PD0: None,
            PD1: None,
            PD2: None,
            PD3: None,
            #[cfg(feature = "storage")]
            EEPROM: None,
        }
    }
    #[cfg(not(feature = "arduino-uno"))]
    {
        OptionalPeripherals
    }
}

#[doc(hidden)]
pub struct SWI;
