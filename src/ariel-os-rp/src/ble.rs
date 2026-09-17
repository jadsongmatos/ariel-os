use bt_hci::controller::ExternalController;
use cyw43::bluetooth::BtDriver;

pub type BleController = ExternalController<BtDriver<'static>, SLOTS>;

/// Number of command slots for the Bluetooth driver.
pub const SLOTS: usize = 10;
