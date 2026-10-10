// @generated

pub mod pins {
    ariel_os_hal::define_peripherals!(
        LedPeripherals { led0 : P0_13, led1 : P0_14, led2 : P0_15, led3 : P0_16, }
    );
    ariel_os_hal::define_peripherals!(
        ButtonPeripherals { button0 : P0_11, button1 : P0_12, button2 : P0_24, button3 :
        P0_25, }
    );
    ariel_os_hal::define_i2c_buses![
        { name : I2c0, peripheral : TWISPI0, sda : P0_26, scl : P0_27, aliases : [] },
    ];
    ariel_os_hal::define_uarts![
        { name : Uart0, device : UARTE0, tx : P0_06, rx : P0_08, host_facing : true },
    ];
}
#[allow(unused_variables)]
pub fn init(peripherals: &mut ariel_os_hal::hal::OptionalPeripherals) {}
