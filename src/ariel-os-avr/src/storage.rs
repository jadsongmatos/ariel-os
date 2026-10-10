//! AVR EEPROM backend for [`ariel_os_storage`].
//!
//! The ATmega328P exposes 1 KiB of EEPROM in an address space of its own. This adapter
//! presents that EEPROM as the logical NOR flash range `0..1024` used by
//! `sequential-storage`.
//!
//! The EEPROM reads, writes and erases single bytes, but `sequential-storage` lays its items
//! out in pages, and a page has to hold its two status bytes plus at least one item. The
//! backend therefore groups the EEPROM into [`PAGE_SIZE`]-byte pages and erases them byte by
//! byte, skipping the bytes that are already erased.
//!
//! Writing or erasing a byte takes 1.8 ms, during which the executor cannot run: the
//! `avr-hal` EEPROM driver polls the write-enable bit instead of using the ready interrupt.

use embassy_avr::storage::Eeprom;

/// Size of an emulated flash page, in bytes.
///
/// The 1 KiB EEPROM holds 8 of them. Each page spends 2 bytes on the status markers of
/// `sequential-storage`, and every stored item takes 8 bytes of header plus its key and its
/// value, so a page has to stay well above that.
pub const PAGE_SIZE: usize = 128;

/// EEPROM-backed NOR flash used by [`ariel_os_storage`].
pub struct Flash {
    eeprom: Eeprom,
}

/// Errors returned by the AVR EEPROM backend.
#[derive(Debug, PartialEq, Eq)]
pub enum FlashError {
    /// The operation would leave the EEPROM.
    OutOfBounds,
    /// The erase range is not aligned to [`PAGE_SIZE`].
    NotAligned,
}

impl Flash {
    /// Creates a flash backend from an EEPROM peripheral.
    #[must_use]
    pub fn new(eeprom: Eeprom) -> Self {
        Self { eeprom }
    }

    /// Returns `offset` as an EEPROM address.
    ///
    /// # Errors
    ///
    /// Returns [`FlashError::OutOfBounds`] if `len` bytes from `offset` leave the EEPROM.
    fn address(offset: u32, len: usize) -> Result<u16, FlashError> {
        let capacity = usize::from(Eeprom::CAPACITY);
        let offset = usize::try_from(offset).map_err(|_| FlashError::OutOfBounds)?;
        if offset > capacity || len > capacity - offset {
            return Err(FlashError::OutOfBounds);
        }
        u16::try_from(offset).map_err(|_| FlashError::OutOfBounds)
    }
}

impl embedded_storage_async::nor_flash::ErrorType for Flash {
    type Error = FlashError;
}

impl embedded_storage_async::nor_flash::ReadNorFlash for Flash {
    const READ_SIZE: usize = 1;

    async fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        let offset = Self::address(offset, bytes.len())?;
        self.eeprom
            .read(offset, bytes)
            .map_err(|_| FlashError::OutOfBounds)
    }

    fn capacity(&self) -> usize {
        usize::from(Eeprom::CAPACITY)
    }
}

impl embedded_storage_async::nor_flash::NorFlash for Flash {
    const WRITE_SIZE: usize = 1;
    const ERASE_SIZE: usize = PAGE_SIZE;

    async fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        let offset = Self::address(offset, bytes.len())?;
        self.eeprom
            .write(offset, bytes)
            .map_err(|_| FlashError::OutOfBounds)
    }

    async fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        // `sequential-storage` erases whole pages, and the EEPROM is only asked for the
        // bytes of those pages.
        #[expect(clippy::cast_possible_truncation, reason = "`PAGE_SIZE` is 128")]
        const PAGE_MASK: u32 = PAGE_SIZE as u32 - 1;
        if from & PAGE_MASK != 0 || to & PAGE_MASK != 0 {
            return Err(FlashError::NotAligned);
        }
        if from > to {
            return Err(FlashError::OutOfBounds);
        }
        let len = usize::try_from(to - from).map_err(|_| FlashError::OutOfBounds)?;
        let from = Self::address(from, len)?;
        let to = Self::address(to, 0)?;
        for address in from..to {
            // Writing 0xFF erases the byte only if it is not erased yet. The erase of `avr-hal`
            // spends 1.8 ms and a write cycle on every byte, even an erased one.
            self.eeprom.write_byte(address, 0xFF);
        }
        Ok(())
    }
}

impl embedded_storage_async::nor_flash::MultiwriteNorFlash for Flash {}

impl embedded_storage_async::nor_flash::NorFlashError for FlashError {
    fn kind(&self) -> embedded_storage_async::nor_flash::NorFlashErrorKind {
        match self {
            Self::OutOfBounds => embedded_storage_async::nor_flash::NorFlashErrorKind::OutOfBounds,
            Self::NotAligned => embedded_storage_async::nor_flash::NorFlashErrorKind::NotAligned,
        }
    }
}

/// Initializes the EEPROM-backed flash driver.
///
/// # Panics
///
/// Panics if the EEPROM peripheral was already taken.
#[must_use]
pub fn init(peripherals: &mut crate::dummy::OptionalPeripherals) -> Flash {
    let eeprom = peripherals
        .EEPROM
        .take()
        .expect("EEPROM peripheral is only initialized once")
        .into_inner();
    Flash::new(Eeprom::new(eeprom))
}
