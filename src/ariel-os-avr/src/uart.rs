//! Provides the UART driver for USART0.

#![expect(unsafe_code)]

use portable_atomic::{AtomicBool, Ordering};

use ariel_os_embassy_common::{impl_async_uart_for_driver_enum, uart::ConfigError};

use embassy_avr::{
    avr_device::atmega328p,
    mcu::port::{PD0, PD1},
    uart::BufferedUart,
};

use crate::IntoPeripheral;

/// UART interface configuration.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub struct Config {
    /// The baud rate at which UART should operate.
    pub baudrate: ariel_os_embassy_common::uart::Baudrate<Baudrate>,
    /// Number of data bits.
    pub data_bits: DataBits,
    /// Number of stop bits.
    pub stop_bits: StopBits,
    /// Parity mode used for the interface.
    pub parity: Parity,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            baudrate: ariel_os_embassy_common::uart::Baudrate::_115200,
            data_bits: DataBits::Data8,
            stop_bits: StopBits::Stop1,
            parity: Parity::None,
        }
    }
}

/// UART baud rate.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Baudrate {
    /// The baud rate at which UART should operate.
    baud: u32,
}

impl From<Baudrate> for u32 {
    fn from(baud: Baudrate) -> u32 {
        baud.baud
    }
}

impl From<u32> for Baudrate {
    fn from(baudrate: u32) -> Baudrate {
        Baudrate { baud: baudrate }
    }
}

impl From<ariel_os_embassy_common::uart::Baudrate<Self>> for Baudrate {
    fn from(baud: ariel_os_embassy_common::uart::Baudrate<Self>) -> Baudrate {
        match baud {
            ariel_os_embassy_common::uart::Baudrate::Hal(baud) => baud,
            ariel_os_embassy_common::uart::Baudrate::_2400 => Baudrate { baud: 2400 },
            ariel_os_embassy_common::uart::Baudrate::_4800 => Baudrate { baud: 4800 },
            ariel_os_embassy_common::uart::Baudrate::_9600 => Baudrate { baud: 9600 },
            ariel_os_embassy_common::uart::Baudrate::_19200 => Baudrate { baud: 19_200 },
            ariel_os_embassy_common::uart::Baudrate::_38400 => Baudrate { baud: 38_400 },
            ariel_os_embassy_common::uart::Baudrate::_57600 => Baudrate { baud: 57_600 },
            ariel_os_embassy_common::uart::Baudrate::_115200 => Baudrate { baud: 115_200 },
        }
    }
}

/// UART number of data bits.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum DataBits {
    /// 5 bits per character.
    Data5,
    /// 6 bits per character.
    Data6,
    /// 7 bits per character.
    Data7,
    /// 8 bits per character.
    Data8,
}

fn from_data_bits(databits: DataBits) -> embassy_avr::uart::DataBits {
    match databits {
        DataBits::Data5 => embassy_avr::uart::DataBits::DataBits5,
        DataBits::Data6 => embassy_avr::uart::DataBits::DataBits6,
        DataBits::Data7 => embassy_avr::uart::DataBits::DataBits7,
        DataBits::Data8 => embassy_avr::uart::DataBits::DataBits8,
    }
}

impl From<ariel_os_embassy_common::uart::DataBits<Self>> for DataBits {
    fn from(databits: ariel_os_embassy_common::uart::DataBits<Self>) -> DataBits {
        match databits {
            ariel_os_embassy_common::uart::DataBits::Hal(bits) => bits,
            ariel_os_embassy_common::uart::DataBits::Data8 => DataBits::Data8,
        }
    }
}

/// Parity bit.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Parity {
    /// No parity bit.
    None,
    /// Even parity bit.
    Even,
    /// Odd parity bit.
    Odd,
}

fn from_parity(parity: Parity) -> embassy_avr::uart::Parity {
    match parity {
        Parity::None => embassy_avr::uart::Parity::ParityNone,
        Parity::Even => embassy_avr::uart::Parity::ParityEven,
        Parity::Odd => embassy_avr::uart::Parity::ParityOdd,
    }
}

impl From<ariel_os_embassy_common::uart::Parity<Self>> for Parity {
    fn from(parity: ariel_os_embassy_common::uart::Parity<Self>) -> Self {
        match parity {
            ariel_os_embassy_common::uart::Parity::Hal(parity) => parity,
            ariel_os_embassy_common::uart::Parity::None => Self::None,
            ariel_os_embassy_common::uart::Parity::Even => Self::Even,
        }
    }
}

/// UART number of stop bits.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum StopBits {
    /// One stop bit.
    Stop1,
    /// Two stop bits.
    Stop2,
}

fn from_stop_bits(stop_bits: StopBits) -> embassy_avr::uart::StopBits {
    match stop_bits {
        StopBits::Stop1 => embassy_avr::uart::StopBits::STOP1,
        StopBits::Stop2 => embassy_avr::uart::StopBits::STOP2,
    }
}

impl From<ariel_os_embassy_common::uart::StopBits<Self>> for StopBits {
    fn from(stopbits: ariel_os_embassy_common::uart::StopBits<Self>) -> Self {
        match stopbits {
            ariel_os_embassy_common::uart::StopBits::Hal(stopbits) => stopbits,
            ariel_os_embassy_common::uart::StopBits::Stop1 => StopBits::Stop1,
        }
    }
}

/// Peripheral-specific UART driver.
///
/// USART0 uses `PD0` as RX and `PD1` as TX. Interrupts move the data in both directions, so it
/// stays buffered while the application is busy. Each buffer holds one byte less than its
/// length.
pub struct USART0<'d> {
    uart: BufferedUart<'d>,
}

// Make this struct a compile-time-enforced singleton: having multiple statics
// defined with the same name would result in a compile-time error.
#[allow(dead_code)]
static PREVENT_MULTIPLE_USART0: () = ();

// Ensure this peripheral has only one active instance.
static ACTIVE_USART0: AtomicBool = AtomicBool::new(false);

impl<'d> USART0<'d> {
    /// Returns a driver implementing embedded-io traits for this UART peripheral.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::BaudrateNotSupported`] when the 16 MHz clock cannot generate the
    /// baud rate within 5 %, and [`ConfigError::ConfigurationNotSupported`] when a buffer is
    /// shorter than 2 bytes.
    ///
    /// # Panics
    ///
    /// Panics if a USART0 driver already exists.
    #[expect(clippy::new_ret_no_self)]
    pub fn new(
        rx_pin: impl IntoPeripheral<'d, PD0>,
        tx_pin: impl IntoPeripheral<'d, PD1>,
        rx_buf: &'d mut [u8],
        tx_buf: &'d mut [u8],
        config: Config,
    ) -> Result<Uart<'d>, ConfigError> {
        let mut uart_config = embassy_avr::uart::Config::default();
        uart_config.baudrate = Baudrate::from(config.baudrate).into();
        uart_config.data_bits = from_data_bits(config.data_bits);
        uart_config.stop_bits = from_stop_bits(config.stop_bits);
        uart_config.parity = from_parity(config.parity);

        // Check if we can initialize this peripheral (check if the value was previously false,
        // set it to true).
        assert!(
            !ACTIVE_USART0.swap(true, Ordering::AcqRel),
            "UART peripheral already initialized"
        );

        // FIXME(safety): `avr_device::atmega328p::Peripherals::take()` still hands out USART0.
        // SAFETY: `ACTIVE_USART0` ensures that only one driver uses USART0 at a time.
        let peripheral = unsafe { atmega328p::USART0::steal() };

        let uart = BufferedUart::new(
            peripheral,
            rx_pin.into_hal_peripheral().into_inner(),
            tx_pin.into_hal_peripheral().into_inner(),
            rx_buf,
            tx_buf,
            uart_config,
        )
        .map_err(|error| {
            ACTIVE_USART0.store(false, Ordering::Release);
            match error {
                embassy_avr::uart::ConfigError::BaudrateNotSupported => {
                    ConfigError::BaudrateNotSupported
                }
                _ => ConfigError::ConfigurationNotSupported,
            }
        })?;

        Ok(Uart::USART0(Self { uart }))
    }
}

impl Drop for USART0<'_> {
    fn drop(&mut self) {
        ACTIVE_USART0.store(false, Ordering::Release);
    }
}

/// Peripheral-agnostic UART driver.
pub enum Uart<'d> {
    /// USART0 peripheral.
    USART0(USART0<'d>),
}

impl embedded_io_async::ErrorType for Uart<'_> {
    type Error = embassy_avr::uart::Error;
}

impl_async_uart_for_driver_enum!(Uart, USART0);

#[doc(hidden)]
pub fn init(_peripherals: &mut crate::OptionalPeripherals) {
    // USART0 is not part of the peripherals: `ACTIVE_USART0` guards it instead.
}
