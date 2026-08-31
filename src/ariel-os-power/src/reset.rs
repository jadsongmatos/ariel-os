/// Reboots the MCU.
///
/// This function initiates a software reset of the microcontroller and never returns.
pub fn reboot() -> ! {
    cfg_select! {
        context = "cortex-m" => {
            cortex_m::peripheral::SCB::sys_reset()
        }
        context = "esp" => {
            esp_hal::system::software_reset()
        }
        context = "native" => {
            std::process::exit(0)
        }
        context = "avr" => {
            // Enable watchdog timer for shortest timeout (16ms) and wait for reset
            // This works on ATmega328P and most AVR devices
            #[expect(unsafe_code, reason = "hardware register access for watchdog reset")]
            unsafe {
                // WDTCSR: Watchdog Timer Control Register
                // WDE: Watchdog Enable
                // WDP2:WDP0: Watchdog Prescaler bits
                // 16ms timeout: WDP2=0, WDP1=0, WDP0=0
                let wdtcsr = 0x60 as *mut u8;
                core::ptr::write_volatile(wdtcsr, (1 << 3) | (1 << 4)); // WDCE | WDE
                core::ptr::write_volatile(wdtcsr, 1 << 3); // WDE only, 16ms timeout
            }
            #[expect(clippy::empty_loop, reason = "waiting for watchdog reset")]
            loop {}
        }
        _ => {
            #[expect(clippy::empty_loop, reason = "for platform-independent tooling only")]
            loop {}
        }
    }
}
