#![deny(missing_docs)]

#[cfg(feature = "arduino-uno")]
#[doc(hidden)]
#[allow(non_snake_case)]
pub struct OptionalPeripherals {
    pub PB5: Option<Peri<'static, crate::peripherals::PB5>>,
    pub PD7: Option<Peri<'static, crate::peripherals::PD7>>,
    pub PC0: Option<Peri<'static, crate::peripherals::PC0>>,
}

#[cfg(not(feature = "arduino-uno"))]
#[doc(hidden)]
pub struct OptionalPeripherals;

#[doc(hidden)]
pub trait IntoPeripheral<'a, P>: private::Sealed {
    #[must_use]
    fn into_hal_peripheral(self) -> Peri<'a, P>;
}

#[doc(hidden)]
pub struct Peripheral;

impl<'a, T> private::Sealed for Peri<'a, T> {}

impl<'a, T> IntoPeripheral<'a, T> for Peri<'a, T> {
    fn into_hal_peripheral(self) -> Peri<'a, T> {
        self
    }
}

#[doc(hidden)]
pub struct Peripherals;

#[cfg(feature = "arduino-uno")]
impl From<Peripherals> for OptionalPeripherals {
    fn from(_peripherals: Peripherals) -> Self {
        Self {
            PB5: None,
            PD7: None,
            PC0: None,
        }
    }
}

#[cfg(not(feature = "arduino-uno"))]
impl From<Peripherals> for OptionalPeripherals {
    fn from(_peripherals: Peripherals) -> Self {
        Self {}
    }
}

/// Embassy-style peripheral wrapper.
#[doc(hidden)]
pub struct Peri<'a, T> {
    inner: T,
    _phantom: core::marker::PhantomData<&'a mut T>,
}

impl<'a, T> Peri<'a, T> {
    /// Create a new Peri from a peripheral.
    #[doc(hidden)]
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            _phantom: core::marker::PhantomData,
        }
    }

    /// Consume the wrapper and return the inner peripheral value.
    ///
    /// Used by HAL drivers (e.g. GPIO) to extract the raw AVR pin marker before
    /// wrapping it in an avr-hal `Pin`.
    #[doc(hidden)]
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<'a, T> core::ops::Deref for Peri<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<'a, T> core::ops::DerefMut for Peri<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

mod private {
    pub trait Sealed {}
}
