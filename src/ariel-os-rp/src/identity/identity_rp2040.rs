#![expect(unsafe_code)]

use ariel_os_log::debug;

pub struct DeviceId(u64);

// Manufacturer IDs for which the memory chips return (at least) eight unique bytes as unique ID.
// NOTE: this can be expanded with other manufacturers; currently the type of memory chip is
// constrained by the second-stage bootloader (boot2).
const KNOWN_JEDEC_MANUFACTURED_ID: &[u8] = &[
    0xef, // Used by Winbond chips.
];

impl ariel_os_embassy_common::identity::DeviceId for DeviceId {
    type Bytes = [u8; 8];

    fn get() -> Result<Self, impl core::error::Error> {
        // The flash size does not matter here.
        const FLASH_SIZE: usize = 0;

        let mut unique_id: Self::Bytes = Default::default();

        // This critical section takes less than 200 µs on a RPi Pico.
        let jedec_id = critical_section::with(|_| {
            // SAFETY: the peripheral is stolen and used in a critical section.
            let flash_peripheral = unsafe { embassy_rp::peripherals::FLASH::steal() };
            let mut flash =
                embassy_rp::flash::Flash::<_, _, FLASH_SIZE>::new_blocking(flash_peripheral);

            flash
                .blocking_jedec_id()
                .map_err(|_| Error::MemoryChipIdInaccessible)
        })?;
        debug!("JEDEC ID: {:#x}", jedec_id);

        // The JEDEC ID is fetched with the 0x9f SPI instruction, which returns three bytes.
        #[expect(clippy::cast_possible_truncation, reason = "the truncation is desired")]
        let manufacturer_id = (jedec_id >> 16) as u8;

        if !KNOWN_JEDEC_MANUFACTURED_ID.contains(&manufacturer_id) {
            return Err(Error::UnsupportedMemoryChip);
        }

        // This critical section takes less than 200 µs on a RPi Pico.
        critical_section::with(|_| {
            // SAFETY: the peripheral is stolen and used in a critical section.
            let flash_peripheral = unsafe { embassy_rp::peripherals::FLASH::steal() };
            let mut flash =
                embassy_rp::flash::Flash::<_, _, FLASH_SIZE>::new_blocking(flash_peripheral);

            flash
                .blocking_unique_id(&mut unique_id)
                .map_err(|_| Error::MemoryChipIdInaccessible)
        })?;

        let unique_id = u64::from_be_bytes(unique_id);

        Ok::<_, Error>(Self(unique_id))
    }

    fn bytes(&self) -> Self::Bytes {
        self.0.to_le_bytes()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    MemoryChipIdInaccessible,
    UnsupportedMemoryChip,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::MemoryChipIdInaccessible => {
                write!(f, "the ID of the memory chip is accessible")
            }
            Self::UnsupportedMemoryChip => {
                write!(f, "the memory chip is unsupported to fetch its unique ID")
            }
        }
    }
}

impl core::error::Error for Error {}
