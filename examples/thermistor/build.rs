//! Generates the lookup table that converts ADC readings of the NTC thermistor into temperatures.

use std::{env, fmt::Write as _, fs, path::Path};

/// Nominal resistance of the thermistor, in ohms, at [`T_NOMINAL_C`].
const R_NOMINAL_OHMS: f64 = 100_000.0;
/// Temperature at which the thermistor has its nominal resistance, in °C.
const T_NOMINAL_C: f64 = 25.0;
/// Beta coefficient of the thermistor, in kelvins.
const BETA_K: f64 = 3950.0;
/// Pull-up resistor between 5 V and the ADC pin, in ohms.
const R_PULLUP_OHMS: u32 = 4_700;
/// Resistance of the `ATmega328P` internal pull-up, in ohms, used with the `internal-pull-up`
/// feature.
///
/// The datasheet only guarantees 20–50 kΩ. This value was measured on one Arduino Uno against a
/// 19.9 °C reference; calibrate it again for another board.
const R_INTERNAL_PULLUP_OHMS: u32 = 35_687;
/// Number of 10-bit ADC readings summed per measurement (at most 64, so that the sum fits a `u16`).
const OVERSAMPLING: u16 = 16;
/// Lowest temperature covered by the table, in °C.
const TABLE_MIN_C: u16 = 0;
/// Highest temperature covered by the table, in °C.
const TABLE_MAX_C: u16 = 300;
/// Temperature step between table entries, in °C.
const TABLE_STEP_C: usize = 10;

fn main() {
    const { assert!(OVERSAMPLING <= 64, "the sum of ADC readings must fit a u16") };

    let r_pullup_ohms = if env::var_os("CARGO_FEATURE_INTERNAL_PULL_UP").is_some() {
        R_INTERNAL_PULLUP_OHMS
    } else {
        R_PULLUP_OHMS
    };
    let full_scale = u32::from(OVERSAMPLING) * 1024;
    let t_nominal_k = T_NOMINAL_C + 273.15;

    let mut code = String::new();
    writeln!(code, "pub const R_PULLUP_OHMS: u32 = {r_pullup_ohms};").unwrap();
    writeln!(code, "pub const OVERSAMPLING: u16 = {OVERSAMPLING};").unwrap();
    writeln!(code, "pub const FULL_SCALE: u32 = {full_scale};").unwrap();
    code.push_str("pub const TABLE: &[(u16, u16)] = &[\n");
    // Hottest entry first, so that the ADC sums are in ascending order.
    for t_c in (TABLE_MIN_C..=TABLE_MAX_C).rev().step_by(TABLE_STEP_C) {
        let t_k = f64::from(t_c) + 273.15;
        let r_ohms = R_NOMINAL_OHMS * (BETA_K * (1.0 / t_k - 1.0 / t_nominal_k)).exp();
        let divider = r_ohms / (r_ohms + f64::from(r_pullup_ohms));
        // ADC code `n` covers inputs around `n + 0.5`, hence the half-code offset per reading.
        let adc_sum = divider * f64::from(full_scale) - f64::from(OVERSAMPLING) / 2.0;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "0 <= adc_sum < 65536"
        )]
        let adc_sum = adc_sum.round() as u16;
        writeln!(code, "    ({adc_sum}, {}),", t_c * 10).unwrap();
    }
    code.push_str("];\n");

    let out_dir = env::var_os("OUT_DIR").unwrap();
    fs::write(Path::new(&out_dir).join("ntc_table.rs"), code).unwrap();
    println!("cargo::rerun-if-changed=build.rs");
}
