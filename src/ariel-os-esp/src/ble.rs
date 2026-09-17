use bt_hci::controller::ExternalController;
use embassy_executor::Spawner;
use esp_radio::ble::controller::BleConnector;

/// Number of command slots for the BLE driver.
pub const SLOTS: usize = 10;

pub type BleController = ExternalController<BleConnector<'static>, SLOTS>;

pub fn build_controller<'a>(
    peripherals: &mut esp_hal::peripherals::OptionalPeripherals,
    _spawner: Spawner,
) -> BleController {
    let connector = BleConnector::new(peripherals.BT.take().unwrap(), Default::default()).unwrap();
    ExternalController::new(connector)
}
