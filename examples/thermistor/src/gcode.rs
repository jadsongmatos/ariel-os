//! Parses the few G-code commands the controller understands.
//!
//! Lines may come from a serial terminal or from a 3D-printer host program, so a leading line
//! number (`N12`), a trailing checksum (`*34`), and comments (`; …`) are accepted and ignored.
//! Numbers are returned in hundredths, so `S200.5` gives `20050`.

/// A parsed command line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// A blank or comment-only line.
    Empty,
    /// `M104 S<°C>`: sets the target temperature; `S0` turns the heater off.
    SetTarget(Option<i32>),
    /// `M105`: reports the temperature.
    Report,
    /// `M112`: emergency stop.
    Kill,
    /// `M155 S<seconds>`: sets the interval of automatic reports; `S0` disables them.
    AutoReport(Option<i32>),
    /// `M301 [P<kp>] [I<ki>] [D<kd>]`: sets the given PID gains, or reports them.
    Pid {
        /// Proportional gain.
        p: Option<i32>,
        /// Integral gain.
        i: Option<i32>,
        /// Derivative gain.
        d: Option<i32>,
    },
    /// `M305 [R<ohms>] [K<ohms>]`: sets the thermistor pull-up, calibrates it against a known
    /// resistance at the thermistor's place, or reports it.
    Thermistor {
        /// Pull-up resistance.
        r: Option<i32>,
        /// Known resistance in place of the thermistor.
        k: Option<i32>,
    },
    /// `M500`: saves the target, the PID gains, and the pull-up to EEPROM.
    Save,
    /// `M999`: clears a fault.
    ClearFault,
    /// Any other command.
    Unknown,
}

/// Parses one command line, without its line terminator.
#[must_use]
pub fn parse(line: &[u8]) -> Command {
    let end = line
        .iter()
        .position(|&b| b == b';' || b == b'*')
        .unwrap_or(line.len());
    let line = line.get(..end).unwrap_or_default();

    let mut words = Words { rest: line };
    let Some(mut word) = words.next() else {
        return Command::Empty;
    };
    if word.0 == b'N' {
        let Some(next) = words.next() else {
            return Command::Empty;
        };
        word = next;
    }
    let (b'M', Some(code)) = word else {
        return Command::Unknown;
    };
    let param = |letter| {
        Words { rest: words.rest }
            .find(|&(l, _)| l == letter)
            .and_then(|(_, value)| value)
    };

    match code {
        10400 => Command::SetTarget(param(b'S')),
        10500 => Command::Report,
        11200 => Command::Kill,
        15500 => Command::AutoReport(param(b'S')),
        30100 => Command::Pid {
            p: param(b'P'),
            i: param(b'I'),
            d: param(b'D'),
        },
        30500 => Command::Thermistor {
            r: param(b'R'),
            k: param(b'K'),
        },
        50000 => Command::Save,
        99900 => Command::ClearFault,
        _ => Command::Unknown,
    }
}

/// Iterates over the words of a line: an uppercase letter, and the number that follows it, if any.
struct Words<'a> {
    rest: &'a [u8],
}

impl Iterator for Words<'_> {
    type Item = (u8, Option<i32>);

    fn next(&mut self) -> Option<Self::Item> {
        let start = self.rest.iter().position(|b| !b.is_ascii_whitespace())?;
        let (&letter, rest) = self.rest.get(start..)?.split_first()?;
        let (value, len) = parse_hundredths(rest);
        self.rest = rest.get(len..).unwrap_or_default();
        Some((letter.to_ascii_uppercase(), value))
    }
}

/// Returns `value * 10 + digit`, saturating at [`i32::MAX`] for non-negative `value`.
///
/// The bound is checked by hand: on AVR, `i32::saturating_mul()` and `i32::checked_mul()` call
/// `__mulosi4`, whose `compiler-builtins` version writes a 32-bit overflow flag where LLVM only
/// reserved a 16-bit `int`, which corrupts the caller's saved frame pointer.
fn push_digit(value: i32, digit: u8) -> i32 {
    if value > (i32::MAX - 9) / 10 {
        i32::MAX
    } else {
        value.wrapping_mul(10).wrapping_add(i32::from(digit))
    }
}

/// Parses the decimal number at the start of `s`, in hundredths, and returns it with the number
/// of bytes it takes.
///
/// Decimals beyond the second are ignored, and large values saturate.
fn parse_hundredths(s: &[u8]) -> (Option<i32>, usize) {
    let (negative, mut len) = match s.first() {
        Some(b'-') => (true, 1),
        Some(b'+') => (false, 1),
        _ => (false, 0),
    };
    let mut value: i32 = 0;
    let mut has_digits = false;
    let mut decimals: Option<u8> = None;
    while let Some(&b) = s.get(len) {
        match b {
            b'0'..=b'9' => {
                has_digits = true;
                if decimals.is_none_or(|n| n < 2) {
                    value = push_digit(value, b - b'0');
                    decimals = decimals.map(|n| n + 1);
                }
            }
            b'.' if decimals.is_none() => decimals = Some(0),
            _ => break,
        }
        len += 1;
    }
    if !has_digits {
        return (None, len);
    }
    for _ in decimals.unwrap_or(0)..2 {
        value = push_digit(value, 0);
    }
    (Some(if negative { -value } else { value }), len)
}
