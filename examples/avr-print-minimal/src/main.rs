
#![no_main]
#![no_std]

use ariel_os as _;

#[ariel_os::task(autostart)]
async fn main() {
    loop {
        // Tight loop - toggle LED using embassy HAL directly
        // No format strings, no logging
    }
}
