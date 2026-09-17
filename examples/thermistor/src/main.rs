//! Controls a 3D-printer hotend with an Arduino Uno and a relay, with no printer involved.
//!
//! An NTC 100 kΩ, B = 3950 K thermistor on `A0` measures the temperature, and a 5 V relay module on
//! `D7` switches the heater. The target temperature is set over USB serial.
//!
//! Control works like Marlin with `SLOW_PWM_HEATERS`: every 250 ms a PID computes the heater
//! output (0–255), and the relay stays on for that fraction of each 4 s window, so it switches at
//! most once per window. Marlin's thermal protections stop the heater when the thermistor fails,
//! from 275 °C on, when the temperature does not rise while heating, and when it falls below the
//! target and stays there.
//!
//! Wiring, with the `ATmega328P` internal pull-up on `A0` (laze module `internal-pull-up`, selected
//! by default):
//!
//! ```text
//! A0 ── NTC 100 kΩ ── GND
//!
//! 5V ── VCC, GND ── GND, D7 ── IN        (relay module)
//! heater supply + ── COM, NO ── heater ── heater supply −
//! ```
//!
//! With `-d internal-pull-up`, the NTC forms a divider with an external resistor instead:
//! `5V ── 4.7 kΩ ── A0 ── NTC 100 kΩ ── GND`. The `D13` LED mirrors the relay.
//!
//! Commands, one per line, over USB serial at 57600 baud:
//!
//! - `M104 S<°C>` sets the target temperature; `M104 S0` turns the heater off.
//! - `M105` reports `ok T:<temperature> /<target> @:<output>`.
//! - `M155 S<seconds>` sets the interval of automatic reports (1 s at boot); `S0` disables them.
//! - `M301 P<kp> I<ki> D<kd>` sets PID gains; `M301` alone reports them.
//! - `M305 R<ohms>` sets the pull-up. `M305 K<ohms>` calibrates it, with the heater off, from the
//!   known resistance at the thermistor's place: a resistor, or the thermistor at a known
//!   temperature. `M305` alone reports the pull-up and the measured resistance.
//! - `M500` saves the target, the gains, and the pull-up. The saved target applies at boot, even
//!   without USB.
//! - `M112` stops the heater until `M999`.

#![no_main]
#![no_std]

mod gcode;
mod heater;
mod ntc;
mod pins;
mod settings;

use core::convert::Infallible;

use ariel_os::{
    gpio::{Input, Level, Output, Pull},
    time::{Duration, Instant, Timer},
};
use embassy_avr::{
    adc::Adc,
    avr_device,
    mcu::{Eeprom, Peripherals, Wdt, wdt::Timeout},
    serial::Serial,
};
use ufmt::uWrite;

use crate::{
    gcode::Command,
    heater::{Controller, Decicelsius, Fault, Gains, SlowPwm},
    settings::{MIN_PULLUP_OHMS, Settings},
};

ariel_os::hal::group_peripherals!(Pins {
    heater: pins::HeaterPeripherals,
});

/// Whether the relay module switches on with a high input, like most modules without an
/// optocoupler. Set to `false` if the relay switches on at boot.
const RELAY_ACTIVE_HIGH: bool = true;
/// Period of the main loop.
const TICK: Duration = Duration::from_millis(10);
/// Interval between automatic reports at boot, in seconds.
const DEFAULT_REPORT_INTERVAL_S: u64 = 1;
/// Longest accepted interval between automatic reports, in seconds.
const MAX_REPORT_INTERVAL_S: i32 = 60;
/// Longest accepted command line, in bytes.
const LINE_LEN: usize = 48;
/// EEPROM offset of the settings.
const SETTINGS_OFFSET: u16 = 0;
/// Time to wait after writing an EEPROM byte, in milliseconds (a write takes 3.4 ms).
const EEPROM_WRITE_MS: u64 = 5;
/// Number of measurements averaged by `M305 K`.
const CALIBRATION_MEASUREMENTS: u32 = 8;
/// Reply to a parameter outside of its range. Strings take RAM on AVR, so commands share it.
const OUT_OF_RANGE: &str = "echo:value out of range";

/// Writes formatted text with [`ufmt`], to a writer that cannot fail.
macro_rules! out {
    ($($tt:tt)*) => {{
        let Ok(()) = ufmt::uwrite!($($tt)*);
    }};
}

#[ariel_os::task(autostart, peripherals)]
async fn main(peripherals: Pins) {
    let pins = peripherals.heater;
    let relay = Relay {
        pin: Output::new(pins.relay, relay_level(false)),
        led: Output::new(pins.status_led, Level::Low),
        on: false,
    };
    // Keeps the thermistor pin an input, with the pull-up if requested, while the ADC samples it.
    let pull = if cfg!(feature = "internal-pull-up") {
        Pull::Up
    } else {
        Pull::None
    };
    let _thermistor = Input::new(pins.thermistor, pull);
    let adc = Adc::init();
    let serial = Serial::init();

    // Ariel OS provides no EEPROM or watchdog API on AVR yet, so this uses `avr-hal` directly;
    // nothing else in the system takes these peripherals.
    let dp = Peripherals::take().unwrap();
    let eeprom = Eeprom::new(dp.EEPROM);
    // Resets the MCU if the loop stalls. The relay pin then floats, which switches the relay off.
    let mut watchdog = Wdt::new(dp.WDT, dp.CPU.mcusr());
    watchdog.start(Timeout::Ms2000).unwrap();

    let settings = load_settings(&eeprom).unwrap_or(Settings::DEFAULT);
    let mut app = App {
        serial,
        relay,
        adc,
        eeprom,
        controller: Controller::new(settings.gains),
        pwm: SlowPwm::new(),
        line: LineBuffer::new(),
        report_interval: Duration::from_secs(DEFAULT_REPORT_INTERVAL_S),
        next_report: Instant::now(),
        pullup_ohms: settings.pullup_ohms,
        adc_sum: 0,
        reported_fault: None,
    };
    app.controller.set_target(settings.target);

    // Hosts such as Pronterface wait for Marlin's `start` greeting.
    app.line_out("start");
    // Strings take RAM on AVR, so messages are terse.
    app.line_out("echo:M104 S<C> M105 M155 S<s> M301 P I D M305 R K M500 M112 M999");
    if settings.target > 0 {
        app.write("echo:saved target ");
        app.write_fixed(settings.target, 1);
        app.line_out("");
    }

    let mut next_tick = Instant::now();
    let mut next_sample = next_tick;
    loop {
        watchdog.feed();
        app.run_commands().await;

        let now = Instant::now();
        if now >= next_sample {
            next_sample = next_after(next_sample, sample_period(), now);
            app.sample(now);
        }
        app.check_fault();
        app.drive_relay(now);
        if app.report_interval.as_ticks() > 0 && now >= app.next_report {
            app.next_report = next_after(app.next_report, app.report_interval, now);
            app.report(" ");
        }

        next_tick = next_after(next_tick, TICK, Instant::now());
        Timer::at(next_tick).await;
    }
}

/// State of the controller application.
struct App<'a> {
    serial: Serial,
    relay: Relay<'a>,
    adc: Adc,
    eeprom: Eeprom,
    controller: Controller,
    pwm: SlowPwm,
    line: LineBuffer,
    report_interval: Duration,
    next_report: Instant,
    /// Thermistor pull-up, in ohms.
    pullup_ohms: u16,
    /// Latest sum of ADC readings.
    adc_sum: u16,
    /// Fault last reported on the console.
    reported_fault: Option<Fault>,
}

impl App<'_> {
    /// Handles the command lines received so far.
    async fn run_commands(&mut self) {
        while let Some(byte) = self.serial.try_read() {
            if let Some(command) = self.line.push(byte) {
                self.handle(command).await;
            }
        }
    }

    /// Handles one command, and acknowledges it like Marlin does.
    async fn handle(&mut self, command: Command) {
        match command {
            Command::Empty => return,
            Command::SetTarget(Some(hundredths)) => {
                if !self.controller.set_target(hundredths / 10) {
                    self.line_out("echo:fault active, send M999");
                }
            }
            Command::Report => {
                self.report("ok ");
                return;
            }
            Command::Kill => {
                self.controller.kill();
                self.relay.set(false);
            }
            Command::AutoReport(Some(hundredths)) => {
                let seconds = (hundredths / 100).clamp(0, MAX_REPORT_INTERVAL_S);
                self.report_interval = Duration::from_secs(u64::try_from(seconds).unwrap_or(0));
                self.next_report = Instant::now() + self.report_interval;
            }
            Command::Pid {
                p: None,
                i: None,
                d: None,
            } => {
                let Gains { kp, ki, kd } = self.controller.gains();
                self.write("echo: p:");
                self.write_fixed(kp.into(), 2);
                self.write(" i:");
                self.write_fixed(ki.into(), 2);
                self.write(" d:");
                self.write_fixed(kd.into(), 2);
                self.line_out("");
            }
            Command::Pid { p, i, d } => {
                let gains = self.controller.gains();
                let gain = |value: Option<i32>, current| {
                    value.map_or(Some(current), |v| u16::try_from(v).ok())
                };
                if let (Some(kp), Some(ki), Some(kd)) =
                    (gain(p, gains.kp), gain(i, gains.ki), gain(d, gains.kd))
                {
                    self.controller.set_gains(Gains { kp, ki, kd });
                } else {
                    self.line_out(OUT_OF_RANGE);
                }
            }
            Command::Thermistor { r: None, k: None } => self.report_thermistor(),
            Command::Thermistor {
                r: Some(hundredths),
                k: None,
            } => match u16::try_from(hundredths / 100) {
                Ok(ohms) if ohms >= MIN_PULLUP_OHMS => self.pullup_ohms = ohms,
                _ => self.line_out(OUT_OF_RANGE),
            },
            Command::Thermistor {
                k: Some(hundredths),
                ..
            } => self.calibrate(hundredths / 100),
            Command::Save => self.save().await,
            Command::ClearFault => self.controller.clear_fault(),
            Command::Unknown => self.line_out("echo:Unknown command"),
            Command::SetTarget(None) | Command::AutoReport(None) => {}
        }
        self.line_out("ok");
    }

    /// Reads the thermistor and updates the heater output.
    fn sample(&mut self, now: Instant) {
        self.adc_sum = self.measure();
        let temperature =
            ntc::temperature_decicelsius(self.adc_sum, self.pullup_ohms).map(Decicelsius::from);
        self.controller.update(now.as_millis(), temperature);
    }

    /// Returns the sum of [`ntc::OVERSAMPLING`] ADC readings of the thermistor.
    fn measure(&mut self) -> u16 {
        // Summing several readings averages out noise and adds resolution.
        (0..ntc::OVERSAMPLING)
            .map(|_| self.adc.read_blocking())
            .sum()
    }

    /// Sets the pull-up so that the thermistor reads `known_ohms`, which the resistor at its
    /// place, or the thermistor at a known temperature, has.
    fn calibrate(&mut self, known_ohms: i32) {
        // The heater must be off: the controller sees a fake temperature meanwhile.
        if self.controller.target() > 0 {
            self.line_out("echo:send M104 S0 first");
            return;
        }
        let total: u32 = (0..CALIBRATION_MEASUREMENTS)
            .map(|_| u32::from(self.measure()))
            .sum();
        let average = (total + CALIBRATION_MEASUREMENTS / 2) / CALIBRATION_MEASUREMENTS;
        self.adc_sum = u16::try_from(average).unwrap_or(u16::MAX);
        let pullup = u32::try_from(known_ohms)
            .ok()
            .and_then(|known| ntc::pullup_ohms_for(self.adc_sum, known))
            .filter(|&ohms| ohms >= MIN_PULLUP_OHMS);
        match pullup {
            Some(ohms) => {
                self.pullup_ohms = ohms;
                self.report_thermistor();
            }
            None => self.line_out(OUT_OF_RANGE),
        }
    }

    /// Reports the pull-up and the thermistor resistance measured with it.
    fn report_thermistor(&mut self) {
        let pullup = self.pullup_ohms;
        out!(&mut self.serial_writer(), "echo: R:{} ntc:", pullup);
        match ntc::resistance_ohms(self.adc_sum, pullup) {
            Some(ohms) => out!(&mut self.serial_writer(), "{}", ohms),
            None => self.write("open"),
        }
        self.line_out("");
    }

    /// Reports a new fault once.
    fn check_fault(&mut self) {
        let fault = self.controller.fault();
        if fault != self.reported_fault {
            self.reported_fault = fault;
            if let Some(fault) = fault {
                self.report_fault(fault);
            }
        }
    }

    /// Switches the relay according to the heater output.
    fn drive_relay(&mut self, now: Instant) {
        let on = self.pwm.poll(now.as_millis(), self.controller.output());
        self.relay.set(on);
    }

    /// Saves the target, the gains, and the pull-up to EEPROM, and checks them.
    async fn save(&mut self) {
        let settings = Settings {
            target: self.controller.target(),
            gains: self.controller.gains(),
            pullup_ohms: self.pullup_ohms,
        };
        for (offset, byte) in (SETTINGS_OFFSET..).zip(settings.encode()) {
            // An interrupt between the two write-enable steps would cancel the write.
            avr_device::interrupt::free(|_| self.eeprom.write_byte(offset, byte));
            // Lets the write finish here rather than in the next critical section.
            Timer::after_millis(EEPROM_WRITE_MS).await;
        }
        if load_settings(&self.eeprom) == Some(settings) {
            self.line_out("echo:settings saved");
        } else {
            self.line_out("Error:EEPROM write failed");
        }
    }

    /// Writes a Marlin-style temperature report after `prefix`, preceded by the active fault.
    fn report(&mut self, prefix: &str) {
        // An `ok` report must be the last line of the response.
        if let Some(fault) = self.controller.fault() {
            self.report_fault(fault);
        }
        self.write(prefix);
        self.write("T:");
        match self.controller.temperature() {
            Some(temperature) => self.write_fixed(temperature, 1),
            None => self.write("---"),
        }
        self.write(" /");
        self.write_fixed(self.controller.target(), 1);
        out!(&mut self.serial_writer(), " @:{}", self.controller.output());
        self.line_out("");
    }

    fn report_fault(&mut self, fault: Fault) {
        self.write("Error:");
        self.write(fault.message());
        if fault == Fault::Sensor {
            let adc_sum = self.adc_sum;
            out!(
                &mut self.serial_writer(),
                " (adc {}/{} R=",
                adc_sum,
                ntc::FULL_SCALE
            );
            match ntc::resistance_ohms(adc_sum, self.pullup_ohms) {
                Some(ohms) => out!(&mut self.serial_writer(), "{})", ohms),
                None => self.write("open)"),
            }
        }
        self.line_out("; heater off, M999 clears");
    }

    /// Writes `value`, scaled by `10^decimals`, with exactly `decimals` decimals.
    fn write_fixed(&mut self, value: i32, decimals: u32) {
        let scale = 10_u32.pow(decimals);
        let magnitude = value.unsigned_abs();
        if value < 0 {
            self.write("-");
        }
        let fraction = magnitude % scale;
        out!(&mut self.serial_writer(), "{}.", magnitude / scale);
        let mut digit = scale / 10;
        while digit > 1 && fraction < digit {
            self.write("0");
            digit /= 10;
        }
        out!(&mut self.serial_writer(), "{}", fraction);
    }

    fn write(&mut self, s: &str) {
        self.serial.write_all(s.as_bytes());
    }

    fn line_out(&mut self, s: &str) {
        self.write(s);
        self.write("\r\n");
    }

    fn serial_writer(&mut self) -> SerialWriter<'_> {
        SerialWriter(&mut self.serial)
    }
}

/// Adapts [`Serial`] to [`ufmt`].
struct SerialWriter<'a>(&'a mut Serial);

impl uWrite for SerialWriter<'_> {
    type Error = Infallible;

    fn write_str(&mut self, s: &str) -> Result<(), Infallible> {
        self.0.write_all(s.as_bytes());
        Ok(())
    }
}

/// The relay output, mirrored on the LED.
struct Relay<'a> {
    pin: Output<'a>,
    led: Output<'a>,
    on: bool,
}

impl Relay<'_> {
    fn set(&mut self, on: bool) {
        if on != self.on {
            self.on = on;
            self.pin.set_level(relay_level(on));
            self.led.set_level(Level::from(on));
        }
    }
}

/// Returns the pin level that switches the relay on or off.
fn relay_level(on: bool) -> Level {
    Level::from(on == RELAY_ACTIVE_HIGH)
}

/// Collects received bytes into command lines.
struct LineBuffer {
    bytes: [u8; LINE_LEN],
    len: usize,
    overflow: bool,
}

impl LineBuffer {
    const fn new() -> Self {
        Self {
            bytes: [0; LINE_LEN],
            len: 0,
            overflow: false,
        }
    }

    /// Adds a received byte, and returns the command once its line is complete.
    fn push(&mut self, byte: u8) -> Option<Command> {
        if byte == b'\n' || byte == b'\r' {
            let len = core::mem::take(&mut self.len);
            if core::mem::take(&mut self.overflow) {
                return Some(Command::Unknown);
            }
            return Some(gcode::parse(self.bytes.get(..len).unwrap_or_default()));
        }
        match self.bytes.get_mut(self.len) {
            Some(slot) => {
                *slot = byte;
                self.len += 1;
            }
            None => self.overflow = true,
        }
        None
    }
}

/// Returns the settings stored in EEPROM, if valid.
fn load_settings(eeprom: &Eeprom) -> Option<Settings> {
    let mut bytes = [0; settings::LEN];
    eeprom.read(SETTINGS_OFFSET, &mut bytes).ok()?;
    Settings::decode(&bytes)
}

fn sample_period() -> Duration {
    Duration::from_millis(heater::SAMPLE_PERIOD_MS.into())
}

/// Returns `deadline + period`, or `now + period` if the loop fell behind.
fn next_after(deadline: Instant, period: Duration, now: Instant) -> Instant {
    let next = deadline + period;
    if next > now { next } else { now + period }
}
