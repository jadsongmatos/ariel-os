// @generated

pub mod pins {
    ariel_os_hal::define_peripherals!(ButtonPeripherals { button0 : GPIO0, });
    ariel_os_hal::define_i2c_buses![
        { name : I2c0, peripheral : I2C0, sda : GPIO47, scl : GPIO48, aliases : [] },
    ];
}
#[allow(unused_variables)]
pub fn init(peripherals: &mut ariel_os_hal::hal::OptionalPeripherals) {}
