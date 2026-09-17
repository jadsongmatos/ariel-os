//! Settings kept in EEPROM across resets, saved with `M500`.

use crate::{
    heater::{Decicelsius, Gains, MAX_TARGET},
    ntc,
};

/// Size of the encoded settings, in bytes.
pub const LEN: usize = 13;
/// Lowest accepted thermistor pull-up, in ohms.
pub const MIN_PULLUP_OHMS: u16 = 1000;

/// Marks the start of valid settings; an erased EEPROM reads `0xff`.
const MAGIC: u8 = b'H';
/// Layout version, to change when the layout does.
const VERSION: u8 = 2;

/// Settings applied at boot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Target temperature applied at boot; 0 keeps the heater off.
    pub target: Decicelsius,
    /// PID gains.
    pub gains: Gains,
    /// Thermistor pull-up, in ohms.
    pub pullup_ohms: u16,
}

impl Settings {
    /// Settings used when the EEPROM holds none: heater off, Marlin's default gains, and the
    /// pull-up that the thermistor table was built for.
    pub const DEFAULT: Self = Self {
        target: 0,
        gains: Gains::DEFAULT,
        pullup_ohms: ntc::R_PULLUP_OHMS,
    };

    /// Returns the EEPROM representation of the settings.
    #[must_use]
    pub fn encode(&self) -> [u8; LEN] {
        let target = u16::try_from(self.target.clamp(0, MAX_TARGET)).unwrap_or(0);
        let [t0, t1] = target.to_le_bytes();
        let [p0, p1] = self.gains.kp.to_le_bytes();
        let [i0, i1] = self.gains.ki.to_le_bytes();
        let [d0, d1] = self.gains.kd.to_le_bytes();
        let [r0, r1] = self.pullup_ohms.to_le_bytes();
        let mut bytes = [MAGIC, VERSION, t0, t1, p0, p1, i0, i1, d0, d1, r0, r1, 0];
        if let Some((sum, data)) = bytes.split_last_mut() {
            *sum = checksum(data);
        }
        bytes
    }

    /// Returns the settings stored in `bytes`, or [`None`] if they are missing or corrupted.
    #[must_use]
    pub fn decode(bytes: &[u8; LEN]) -> Option<Self> {
        let [MAGIC, VERSION, t0, t1, p0, p1, i0, i1, d0, d1, r0, r1, sum] = *bytes else {
            return None;
        };
        let (_, data) = bytes.split_last()?;
        if sum != checksum(data) {
            return None;
        }
        let target = Decicelsius::from(u16::from_le_bytes([t0, t1]));
        let pullup_ohms = u16::from_le_bytes([r0, r1]);
        if target > MAX_TARGET || pullup_ohms < MIN_PULLUP_OHMS {
            return None;
        }
        Some(Self {
            target,
            gains: Gains {
                kp: u16::from_le_bytes([p0, p1]),
                ki: u16::from_le_bytes([i0, i1]),
                kd: u16::from_le_bytes([d0, d1]),
            },
            pullup_ohms,
        })
    }
}

/// Returns the checksum of `data`: the complement of the sum of its bytes.
fn checksum(data: &[u8]) -> u8 {
    !data.iter().fold(0, |sum: u8, &b| sum.wrapping_add(b))
}
