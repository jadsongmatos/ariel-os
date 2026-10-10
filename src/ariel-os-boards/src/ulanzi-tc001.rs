// @generated

pub mod pins {
    ariel_os_hal::define_peripherals!(
        ButtonPeripherals { button0 : GPIO26, button1 : GPIO27, button2 : GPIO14, }
    );
    ariel_os_hal::define_i2c_buses![
        { name : I2c0, peripheral : I2C0, sda : GPIO21, scl : GPIO22, aliases : [] },
    ];
}
#[allow(unused_variables)]
pub fn init(peripherals: &mut ariel_os_hal::hal::OptionalPeripherals) {
    {
        let pin = peripherals.GPIO15.take().unwrap();
        let output = ariel_os_hal::gpio::Output::new(
            pin,
            ariel_os_embassy_common::gpio::Level::Low,
        );
        core::mem::forget(output);
    }
}
