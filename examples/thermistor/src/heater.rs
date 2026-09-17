//! Heater control: PID with Marlin-style slow PWM for a relay, plus thermal protection.
//!
//! This module only holds logic on integers. The caller passes the current time, in
//! milliseconds, and the latest temperature reading, and drives the relay.

/// A temperature in tenths of a degree Celsius.
pub type Decicelsius = i32;

/// Readings below this mean a broken or disconnected thermistor (Marlin's `HEATER_0_MINTEMP`).
pub const MIN_TEMP: Decicelsius = 50;
/// Readings from this temperature on stop the heater for good (Marlin's `HEATER_0_MAXTEMP`).
pub const MAX_TEMP: Decicelsius = 2750;
/// Highest accepted target, leaving room for overshoot below [`MAX_TEMP`] (Marlin's
/// `HOTEND_OVERSHOOT`).
pub const MAX_TARGET: Decicelsius = MAX_TEMP - 150;
/// Interval between two readings, in milliseconds.
pub const SAMPLE_PERIOD_MS: u16 = 250;

/// Consecutive invalid readings tolerated while heating, before a [`Fault::Sensor`].
const MAX_INVALID_READINGS: u8 = 4;
/// Further than this from the target, the heater is fully on or off, and the PID state is reset
/// (Marlin's `PID_FUNCTIONAL_RANGE`).
const PID_FUNCTIONAL_RANGE: Decicelsius = 100;
/// The integral term is kept in output units multiplied by this factor.
const INTEGRAL_SCALE: i32 = 10_000;
/// The derivative term is kept in output units multiplied by this factor.
const DERIVATIVE_SCALE: i32 = 16;
/// Each new derivative value moves the smoothed one by this fraction (1/16, a time constant of
/// about 4 s), as the derivative of a quantized reading is noisy (Marlin's `PID_K1`).
const DERIVATIVE_SMOOTHING: i32 = 16;
/// While heating, the temperature must rise by [`WATCH_INCREASE`] within this time, in
/// milliseconds (Marlin's `WATCH_TEMP_PERIOD`).
const WATCH_PERIOD_MS: u64 = 20_000;
/// See [`WATCH_PERIOD_MS`] (Marlin's `WATCH_TEMP_INCREASE`).
const WATCH_INCREASE: Decicelsius = 20;
/// The heating watch stops this close to the target (Marlin: `WATCH_TEMP_INCREASE +
/// TEMP_HYSTERESIS + 1`).
const WATCH_MARGIN: Decicelsius = 60;
/// Once the target is reached, the temperature may not stay more than
/// [`RUNAWAY_HYSTERESIS`] below it for longer than this, in milliseconds (Marlin's
/// `THERMAL_PROTECTION_PERIOD`).
///
/// Marlin uses 40 s. A hotend on a bench has no print fan to disturb it, and a shorter period
/// halves the overheating when the thermistor falls out of the block: the heater runs at full
/// power until the fault trips.
const RUNAWAY_PERIOD_MS: u64 = 20_000;
/// See [`RUNAWAY_PERIOD_MS`] (Marlin's `THERMAL_PROTECTION_HYSTERESIS`).
const RUNAWAY_HYSTERESIS: Decicelsius = 40;
/// The target counts as reached from this distance below it on.
const REACHED_MARGIN: Decicelsius = 10;

/// Length of a slow-PWM window, in milliseconds. The relay switches on and off at most once per
/// window.
///
/// This trades temperature ripple for relay wear. Marlin's `SLOW_PWM_HEATERS` uses about 8 s,
/// which suits a heated bed, but lets a small hotend block swing by several degrees; 2 s holds it
/// within a degree, at twice the switching of 4 s.
pub const WINDOW_MS: u16 = 4000;
/// Shortest time the relay stays on or off within a window, in milliseconds (Marlin's
/// `MIN_STATE_TIME`, 1 s for its 8 s window).
pub const MIN_STATE_MS: u16 = 500;

/// PID gains in hundredths, in Marlin's `M301` units: heater output (0–255) per °C, per °C·s, and
/// per °C/s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gains {
    /// Proportional gain, in hundredths.
    pub kp: u16,
    /// Integral gain, in hundredths.
    pub ki: u16,
    /// Derivative gain, in hundredths.
    pub kd: u16,
}

impl Gains {
    /// Marlin's default hotend gains, a starting point for a typical 40 W hotend.
    pub const DEFAULT: Self = Self {
        kp: 2220,
        ki: 108,
        kd: 11400,
    };
}

/// Why the heater was stopped. The heater stays off until [`Controller::clear_fault()`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    /// The thermistor reading stayed invalid while heating.
    Sensor,
    /// The temperature reached [`MAX_TEMP`].
    MaxTemp,
    /// The temperature did not rise while heating.
    HeatingFailed,
    /// The temperature fell below the target and stayed there.
    ThermalRunaway,
    /// The user requested an emergency stop.
    Killed,
}

impl Fault {
    /// Returns a short description of the fault (strings take RAM on AVR).
    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            Self::Sensor => "thermistor fault",
            Self::MaxTemp => "MAXTEMP",
            Self::HeatingFailed => "heating failed",
            Self::ThermalRunaway => "thermal runaway",
            Self::Killed => "stopped by M112",
        }
    }
}

/// State of the thermal runaway protection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Runaway {
    /// No target.
    Inactive,
    /// Waiting for the temperature to reach the target.
    Heating,
    /// At the target; the temperature must be back above the hysteresis before the deadline.
    Holding { deadline: u64 },
}

/// Computes the heater output from temperature readings, and stops the heater on faults.
pub struct Controller {
    gains: Gains,
    target: Decicelsius,
    temperature: Option<Decicelsius>,
    output: u8,
    fault: Option<Fault>,
    invalid_readings: u8,
    pid_active: bool,
    integral: i32,
    derivative: i32,
    /// Whether the next valid reading must restart the heating watch.
    rearm_watch: bool,
    /// Deadline and temperature to reach by then, while the heating watch runs.
    watch: Option<(u64, Decicelsius)>,
    runaway: Runaway,
}

impl Controller {
    /// Returns a controller with the heater off.
    #[must_use]
    pub const fn new(gains: Gains) -> Self {
        Self {
            gains,
            target: 0,
            temperature: None,
            output: 0,
            fault: None,
            invalid_readings: 0,
            pid_active: false,
            integral: 0,
            derivative: 0,
            rearm_watch: false,
            watch: None,
            runaway: Runaway::Inactive,
        }
    }

    /// Returns the PID gains.
    #[must_use]
    pub fn gains(&self) -> Gains {
        self.gains
    }

    /// Replaces the PID gains.
    pub fn set_gains(&mut self, gains: Gains) {
        self.gains = gains;
        self.reset_pid();
    }

    /// Returns the target temperature; 0 means that the heater is off.
    #[must_use]
    pub fn target(&self) -> Decicelsius {
        self.target
    }

    /// Sets the target temperature, clamped to [`MAX_TARGET`]; 0 or less turns the heater off.
    ///
    /// Ignored while a fault is active; returns whether the target was applied.
    pub fn set_target(&mut self, target: Decicelsius) -> bool {
        if self.fault.is_some() {
            return false;
        }
        self.target = target.clamp(0, MAX_TARGET);
        self.reset_pid();
        self.watch = None;
        self.rearm_watch = self.target > 0;
        self.runaway = if self.target > 0 {
            Runaway::Heating
        } else {
            Runaway::Inactive
        };
        if self.target == 0 {
            self.output = 0;
        }
        true
    }

    /// Returns the latest valid temperature, if any.
    #[must_use]
    pub fn temperature(&self) -> Option<Decicelsius> {
        self.temperature
    }

    /// Returns the heater output: 0 is off, 255 is fully on.
    #[must_use]
    pub fn output(&self) -> u8 {
        self.output
    }

    /// Returns the active fault, if any.
    #[must_use]
    pub fn fault(&self) -> Option<Fault> {
        self.fault
    }

    /// Stops the heater until [`Controller::clear_fault()`].
    pub fn kill(&mut self) {
        self.trip(Fault::Killed);
    }

    /// Clears the active fault. The heater stays off until a new target is set.
    pub fn clear_fault(&mut self) {
        self.fault = None;
        self.invalid_readings = 0;
    }

    /// Processes a reading taken at `now` (in milliseconds), [`None`] when it is invalid, and
    /// returns the new heater output.
    pub fn update(&mut self, now: u64, reading: Option<Decicelsius>) -> u8 {
        self.output = self.compute(now, reading);
        self.output
    }

    fn compute(&mut self, now: u64, reading: Option<Decicelsius>) -> u8 {
        let Some(temperature) = reading.filter(|&t| t >= MIN_TEMP) else {
            self.temperature = None;
            self.invalid_readings = self.invalid_readings.saturating_add(1);
            if self.target > 0 && self.invalid_readings >= MAX_INVALID_READINGS {
                self.trip(Fault::Sensor);
            }
            // The heater stays off while the temperature is unknown. Clearing the temperature
            // also skips the next derivative update, which needs two consecutive readings.
            return 0;
        };
        self.invalid_readings = 0;
        let previous = self.temperature.replace(temperature);

        // Checked even with the heater off: a relay with welded contacts keeps heating.
        if temperature >= MAX_TEMP {
            self.trip(Fault::MaxTemp);
        }
        if self.fault.is_none() && self.target > 0 {
            self.check_heating(now, temperature);
        }
        if self.fault.is_some() || self.target == 0 {
            return 0;
        }
        self.pid(temperature, previous)
    }

    /// Runs the heating watch and the thermal runaway protection.
    fn check_heating(&mut self, now: u64, temperature: Decicelsius) {
        if self.rearm_watch {
            self.rearm_watch = false;
            self.start_watch(now, temperature);
        }
        if let Some((deadline, goal)) = self.watch
            && now >= deadline
        {
            if temperature < goal {
                self.trip(Fault::HeatingFailed);
                return;
            }
            self.start_watch(now, temperature);
        }

        match self.runaway {
            Runaway::Inactive => {}
            Runaway::Heating => {
                if temperature >= self.target - REACHED_MARGIN {
                    self.runaway = Runaway::Holding {
                        deadline: now + RUNAWAY_PERIOD_MS,
                    };
                }
            }
            Runaway::Holding { deadline } => {
                if temperature >= self.target - RUNAWAY_HYSTERESIS {
                    self.runaway = Runaway::Holding {
                        deadline: now + RUNAWAY_PERIOD_MS,
                    };
                } else if now >= deadline {
                    self.trip(Fault::ThermalRunaway);
                }
            }
        }
    }

    /// Starts a heating watch period, unless the temperature is already close to the target.
    fn start_watch(&mut self, now: u64, temperature: Decicelsius) {
        self.watch = (temperature < self.target - WATCH_MARGIN)
            .then_some((now + WATCH_PERIOD_MS, temperature + WATCH_INCREASE));
    }

    /// Returns the PID output for `temperature`, following Marlin's `get_pid_output_hotend()`.
    fn pid(&mut self, temperature: Decicelsius, previous: Option<Decicelsius>) -> u8 {
        let error = self.target - temperature;
        if error > PID_FUNCTIONAL_RANGE {
            self.reset_pid();
            return u8::MAX;
        }
        if error < -PID_FUNCTIONAL_RANGE {
            self.reset_pid();
            return 0;
        }
        if !self.pid_active {
            // Entering the functional range: start from a clean state, like Marlin.
            self.pid_active = true;
            self.integral = 0;
            self.derivative = 0;
        }

        let Gains { kp, ki, kd } = self.gains;
        let dt_ms = i32::from(SAMPLE_PERIOD_MS);
        // (Kp × 100) × (error × 10) / 1000 = Kp × error.
        let p = i32::from(kp) * error / 1000;
        // (Ki × 100) × (error × 10) × (dt × 1000) / 100 = Ki × error × dt × INTEGRAL_SCALE.
        // |error| <= PID_FUNCTIONAL_RANGE here, so this fits an `i32`. Clamping the sum keeps the
        // integral term within the output range (anti-windup).
        self.integral = (self.integral + i32::from(ki) * error * dt_ms / 100)
            .clamp(0, i32::from(u8::MAX) * INTEGRAL_SCALE);
        let i = self.integral / INTEGRAL_SCALE;
        // Derivative of the measurement, not of the error, so that target changes cause no kick:
        // (Kd × 100) × (Δt × 10) / (dt × 1000) = Kd × Δt / dt.
        if let Some(previous) = previous {
            let raw = -(i32::from(kd) * (temperature - previous) / dt_ms);
            self.derivative += (raw * DERIVATIVE_SCALE - self.derivative) / DERIVATIVE_SMOOTHING;
        }
        let d = self.derivative / DERIVATIVE_SCALE;

        let output = (p + i + d).clamp(0, i32::from(u8::MAX));
        u8::try_from(output).unwrap_or(u8::MAX)
    }

    /// Makes the next PID update start from a clean state.
    fn reset_pid(&mut self) {
        self.pid_active = false;
    }

    fn trip(&mut self, fault: Fault) {
        if self.fault.is_none() {
            self.fault = Some(fault);
        }
        self.target = 0;
        self.output = 0;
        self.watch = None;
        self.rearm_watch = false;
        self.runaway = Runaway::Inactive;
        self.reset_pid();
    }
}

/// Turns the heater output into relay on/off times, like Marlin's `SLOW_PWM_HEATERS`.
///
/// Each [`WINDOW_MS`] window starts with the relay on for `output / 255` of the window, then off.
/// On or off times shorter than [`MIN_STATE_MS`] are skipped, and the difference is carried over
/// to the next windows, so the average power stays right while the relay switches less.
#[derive(Default)]
pub struct SlowPwm {
    window_start: Option<u64>,
    on_ms: u16,
    /// On-time owed to (positive) or taken from (negative) the next windows, in milliseconds.
    carry: i32,
}

impl SlowPwm {
    /// Returns a slow PWM with the relay off.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            window_start: None,
            on_ms: 0,
            carry: 0,
        }
    }

    /// Returns whether the relay must be on at `now` (in milliseconds) for `output`.
    ///
    /// An output of 0 switches the relay off right away, even in the middle of a window.
    pub fn poll(&mut self, now: u64, output: u8) -> bool {
        let window = u64::from(WINDOW_MS);
        let start = match self.window_start {
            Some(start) if now < start + window => start,
            previous => {
                // Windows follow each other, unless the caller fell more than a window behind.
                let start = match previous {
                    Some(start) if now < start + 2 * window => start + window,
                    _ => now,
                };
                self.window_start = Some(start);
                self.on_ms = self.plan(output);
                start
            }
        };
        if output == 0 {
            self.carry = 0;
            return false;
        }
        now < start + u64::from(self.on_ms)
    }

    /// Returns the on-time of a new window, in milliseconds.
    fn plan(&mut self, output: u8) -> u16 {
        let window = i32::from(WINDOW_MS);
        let min_state = i32::from(MIN_STATE_MS);
        let (on_ms, carry) = match output {
            0 => (0, 0),
            u8::MAX => (window, 0),
            _ => {
                let wanted = i32::from(output) * window / i32::from(u8::MAX) + self.carry;
                let on_ms = if wanted < min_state {
                    0
                } else if wanted > window - min_state {
                    window
                } else {
                    wanted
                };
                (on_ms, (wanted - on_ms).clamp(-window, window))
            }
        };
        self.carry = carry;
        u16::try_from(on_ms).unwrap_or(WINDOW_MS)
    }
}
