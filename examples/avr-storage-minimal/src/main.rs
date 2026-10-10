#![no_main]
#![no_std]

//! Counts the boots of the board in its EEPROM, and prints the count on the host-facing UART.
//!
//! This is the smallest program that exercises [`ariel_os::storage`], written to see how much
//! flash that costs on a board as small as the Arduino Uno.

use ariel_os::{hal, storage, uart::Baudrate};
use embedded_io_async::Write as _;

/// Key of the boot counter.
const KEY: &str = "boots";

#[ariel_os::task(autostart, peripherals)]
async fn main(peripherals: ariel_os_boards::pins::HOST_FACING_UART) {
    let mut rx_buf = [0; 2];
    let mut tx_buf = [0; 32];
    let mut config = hal::uart::Config::default();
    config.baudrate = Baudrate::_57600;
    let mut uart = peripherals
        .build_with_config(&mut rx_buf, &mut tx_buf, config)
        .unwrap();

    // Writes to the UART never fail, so their results are ignored.
    let stored = if let Ok(stored) = storage::get::<u8>(KEY).await {
        stored.unwrap_or(0)
    } else {
        // The EEPROM holds data that the storage cannot read, such as the settings of another
        // firmware. Ariel OS erases such data at boot on the other platforms, but not on AVR,
        // and the bootloader of the Uno cannot erase the EEPROM for `avrdude`.
        let _ = uart.write_all(b"erasing the storage\r\n").await;
        let _ = storage::erase_all().await;
        0
    };
    let boots = stored.wrapping_add(1);

    if storage::insert(KEY, boots).await.is_ok() {
        let mut digits = [0; 3];
        let _ = uart.write_all(b"boot ").await;
        let _ = uart.write_all(decimal(boots, &mut digits)).await;
        let _ = uart.write_all(b"\r\n").await;
    } else {
        let _ = uart.write_all(b"cannot write the storage\r\n").await;
    }
    // Dropping the UART at the end of the task turns it off, even with bytes left to send.
    let _ = uart.flush().await;
}

/// Writes `value` in decimal into `digits`, and returns the digits without leading zeros.
fn decimal(value: u8, digits: &mut [u8; 3]) -> &[u8] {
    *digits = [value / 100, value / 10 % 10, value % 10].map(|digit| b'0' + digit);
    let skip = match value {
        100.. => 0,
        10.. => 1,
        _ => 2,
    };
    digits.get(skip..).unwrap_or_default()
}
