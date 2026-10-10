// @generated

pub mod pins {
    ariel_os_hal::define_peripherals!(LedPeripherals { led0 : P0_02, });
    ariel_os_hal::define_peripherals!(ButtonPeripherals { button0 : P0_06, });
    ariel_os_hal::define_i2c_buses![
        { name : I2c0, peripheral : SERIAL0, sda : P0_30, scl : P0_31, aliases : [] },
    ];
}
#[allow(unused_variables)]
pub fn init(peripherals: &mut ariel_os_hal::hal::OptionalPeripherals) {}
