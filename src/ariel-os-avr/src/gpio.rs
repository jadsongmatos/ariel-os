#![expect(unsafe_code, reason = "uses `core::mem::transmute` to type-erase AVR pin input mode (PhantomData only)")]
//! Provides GPIO access for AVR microcontrollers (ATmega328P / Arduino Uno).
//!
//! Wraps [`avr_hal_generic::port::Pin`] with type-erased modes and a lifetime
//! parameter so that HAL-generic code in [`ariel_os::gpio`] can operate on
//! AVR pins just like on cortex-m or RISC-V targets.

use core::marker::PhantomData;

pub mod input {
    //! Input-specific types.

    use core::marker::PhantomData;
    use ariel_os_embassy_common::gpio::Pull;

    #[cfg(feature = "external-interrupts")]
    use ariel_os_embassy_common::gpio::input::InterruptError;

    // Re-export the HAL-specific pin mode types.
    #[doc(hidden)]
    pub use embassy_avr::hal::port::mode;

    /// AVR's [`InputPin`](embassy_avr::hal::port::PinOps) trait is implemented by every concrete
    /// pin marker type (`PB5`, `PC0`, etc.) via the blanket `PinOps` impl. We use a trait
    /// alias to match the HAL-generic surface expected by [`ariel_os_hal::gpio`].
    #[doc(hidden)]
    pub trait InputPin: embassy_avr::hal::port::PinOps<Dynamic = embassy_avr::mcu::port::Dynamic> {}

    #[doc(hidden)]
    impl<T: embassy_avr::hal::port::PinOps<Dynamic = embassy_avr::mcu::port::Dynamic>> InputPin for T {}

    /// Whether inputs support configuring whether a Schmitt trigger is enabled.
    ///
    /// ATmega328P inputs have a fixed Schmitt trigger; this cannot be disabled at runtime.
    pub const SCHMITT_TRIGGER_CONFIGURABLE: bool = false;

    /// Digital level of an input.
    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    #[cfg_attr(feature = "defmt", derive(defmt::Format))]
    pub enum Level {
        /// Digital low level.
        Low,
        /// Digital high level.
        High,
    }

    ariel_os_embassy_common::define_into_level!();

    /// Public input wrapper. Owns a type-erased [`Pin<mode::Input<...>, Dynamic>`] so that
    /// a single `Output<'a>` / `Input<'a>` type covers every concrete pin.
    pub struct Input<'a> {
        pin: embassy_avr::mcu::port::Pin<embassy_avr::hal::port::mode::Input<embassy_avr::hal::port::mode::AnyInput>, embassy_avr::mcu::port::Dynamic>,
        _marker: PhantomData<&'a ()>,
    }

    impl<'a> Input<'a> {
        /// Check whether the input is driven high.
        #[must_use]
        pub fn is_high(&self) -> bool {
            self.pin.is_high()
        }

        /// Check whether the input is driven low.
        #[must_use]
        pub fn is_low(&self) -> bool {
            self.pin.is_low()
        }

        /// Returns the input level as a HAL-specific enum value.
        #[must_use]
        pub fn get_level(&self) -> Level {
            if self.is_high() { Level::High } else { Level::Low }
        }

        /// Wait asynchronously until the input level is high. Returns immediately if it is
        /// already high. Unimplemented on AVR (no async signal yet).
        pub async fn wait_for_high(&mut self) {
            let _ = &self.pin;
        }

        /// Wait asynchronously until the input level is low. Unimplemented on AVR.
        pub async fn wait_for_low(&mut self) {
            let _ = &self.pin;
        }

        /// Wait asynchronously for a low-to-high transition. Unimplemented on AVR.
        pub async fn wait_for_rising_edge(&mut self) {
            let _ = &self.pin;
        }

        /// Wait asynchronously for a high-to-low transition. Unimplemented on AVR.
        pub async fn wait_for_falling_edge(&mut self) {
            let _ = &self.pin;
        }

        /// Wait asynchronously for any transition. Unimplemented on AVR.
        pub async fn wait_for_any_edge(&mut self) {
            let _ = &self.pin;
        }
    }

    impl embedded_hal::digital::ErrorType for Input<'_> {
        type Error = core::convert::Infallible;
    }

    impl embedded_hal::digital::InputPin for Input<'_> {
        fn is_high(&mut self) -> Result<bool, Self::Error> {
            Ok(Input::is_high(self))
        }

        fn is_low(&mut self) -> Result<bool, Self::Error> {
            Ok(Input::is_low(self))
        }
    }

    // Interrupt-enabled input. AVR does not yet have async GPIO interrupts wired up here,
    // but the HAL-generic surface expects this type when `external-interrupts` is on.
    #[cfg(feature = "external-interrupts")]
    pub struct IntEnabledInput<'a> {
        pin: embassy_avr::mcu::port::Pin<embassy_avr::hal::port::mode::Input<embassy_avr::hal::port::mode::AnyInput>, embassy_avr::mcu::port::Dynamic>,
        _marker: PhantomData<&'a ()>,
    }

    #[cfg(feature = "external-interrupts")]
    impl IntEnabledInput<'_> {
        #[must_use]
        pub fn is_high(&self) -> bool { self.pin.is_high() }
        #[must_use]
        pub fn is_low(&self) -> bool { self.pin.is_low() }
        #[must_use]
        pub fn get_level(&self) -> Level {
            if self.is_high() { Level::High } else { Level::Low }
        }

        pub async fn wait_for_high(&mut self) { let _ = &self.pin; }
        pub async fn wait_for_low(&mut self) { let _ = &self.pin; }
        pub async fn wait_for_rising_edge(&mut self) { let _ = &self.pin; }
        pub async fn wait_for_falling_edge(&mut self) { let _ = &self.pin; }
        pub async fn wait_for_any_edge(&mut self) { let _ = &self.pin; }
    }

    #[cfg(feature = "external-interrupts")]
    impl embedded_hal::digital::ErrorType for IntEnabledInput<'_> {
        type Error = core::convert::Infallible;
    }

    #[cfg(feature = "external-interrupts")]
    impl embedded_hal::digital::InputPin for IntEnabledInput<'_> {
        fn is_high(&mut self) -> Result<bool, Self::Error> { Ok(IntEnabledInput::is_high(self)) }
        fn is_low(&mut self) -> Result<bool, Self::Error> { Ok(IntEnabledInput::is_low(self)) }
    }

    /// Construct a new [`Input`] from a HAL-owned pin.
    ///
    /// The pull configuration is applied by calling the appropriate
    /// `into_*_input()` conversion on the underlying [`Pin`].
    #[doc(hidden)]
    pub fn new<'a, P: InputPin>(
        pin: crate::peripheral::Peri<'a, P>,
        pull: Pull,
        _schmitt_trigger: bool,
    ) -> Result<Input<'a>, ariel_os_embassy_common::gpio::input::Error> {
        let raw = pin.into_inner();
        let pin = match pull {
            Pull::None => unsafe {
                core::mem::transmute::<
                    embassy_avr::mcu::port::Pin<embassy_avr::hal::port::mode::Input<embassy_avr::hal::port::mode::Floating>, embassy_avr::mcu::port::Dynamic>,
                    embassy_avr::mcu::port::Pin<embassy_avr::hal::port::mode::Input<embassy_avr::hal::port::mode::AnyInput>, embassy_avr::mcu::port::Dynamic>
                >(embassy_avr::hal::port::Pin::new(raw).into_floating_input().downgrade())
            },
            Pull::Up => unsafe {
                core::mem::transmute::<
                    embassy_avr::mcu::port::Pin<embassy_avr::hal::port::mode::Input<embassy_avr::hal::port::mode::PullUp>, embassy_avr::mcu::port::Dynamic>,
                    embassy_avr::mcu::port::Pin<embassy_avr::hal::port::mode::Input<embassy_avr::hal::port::mode::AnyInput>, embassy_avr::mcu::port::Dynamic>
                >(embassy_avr::hal::port::Pin::new(raw).into_pull_up_input().downgrade())
            },
            Pull::Down => {
                // AVR pins only support pull-up; fall back to floating for pull-down requests.
                unsafe {
                    core::mem::transmute::<
                        embassy_avr::mcu::port::Pin<embassy_avr::hal::port::mode::Input<embassy_avr::hal::port::mode::Floating>, embassy_avr::mcu::port::Dynamic>,
                        embassy_avr::mcu::port::Pin<embassy_avr::hal::port::mode::Input<embassy_avr::hal::port::mode::AnyInput>, embassy_avr::mcu::port::Dynamic>
                    >(embassy_avr::hal::port::Pin::new(raw).into_floating_input().downgrade())
                }
            }
        };

        Ok(Input { pin, _marker: PhantomData })
    }

    #[cfg(feature = "external-interrupts")]
    #[doc(hidden)]
    pub fn new_int_enabled<'a, P: InputPin>(
        pin: crate::peripheral::Peri<'a, P>,
        pull: Pull,
        schmitt_trigger: bool,
    ) -> Result<IntEnabledInput<'a>, ariel_os_embassy_common::gpio::input::Error> {
        // Interrupt registration is not yet wired up for AVR; reuse the plain input.
        let input = new(pin, pull, schmitt_trigger)?;
        Ok(IntEnabledInput { pin: input.pin, _marker: PhantomData })
    }
}

pub mod output {
    //! Output-specific types.

    use core::marker::PhantomData;
    use embedded_hal::digital::StatefulOutputPin;

    use ariel_os_embassy_common::gpio::Level;

    /// AVR's pin marker types (PB5, etc.) implement `PinOps`, which is the trait our HAL pin
    /// wrappers use to provide `OutputPin`/`InputPin`. We re-export it here as the
    /// HAL-generic `OutputPin` trait expected by [`ariel_os_hal::gpio`].
    #[doc(hidden)]
    pub trait OutputPin: embassy_avr::hal::port::PinOps<Dynamic = embassy_avr::mcu::port::Dynamic> {}

    impl<T: embassy_avr::hal::port::PinOps<Dynamic = embassy_avr::mcu::port::Dynamic>> OutputPin for T {}

    /// Whether outputs support configuring drive strength.
    ///
    /// ATmega328P outputs do not have configurable drive strength.
    pub const DRIVE_STRENGTH_CONFIGURABLE: bool = false;
    /// Whether outputs support configuring speed/slew rate.
    ///
    /// ATmega328P outputs do not have configurable slew rate.
    pub const SPEED_CONFIGURABLE: bool = false;

    /// Public output wrapper. Owns a type-erased `Pin<Output, Dynamic>` so that a single
    /// `Output<'a>` type covers every concrete pin.
    pub struct Output<'a> {
        pin: embassy_avr::mcu::port::Pin<
            embassy_avr::hal::port::mode::Output,
            embassy_avr::mcu::port::Dynamic,
        >,
        _marker: PhantomData<&'a ()>,
    }

    impl Output<'_> {
        /// Set the output high.
        pub fn set_high(&mut self) {
            self.pin.set_high();
        }

        /// Set the output low.
        pub fn set_low(&mut self) {
            self.pin.set_low();
        }

        /// Check whether the output is currently driven high.
        pub fn is_set_high(&self) -> bool {
            self.pin.is_set_high()
        }

        /// Check whether the output is currently driven low.
        pub fn is_set_low(&self) -> bool {
            self.pin.is_set_low()
        }

        /// Toggle the output level.
        pub fn toggle(&mut self) {
            self.pin.toggle();
        }
    }

    impl embedded_hal::digital::ErrorType for Output<'_> {
        type Error = core::convert::Infallible;
    }

    impl embedded_hal::digital::OutputPin for Output<'_> {
        fn set_high(&mut self) -> Result<(), Self::Error> {
            Output::set_high(self);
            Ok(())
        }

        fn set_low(&mut self) -> Result<(), Self::Error> {
            Output::set_low(self);
            Ok(())
        }
    }

    impl StatefulOutputPin for Output<'_> {
        fn is_set_high(&mut self) -> Result<bool, Self::Error> {
            Ok(Output::is_set_high(self))
        }

        fn is_set_low(&mut self) -> Result<bool, Self::Error> {
            Ok(Output::is_set_low(self))
        }
    }

    /// Construct a new [`Output`] from a HAL-owned pin.
    ///
    /// `drive_strength` and `speed` are accepted but ignored: ATmega328P has no configurable
    /// drive strength or slew rate. `initial_level` selects `into_output()` (low) or
    /// `into_output_high()` (high).
    #[doc(hidden)]
    #[allow(clippy::needless_pass_by_value)]
    pub fn new<'a, P: OutputPin>(
        pin: crate::peripheral::Peri<'a, P>,
        initial_level: Level,
        _drive_strength: super::DriveStrength,
        _speed: super::Speed,
    ) -> Output<'a> {
        let raw = pin.into_inner();
        let pin = embassy_avr::hal::port::Pin::new(raw);
        let pin = match initial_level {
            Level::High => pin.into_output_high().downgrade(),
            Level::Low => pin.into_output().downgrade(),
        };

        Output { pin, _marker: PhantomData }
    }
}

/// Available drive strength settings.
///
/// *Note: ATmega328P outputs do not have configurable drive strength.*
#[derive(Copy, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum DriveStrength {
    /// Drive strength is fixed by the hardware.
    #[default]
    Fixed,
}

impl ariel_os_embassy_common::gpio::FromDriveStrength for DriveStrength {
    fn from(_drive_strength: ariel_os_embassy_common::gpio::DriveStrength<Self>) -> Self {
        Self::Fixed
    }
}

/// Available output speed/slew rate settings.
///
/// *Note: ATmega328P outputs do not have configurable slew rate.*
#[derive(Copy, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Speed {
    /// Slew rate is fixed by the hardware.
    #[default]
    Fixed,
}

impl ariel_os_embassy_common::gpio::FromSpeed for Speed {
    fn from(_speed: ariel_os_embassy_common::gpio::Speed<Self>) -> Self {
        Self::Fixed
    }
}
