//! Pins used by the example.

// Relay input on `D7`, status LED `L` on `D13`, thermistor on `A0`.
#[cfg(context = "arduino-uno-board")]
ariel_os::hal::define_peripherals!(HeaterPeripherals {
    relay: PD7,
    status_led: PB5,
    thermistor: PC0,
});
