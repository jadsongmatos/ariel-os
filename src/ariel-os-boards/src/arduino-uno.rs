// @generated

pub mod pins {
    ariel_os_hal::define_peripherals!(LedPeripherals { led0 : PB5, });
    ariel_os_hal::define_peripherals!(ButtonPeripherals { button0 : PD2, });
    ariel_os_hal::define_uarts![
        { name : uart0, device : USART0, tx : PD1, rx : PD0, host_facing : true },
    ];
}
#[allow(unused_variables)]
pub fn init(peripherals: &mut ariel_os_hal::hal::OptionalPeripherals) {}
