//! Reads an NTC 100 kΩ, B = 3950 K thermistor (as used in 3D-printer hotends) on an Arduino Uno.
//!
//! Wiring (voltage divider on `A0`):
//!
//! ```text
//! 5V ── 4.7 kΩ ──┬── A0
//!                ├── NTC 100 kΩ ── GND
//!                └── 100 nF ────── GND
//! ```
//!
//! With the `internal-pull-up` feature (laze module of the same name), the `ATmega328P` internal
//! pull-up replaces the 4.7 kΩ resistor: connect the NTC between `A0` and `GND` only.
//!
//! Every 500 ms, prints the average ADC reading, the thermistor resistance, and the temperature
//! on the USB serial port (57600 baud).

#![no_main]
#![no_std]

mod ntc;

use embassy_avr::board::{self as arduino_hal, adc::AdcSettings, prelude::*};

/// ADC channel 0 (`A0`, `PC0`), sampled while the pin is configured as a pull-up input.
///
/// `Pin::into_analog_input()` disables the pull-up of the pin, so, to keep it enabled, the pin
/// stays a pull-up input and the ADC reads it through this channel instead.
#[cfg(feature = "internal-pull-up")]
struct A0Channel;

#[cfg(feature = "internal-pull-up")]
impl arduino_hal::adc::AdcChannel<embassy_avr::mcu::Atmega, embassy_avr::mcu::pac::ADC>
    for A0Channel
{
    fn channel(&self) -> embassy_avr::mcu::pac::adc::admux::MUX_A {
        embassy_avr::mcu::pac::adc::admux::MUX_A::ADC0
    }
}

#[ariel_os::task(autostart)]
// `allow` rather than `expect`: the task macros duplicate the function, and the lint only fires on
// one of the copies.
#[allow(
    clippy::unused_async,
    reason = "busy-waits until `embassy_time::Timer` fires reliably on AVR"
)]
async fn main() {
    // Ariel OS does not provide an ADC API on AVR yet, so this uses `avr-hal` (re-exported by
    // `embassy-avr`) directly; nothing else in the system takes these peripherals.
    let dp = arduino_hal::Peripherals::take().unwrap();
    let pins = arduino_hal::pins!(dp);
    let mut serial = arduino_hal::default_serial!(dp, pins, 57600);
    // AVcc (5 V) reference: the reading is ratiometric, so supply variations cancel out.
    let mut adc = arduino_hal::Adc::new(dp.ADC, AdcSettings::default());

    #[cfg(not(feature = "internal-pull-up"))]
    let thermistor = pins.a0.into_analog_input(&mut adc);
    // Keeps ownership of the pin, configured as a pull-up input, while its channel is sampled.
    #[cfg(feature = "internal-pull-up")]
    let _a0 = pins.a0.into_pull_up_input();
    #[cfg(feature = "internal-pull-up")]
    let thermistor = A0Channel;

    loop {
        // Summing several readings averages out noise and adds resolution.
        let adc_sum: u16 = (0..ntc::OVERSAMPLING)
            .map(|_| adc.read_blocking(&thermistor))
            .sum();

        ufmt::uwrite!(&mut serial, "adc={} ", adc_sum / ntc::OVERSAMPLING).unwrap_infallible();
        match ntc::resistance_ohms(adc_sum) {
            Some(ohms) => ufmt::uwrite!(&mut serial, "R={}ohm ", ohms).unwrap_infallible(),
            None => ufmt::uwrite!(&mut serial, "R=open ").unwrap_infallible(),
        }
        match ntc::temperature_decicelsius(adc_sum) {
            Some(t) => {
                ufmt::uwriteln!(&mut serial, "T={}.{}C\r", t / 10, t % 10).unwrap_infallible();
            }
            None => {
                ufmt::uwriteln!(&mut serial, "T=out of range (disconnected or shorted?)\r")
                    .unwrap_infallible();
            }
        }

        // `embassy_time::Timer` does not fire reliably on AVR yet: busy-wait instead.
        arduino_hal::delay_ms(500);
    }
}
