//! Provides control over the system BLE (Bluetooth Low Energy) stack.
//!
//! All interactions happen through the [`trouble_host::Stack`] struct that can be obtained using
//! [`ble_stack()`].
//!
//! The address of the device is randomly generated at boot and may be rotated during execution.
//!
//! # Current implementation
//!
//! The address is not currently rotated during execution; however this behavior may not be relied upon.

use ariel_os_embassy_common::cell::SameExecutorCell;
use embassy_executor::Spawner;
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex, once_lock::OnceLock,
};
use futures_util::FutureExt as _;
use static_cell::StaticCell;
use trouble_host::{
    Address, Stack,
    prelude::{AddrKind, BdAddr, DefaultPacketPool},
};

use ariel_os_embassy_common::ble::Config;
use ariel_os_log::debug;

use crate::hal::ble::BleController;

/// The BLE Stack, representing access to this device's BLE controller.
/// This type is returned by [`ble_stack()`].
///
/// It is a [`trouble_host::Stack`] configured with an appropriate BLE controller and the default
/// packet pool.
pub type BleStack = Stack<'static, BleController, DefaultPacketPool>;

static CURRENT_ADDRESS: OnceLock<Address> = OnceLock::new();
#[allow(dead_code)]
static STACK: StaticCell<SameExecutorCell<BleStack>> = StaticCell::new();
// The stack can effectively only be taken by a single application; once taken, the Option is None.
static STACKREF: OnceLock<
    Mutex<CriticalSectionRawMutex, Option<&'static mut SameExecutorCell<BleStack>>>,
> = OnceLock::new();

#[allow(dead_code, reason = "false positive during builds outside of laze")]
pub(crate) fn config() -> Config {
    // Scanning apps show that the last byte of the array appears fist.
    let mut raw_address = get_random_addr();

    // Set the two most significant bits to 1 to indicate a static random address https://www.bluetooth.com/wp-content/uploads/Files/Specification/HTML/Core-54/out/en/low-energy-controller/link-layer-specification.html#UUID-7edea27a-a47f-8436-4bd7-aedc1945c366_figure-idm4497995733171233616486354268
    raw_address[5] |= 0b1100_0000;

    let address = Address {
        addr: BdAddr::new(raw_address),
        kind: AddrKind::RANDOM,
    };

    let _ = CURRENT_ADDRESS.init(address);

    debug!("Setting random address: {:?}", address);

    Config { address }
}

/// Returns the BLE address currently in use.
///
/// Note that the BLE address may be rotated over time.
pub fn current_address() -> impl Future<Output = Address> {
    // Using map() to avoid creating a new state machine.
    CURRENT_ADDRESS.get().map(|addr| *addr)
}

/// Generates a random address.
#[cfg(not(feature = "ble-config-static-address"))]
fn get_random_addr() -> [u8; 6] {
    let mut addr = [0u8; 6];
    rand_core::RngCore::fill_bytes(&mut ariel_os_random::crypto_rng(), &mut addr);
    addr
}

/// Get random address from env.
#[cfg(feature = "ble-config-static-address")]
fn get_random_addr() -> [u8; 6] {
    use ariel_os_utils::eui48_from_env;
    eui48_from_env!(
        "CONFIG_BLE_STATIC_ADDRESS",
        "static address for BLE in format XX:XX:XX:XX:XX:XX",
    )
}

/// Returns the system BLE stack, [`BleStack`].
///
/// # Panics
///
/// For this function to succeed, it can only be called from the system executor, and only once:
///
/// - Panics if the stack was already taken.
/// - Panics when not called from the system executor.
#[doc(alias = "bluetooth_stack")]
pub async fn ble_stack() -> &'static mut BleStack {
    STACKREF
        .get()
        .await
        .try_lock()
        .expect("Two tasks racing for lock, one would fail the main-executor check")
        .take()
        .expect("Stack was already taken")
        .get_mut_async()
        .await
        .expect("Stack needs to be taken from main executor")
}

#[allow(dead_code, reason = "false positive during builds outside of laze")]
pub(crate) fn init_stack(controller: crate::hal::ble::BleController, spawner: Spawner) {
    let config = config();
    let mut rng = ariel_os_random::crypto_rng();

    let resources = ariel_os_embassy_common::ble::get_ble_host_resources();

    let stack = trouble_host::new(controller, resources)
        .set_random_generator_seed(&mut rng)
        .set_random_address(config.address);

    let stackref = STACK.init(SameExecutorCell::new(stack, spawner));
    let _ = STACKREF.init(Some(stackref).into());
}
