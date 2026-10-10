// @generated

pub mod pins {
    ariel_os_hal::define_peripherals!(
        LedPeripherals { led0 : P0_00, led1 : P0_01, led2 : P0_04, led3 : P0_05, }
    );
    ariel_os_hal::define_peripherals!(
        ButtonPeripherals { button0 : P0_08, button1 : P0_09, button2 : P0_18, button3 :
        P0_19, }
    );
    ariel_os_hal::define_i2c_buses![
        { name : I2c0, peripheral : SERIAL0, sda : P0_30, scl : P0_31, aliases : [] },
    ];
    ariel_os_hal::define_uarts![
        { name : Uart0, device : SERIAL3, tx : P0_27, rx : P0_26, host_facing : true },
    ];
}
#[allow(unused_variables)]
pub fn init(peripherals: &mut ariel_os_hal::hal::OptionalPeripherals) {}
