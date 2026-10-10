#![expect(
    unsafe_code,
    reason = "uses `core::mem::transmute` to type-erase AVR pin input mode (PhantomData only), and accesses memory-mapped I/O registers"
)]
//! Provides GPIO access for AVR microcontrollers (ATmega328P / Arduino Uno).
//!
//! Wraps [`avr_hal_generic::port::Pin`] with type-erased modes and a lifetime
//! parameter so that HAL-generic code in [`ariel_os::gpio`] can operate on
//! AVR pins just like on cortex-m or RISC-V targets.
//!
//! When `external-interrupts` is enabled, interrupt-backed inputs use the
//! ATmega328P pin-change interrupt groups. The interrupt handlers only record
//! transitions and wake the registered Embassy task; the future checks the
//! requested level or edge when it is polled.

pub mod input {
    //! Input-specific types.

    #[cfg(feature = "external-interrupts")]
    use core::cell::Cell;
    #[cfg(feature = "external-interrupts")]
    use core::future::poll_fn;
    use core::marker::PhantomData;
    #[cfg(feature = "external-interrupts")]
    use core::task::{Poll, Waker};

    use ariel_os_embassy_common::gpio::{Pull, input::Error};
    // `InterruptError` only exists with this feature, and only `reserve()` below returns it.
    #[cfg(feature = "external-interrupts")]
    use ariel_os_embassy_common::gpio::input::InterruptError;
    #[cfg(feature = "external-interrupts")]
    use portable_atomic::{AtomicU8, Ordering};

    #[cfg(feature = "external-interrupts")]
    use crate::peripherals::{
        PB0, PB1, PB2, PB3, PB4, PB5, PB6, PB7, PC0, PC1, PC2, PC3, PC4, PC5, PC6, PD0, PD1, PD2,
        PD3, PD4, PD5, PD6, PD7,
    };

    // Re-export the HAL-specific pin mode types.
    #[doc(hidden)]
    pub use embassy_avr::hal::port::mode;

    /// AVR's [`InputPin`](embassy_avr::hal::port::PinOps) trait is implemented by every concrete
    /// pin marker type (`PB5`, `PC0`, etc.) via the blanket `PinOps` impl. We use a trait
    /// alias to match the HAL-generic surface expected by [`ariel_os_hal::gpio`].
    #[doc(hidden)]
    pub trait InputPin:
        embassy_avr::hal::port::PinOps<Dynamic = embassy_avr::mcu::port::Dynamic>
    {
    }

    #[doc(hidden)]
    impl<T: embassy_avr::hal::port::PinOps<Dynamic = embassy_avr::mcu::port::Dynamic>> InputPin for T {}

    /// Whether inputs support configuring whether a Schmitt trigger is enabled.
    ///
    /// ATmega328P inputs have a fixed Schmitt trigger; this cannot be disabled at runtime.
    pub const SCHMITT_TRIGGER_CONFIGURABLE: bool = false;

    /// Digital level of an input.
    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    #[cfg_attr(feature = "defmt", derive(defmt::Format))]
    pub enum Level {
        /// Digital low level.
        Low,
        /// Digital high level.
        High,
    }

    ariel_os_embassy_common::define_into_level!();

    /// Public input wrapper. Owns a type-erased [`Pin<mode::Input<...>, Dynamic>`] so that
    /// a single `Output<'a>` / `Input<'a>` type covers every concrete pin.
    pub struct Input<'a> {
        pin: embassy_avr::mcu::port::Pin<
            embassy_avr::hal::port::mode::Input<embassy_avr::hal::port::mode::AnyInput>,
            embassy_avr::mcu::port::Dynamic,
        >,
        _marker: PhantomData<&'a ()>,
    }

    impl<'a> Input<'a> {
        /// Check whether the input is driven high.
        #[must_use]
        pub fn is_high(&self) -> bool {
            self.pin.is_high()
        }

        /// Check whether the input is driven low.
        #[must_use]
        pub fn is_low(&self) -> bool {
            self.pin.is_low()
        }

        /// Returns the input level as a HAL-specific enum value.
        #[must_use]
        pub fn get_level(&self) -> Level {
            if self.is_high() {
                Level::High
            } else {
                Level::Low
            }
        }
    }

    impl embedded_hal::digital::ErrorType for Input<'_> {
        type Error = core::convert::Infallible;
    }

    impl embedded_hal::digital::InputPin for Input<'_> {
        fn is_high(&mut self) -> Result<bool, Self::Error> {
            Ok(Input::is_high(self))
        }

        fn is_low(&mut self) -> Result<bool, Self::Error> {
            Ok(Input::is_low(self))
        }
    }

    #[cfg(feature = "external-interrupts")]
    /// Identifies the pin-change interrupt group and bit for a concrete AVR pin marker.
    pub trait PinChangeId {
        /// Pin-change interrupt identity for this pin.
        const PIN_CHANGE: PinChange;
    }

    #[cfg(feature = "external-interrupts")]
    #[doc(hidden)]
    #[derive(Copy, Clone)]
    pub struct PinChange {
        group: u8,
        bit: u8,
    }

    #[cfg(feature = "external-interrupts")]
    impl PinChange {
        const fn new(group: u8, bit: u8) -> Self {
            Self { group, bit }
        }

        const fn index(self) -> usize {
            match self.group {
                0 => self.bit as usize,
                1 => 8 + self.bit as usize,
                2 => 16 + self.bit as usize,
                _ => unreachable!(),
            }
        }
    }

    #[cfg(feature = "external-interrupts")]
    macro_rules! impl_pin_change_id {
        ($($pin:ident => $group:expr, $bit:expr),* $(,)?) => {
            $(
                impl PinChangeId for $pin {
                    const PIN_CHANGE: PinChange = PinChange::new($group, $bit);
                }
            )*
        };
    }

    #[cfg(feature = "external-interrupts")]
    impl_pin_change_id! {
        PB0 => 0, 0, PB1 => 0, 1, PB2 => 0, 2, PB3 => 0, 3,
        PB4 => 0, 4, PB5 => 0, 5, PB6 => 0, 6, PB7 => 0, 7,
        PC0 => 1, 0, PC1 => 1, 1, PC2 => 1, 2, PC3 => 1, 3,
        PC4 => 1, 4, PC5 => 1, 5, PC6 => 1, 6,
        PD0 => 2, 0, PD1 => 2, 1, PD2 => 2, 2, PD3 => 2, 3,
        PD4 => 2, 4, PD5 => 2, 5, PD6 => 2, 6, PD7 => 2, 7,
    }

    /// Interrupt-enabled input wrapper.
    ///
    /// The underlying pin is type-erased, while `P` retains the concrete pin marker at the type
    /// level so the driver can select the correct pin-change interrupt bit.
    ///
    /// The concrete pin marker selects the PCINT group and bit at compile time.
    #[cfg(feature = "external-interrupts")]
    pub struct IntEnabledInput<'a, P: PinChangeId> {
        pin: embassy_avr::mcu::port::Pin<
            embassy_avr::hal::port::mode::Input<embassy_avr::hal::port::mode::AnyInput>,
            embassy_avr::mcu::port::Dynamic,
        >,
        _pin_type: PhantomData<P>,
        _marker: PhantomData<&'a ()>,
    }

    // Keep interrupt state out of each input value; the pin-change identity is type-level.
    #[cfg(feature = "external-interrupts")]
    const _: [(); core::mem::size_of::<Input<'static>>()] =
        [(); core::mem::size_of::<IntEnabledInput<'static, PD2>>()];

    #[cfg(feature = "external-interrupts")]
    impl<P: PinChangeId> IntEnabledInput<'_, P> {
        /// Check whether the input is driven high.
        #[must_use]
        pub fn is_high(&self) -> bool {
            self.pin.is_high()
        }

        /// Check whether the input is driven low.
        #[must_use]
        pub fn is_low(&self) -> bool {
            self.pin.is_low()
        }

        /// Returns the input level.
        #[must_use]
        pub fn get_level(&self) -> Level {
            if self.is_high() {
                Level::High
            } else {
                Level::Low
            }
        }

        /// Asynchronously waits until the input level is high.
        ///
        /// Returns immediately when the pin is already high.
        pub async fn wait_for_high(&mut self) {
            self.wait(WaitMode::High).await;
        }

        /// Asynchronously waits until the input level is low.
        ///
        /// Returns immediately when the pin is already low.
        pub async fn wait_for_low(&mut self) {
            self.wait(WaitMode::Low).await;
        }

        /// Asynchronously waits for a low-to-high transition.
        pub async fn wait_for_rising_edge(&mut self) {
            self.wait(WaitMode::Rising).await;
        }

        /// Asynchronously waits for a high-to-low transition.
        pub async fn wait_for_falling_edge(&mut self) {
            self.wait(WaitMode::Falling).await;
        }

        /// Asynchronously waits for any level transition.
        pub async fn wait_for_any_edge(&mut self) {
            self.wait(WaitMode::Any).await;
        }

        async fn wait(&mut self, mode: WaitMode) {
            let index = P::PIN_CHANGE.index();
            let rise_start = RISE_COUNTS[index].load(Ordering::Acquire);
            let fall_start = FALL_COUNTS[index].load(Ordering::Acquire);

            poll_fn(|cx| {
                WAKERS[index].register(cx.waker());

                let rising = count_changed(&RISE_COUNTS[index], rise_start);
                let falling = count_changed(&FALL_COUNTS[index], fall_start);
                let ready = match mode {
                    WaitMode::High => self.pin.is_high() || rising,
                    WaitMode::Low => self.pin.is_low() || falling,
                    WaitMode::Rising => rising,
                    WaitMode::Falling => falling,
                    WaitMode::Any => rising || falling,
                };

                if ready {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            })
            .await;
        }
    }

    #[cfg(feature = "external-interrupts")]
    impl<P: PinChangeId> Drop for IntEnabledInput<'_, P> {
        fn drop(&mut self) {
            release(P::PIN_CHANGE);
        }
    }

    #[cfg(feature = "external-interrupts")]
    impl<P: PinChangeId> embedded_hal::digital::ErrorType for IntEnabledInput<'_, P> {
        type Error = core::convert::Infallible;
    }

    #[cfg(feature = "external-interrupts")]
    impl<P: PinChangeId> embedded_hal::digital::InputPin for IntEnabledInput<'_, P> {
        fn is_high(&mut self) -> Result<bool, Self::Error> {
            Ok(IntEnabledInput::is_high(self))
        }

        fn is_low(&mut self) -> Result<bool, Self::Error> {
            Ok(IntEnabledInput::is_low(self))
        }
    }

    #[cfg(feature = "external-interrupts")]
    impl<P: PinChangeId> embedded_hal_async::digital::Wait for IntEnabledInput<'_, P> {
        async fn wait_for_high(&mut self) -> Result<(), Self::Error> {
            IntEnabledInput::wait_for_high(self).await;
            Ok(())
        }

        async fn wait_for_low(&mut self) -> Result<(), Self::Error> {
            IntEnabledInput::wait_for_low(self).await;
            Ok(())
        }

        async fn wait_for_rising_edge(&mut self) -> Result<(), Self::Error> {
            IntEnabledInput::wait_for_rising_edge(self).await;
            Ok(())
        }

        async fn wait_for_falling_edge(&mut self) -> Result<(), Self::Error> {
            IntEnabledInput::wait_for_falling_edge(self).await;
            Ok(())
        }

        async fn wait_for_any_edge(&mut self) -> Result<(), Self::Error> {
            IntEnabledInput::wait_for_any_edge(self).await;
            Ok(())
        }
    }

    /// Construct a new [`Input`] from a HAL-owned pin.
    ///
    /// The pull configuration is applied by calling the appropriate
    /// `into_*_input()` conversion on the underlying [`Pin`].
    #[doc(hidden)]
    pub fn new<'a, P: InputPin>(
        pin: crate::peripheral::Peri<'a, P>,
        pull: Pull,
        _schmitt_trigger: bool,
    ) -> Result<Input<'a>, Error> {
        let raw = pin.into_inner();
        let pin = match pull {
            Pull::None => unsafe {
                core::mem::transmute::<
                    embassy_avr::mcu::port::Pin<
                        embassy_avr::hal::port::mode::Input<embassy_avr::hal::port::mode::Floating>,
                        embassy_avr::mcu::port::Dynamic,
                    >,
                    embassy_avr::mcu::port::Pin<
                        embassy_avr::hal::port::mode::Input<embassy_avr::hal::port::mode::AnyInput>,
                        embassy_avr::mcu::port::Dynamic,
                    >,
                >(
                    embassy_avr::hal::port::Pin::new(raw)
                        .into_floating_input()
                        .downgrade(),
                )
            },
            Pull::Up => unsafe {
                core::mem::transmute::<
                    embassy_avr::mcu::port::Pin<
                        embassy_avr::hal::port::mode::Input<embassy_avr::hal::port::mode::PullUp>,
                        embassy_avr::mcu::port::Dynamic,
                    >,
                    embassy_avr::mcu::port::Pin<
                        embassy_avr::hal::port::mode::Input<embassy_avr::hal::port::mode::AnyInput>,
                        embassy_avr::mcu::port::Dynamic,
                    >,
                >(
                    embassy_avr::hal::port::Pin::new(raw)
                        .into_pull_up_input()
                        .downgrade(),
                )
            },
            Pull::Down => {
                // AVR pins only support pull-up; fall back to floating for pull-down requests.
                unsafe {
                    core::mem::transmute::<
                        embassy_avr::mcu::port::Pin<
                            embassy_avr::hal::port::mode::Input<
                                embassy_avr::hal::port::mode::Floating,
                            >,
                            embassy_avr::mcu::port::Dynamic,
                        >,
                        embassy_avr::mcu::port::Pin<
                            embassy_avr::hal::port::mode::Input<
                                embassy_avr::hal::port::mode::AnyInput,
                            >,
                            embassy_avr::mcu::port::Dynamic,
                        >,
                    >(
                        embassy_avr::hal::port::Pin::new(raw)
                            .into_floating_input()
                            .downgrade(),
                    )
                }
            }
        };

        Ok(Input {
            pin,
            _marker: PhantomData,
        })
    }

    #[cfg(feature = "external-interrupts")]
    #[doc(hidden)]
    pub fn new_int_enabled<'a, P>(
        pin: crate::peripheral::Peri<'a, P>,
        pull: Pull,
        schmitt_trigger: bool,
    ) -> Result<IntEnabledInput<'a, P>, Error>
    where
        P: InputPin + PinChangeId,
    {
        reserve(P::PIN_CHANGE)?;

        let input = match new(pin, pull, schmitt_trigger) {
            Ok(input) => input,
            Err(error) => {
                release(P::PIN_CHANGE);
                return Err(error);
            }
        };

        Ok(IntEnabledInput {
            pin: input.pin,
            _pin_type: PhantomData,
            _marker: PhantomData,
        })
    }

    #[cfg(feature = "external-interrupts")]
    #[derive(Copy, Clone)]
    enum WaitMode {
        High,
        Low,
        Rising,
        Falling,
        Any,
    }

    /// Number of slots in the per-pin arrays below. `PinChange::index()` puts port B on 0..8,
    /// port C on 8..15 and port D on 16..24 (index 15 stays unused, since port C has 7 pins).
    #[cfg(feature = "external-interrupts")]
    const PIN_COUNT: usize = 24;

    #[cfg(feature = "external-interrupts")]
    static WAKERS: [WakerSlot; PIN_COUNT] = [const { WakerSlot::new() }; PIN_COUNT];
    #[cfg(feature = "external-interrupts")]
    static INTERRUPT_COUNTS: [AtomicU8; PIN_COUNT] = [const { AtomicU8::new(0) }; PIN_COUNT];
    #[cfg(feature = "external-interrupts")]
    static RISE_COUNTS: [AtomicU8; PIN_COUNT] = [const { AtomicU8::new(0) }; PIN_COUNT];
    #[cfg(feature = "external-interrupts")]
    static FALL_COUNTS: [AtomicU8; PIN_COUNT] = [const { AtomicU8::new(0) }; PIN_COUNT];
    #[cfg(feature = "external-interrupts")]
    static LAST_LEVELS: [AtomicU8; 3] = [const { AtomicU8::new(0) }; 3];

    #[cfg(feature = "external-interrupts")]
    fn count_changed(counter: &AtomicU8, start: u8) -> bool {
        counter.load(Ordering::Acquire) != start
    }

    #[cfg(feature = "external-interrupts")]
    fn reserve(pin: PinChange) -> Result<(), Error> {
        critical_section::with(|_| {
            let index = pin.index();
            let count = INTERRUPT_COUNTS[index].load(Ordering::Relaxed);
            if count == u8::MAX {
                return Err(InterruptError::IntChannelAlreadyUsed.into());
            }

            if count == 0 {
                if !group_has_active(pin.group) {
                    clear_pcif(pin.group);
                    init_snapshot(pin.group);
                }
                WAKERS[index].clear();
            }

            INTERRUPT_COUNTS[index].store(count + 1, Ordering::Relaxed);
            update_group_mask(pin.group);
            Ok(())
        })
    }

    #[cfg(feature = "external-interrupts")]
    fn release(pin: PinChange) {
        critical_section::with(|_| {
            let index = pin.index();
            let count = INTERRUPT_COUNTS[index].load(Ordering::Relaxed);
            if count == 0 {
                return;
            }

            if count == 1 {
                INTERRUPT_COUNTS[index].store(0, Ordering::Relaxed);
                update_group_mask(pin.group);
                WAKERS[index].clear();
            } else {
                INTERRUPT_COUNTS[index].store(count - 1, Ordering::Relaxed);
            }
        });
    }

    #[cfg(feature = "external-interrupts")]
    fn init_snapshot(group: u8) {
        let index = group as usize;
        LAST_LEVELS[index].store(read_port(group), Ordering::Release);
    }

    #[cfg(feature = "external-interrupts")]
    fn group_has_active(group: u8) -> bool {
        let (first, len) = match group {
            0 => (0, 8),
            1 => (8, 7),
            2 => (16, 8),
            _ => return false,
        };
        INTERRUPT_COUNTS[first..first + len]
            .iter()
            .any(|count| count.load(Ordering::Relaxed) != 0)
    }

    #[cfg(feature = "external-interrupts")]
    fn update_group_mask(group: u8) {
        let mut mask = 0;
        let first = match group {
            0 => 0,
            1 => 8,
            2 => 16,
            _ => return,
        };
        let len = if group == 1 { 7 } else { 8 };

        for bit in 0..len {
            if INTERRUPT_COUNTS[first + bit].load(Ordering::Relaxed) != 0 {
                mask |= 1 << bit;
            }
        }

        let regs = exint_regs();
        match group {
            0 => regs.pcmsk0().write(|w| {
                // SAFETY: `mask` contains only the eight PCINT0 bit flags.
                unsafe { w.bits(mask) }
            }),
            1 => regs.pcmsk1().write(|w| {
                // SAFETY: `mask` contains only the seven implemented PCINT1 bit flags.
                unsafe { w.bits(mask) }
            }),
            2 => regs.pcmsk2().write(|w| {
                // SAFETY: `mask` contains only the eight PCINT2 bit flags.
                unsafe { w.bits(mask) }
            }),
            _ => unreachable!(),
        };

        let pcicr = regs.pcicr().read().bits() & 0x07;
        let pcicr = if mask == 0 {
            pcicr & !(1 << group)
        } else {
            pcicr | (1 << group)
        };
        regs.pcicr().write(|w| {
            // SAFETY: only the three PCINT enable bits are modified.
            unsafe { w.bits(pcicr) }
        });
    }

    #[cfg(feature = "external-interrupts")]
    fn clear_pcif(group: u8) {
        exint_regs().pcifr().write(|w| {
            // SAFETY: writing one to a PCIFR bit clears that group's pending flag.
            unsafe { w.bits(1 << group) }
        });
    }

    #[cfg(feature = "external-interrupts")]
    fn read_port(group: u8) -> u8 {
        unsafe {
            match group {
                0 => (*embassy_avr::mcu::pac::PORTB::ptr()).pinb().read().bits(),
                1 => (*embassy_avr::mcu::pac::PORTC::ptr()).pinc().read().bits(),
                2 => (*embassy_avr::mcu::pac::PORTD::ptr()).pind().read().bits(),
                _ => 0,
            }
        }
    }

    #[cfg(feature = "external-interrupts")]
    fn exint_regs() -> &'static embassy_avr::avr_device::atmega328p::exint::RegisterBlock {
        // SAFETY: EXINT is a memory-mapped register block with a fixed address, and all access is
        // serialized by the AVR single hardware thread or a critical section.
        unsafe { &*embassy_avr::mcu::pac::EXINT::ptr() }
    }

    #[cfg(feature = "external-interrupts")]
    fn handle_group(group: u8) {
        clear_pcif(group);

        let current = read_port(group);
        let previous = LAST_LEVELS[group as usize].swap(current, Ordering::AcqRel);
        let changed = current ^ previous;
        if changed == 0 {
            return;
        }

        let first = match group {
            0 => 0,
            1 => 8,
            2 => 16,
            _ => return,
        };
        let len = if group == 1 { 7 } else { 8 };

        for bit in 0..len {
            let bit_mask = 1 << bit;
            if changed & bit_mask == 0 {
                continue;
            }

            let index = first + bit;
            if current & bit_mask != 0 {
                RISE_COUNTS[index].fetch_add(1, Ordering::Relaxed);
            } else {
                FALL_COUNTS[index].fetch_add(1, Ordering::Relaxed);
            }

            if INTERRUPT_COUNTS[index].load(Ordering::Relaxed) != 0 {
                WAKERS[index].wake();
            }
        }
    }

    #[cfg(feature = "external-interrupts")]
    /// Holds the waker for one GPIO pin.
    struct WakerSlot(critical_section::Mutex<Cell<Option<Waker>>>);

    #[cfg(feature = "external-interrupts")]
    impl WakerSlot {
        const fn new() -> Self {
            Self(critical_section::Mutex::new(Cell::new(None)))
        }

        fn register(&self, waker: &Waker) {
            critical_section::with(|cs| {
                let slot = self.0.borrow(cs);
                let registered = slot.take();
                slot.set(match registered {
                    Some(registered) if registered.will_wake(waker) => Some(registered),
                    _ => Some(waker.clone()),
                });
            });
        }

        fn wake(&self) {
            if let Some(waker) = critical_section::with(|cs| self.0.borrow(cs).take()) {
                waker.wake();
            }
        }

        fn clear(&self) {
            critical_section::with(|cs| {
                self.0.borrow(cs).take();
            });
        }
    }

    #[cfg(all(feature = "external-interrupts", feature = "arduino-uno"))]
    mod handlers {
        use super::handle_group;

        #[avr_device::interrupt(atmega328p)]
        fn PCINT0() {
            handle_group(0);
        }

        #[avr_device::interrupt(atmega328p)]
        fn PCINT1() {
            handle_group(1);
        }

        #[avr_device::interrupt(atmega328p)]
        fn PCINT2() {
            handle_group(2);
        }
    }
}

pub mod output {
    //! Output-specific types.

    use core::marker::PhantomData;
    use embedded_hal::digital::StatefulOutputPin;

    use ariel_os_embassy_common::gpio::Level;

    /// AVR's pin marker types (PB5, etc.) implement `PinOps`, which is the trait our HAL pin
    /// wrappers use to provide `OutputPin`/`InputPin`. We re-export it here as the
    /// HAL-generic `OutputPin` trait expected by [`ariel_os_hal::gpio`].
    #[doc(hidden)]
    pub trait OutputPin:
        embassy_avr::hal::port::PinOps<Dynamic = embassy_avr::mcu::port::Dynamic>
    {
    }

    impl<T: embassy_avr::hal::port::PinOps<Dynamic = embassy_avr::mcu::port::Dynamic>> OutputPin for T {}

    /// Whether outputs support configuring drive strength.
    ///
    /// ATmega328P outputs do not have configurable drive strength.
    pub const DRIVE_STRENGTH_CONFIGURABLE: bool = false;
    /// Whether outputs support configuring speed/slew rate.
    ///
    /// ATmega328P outputs do not have configurable slew rate.
    pub const SPEED_CONFIGURABLE: bool = false;

    /// Public output wrapper. Owns a type-erased `Pin<Output, Dynamic>` so that a single
    /// `Output<'a>` type covers every concrete pin.
    pub struct Output<'a> {
        pin: embassy_avr::mcu::port::Pin<
            embassy_avr::hal::port::mode::Output,
            embassy_avr::mcu::port::Dynamic,
        >,
        _marker: PhantomData<&'a ()>,
    }

    impl Output<'_> {
        /// Set the output high.
        pub fn set_high(&mut self) {
            self.pin.set_high();
        }

        /// Set the output low.
        pub fn set_low(&mut self) {
            self.pin.set_low();
        }

        /// Check whether the output is currently driven high.
        pub fn is_set_high(&self) -> bool {
            self.pin.is_set_high()
        }

        /// Check whether the output is currently driven low.
        pub fn is_set_low(&self) -> bool {
            self.pin.is_set_low()
        }

        /// Toggle the output level.
        pub fn toggle(&mut self) {
            self.pin.toggle();
        }
    }

    impl embedded_hal::digital::ErrorType for Output<'_> {
        type Error = core::convert::Infallible;
    }

    impl embedded_hal::digital::OutputPin for Output<'_> {
        fn set_high(&mut self) -> Result<(), Self::Error> {
            Output::set_high(self);
            Ok(())
        }

        fn set_low(&mut self) -> Result<(), Self::Error> {
            Output::set_low(self);
            Ok(())
        }
    }

    impl StatefulOutputPin for Output<'_> {
        fn is_set_high(&mut self) -> Result<bool, Self::Error> {
            Ok(Output::is_set_high(self))
        }

        fn is_set_low(&mut self) -> Result<bool, Self::Error> {
            Ok(Output::is_set_low(self))
        }
    }

    /// Construct a new [`Output`] from a HAL-owned pin.
    ///
    /// `drive_strength` and `speed` are accepted but ignored: ATmega328P has no configurable
    /// drive strength or slew rate. `initial_level` selects `into_output()` (low) or
    /// `into_output_high()` (high).
    #[doc(hidden)]
    #[allow(clippy::needless_pass_by_value)]
    pub fn new<'a, P: OutputPin>(
        pin: crate::peripheral::Peri<'a, P>,
        initial_level: Level,
        _drive_strength: super::DriveStrength,
        _speed: super::Speed,
    ) -> Output<'a> {
        let raw = pin.into_inner();
        let pin = embassy_avr::hal::port::Pin::new(raw);
        let pin = match initial_level {
            Level::High => pin.into_output_high().downgrade(),
            Level::Low => pin.into_output().downgrade(),
        };

        Output {
            pin,
            _marker: PhantomData,
        }
    }
}

/// Available drive strength settings.
///
/// *Note: ATmega328P outputs do not have configurable drive strength.*
#[derive(Copy, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum DriveStrength {
    /// Drive strength is fixed by the hardware.
    #[default]
    Fixed,
}

impl ariel_os_embassy_common::gpio::FromDriveStrength for DriveStrength {
    fn from(_drive_strength: ariel_os_embassy_common::gpio::DriveStrength<Self>) -> Self {
        Self::Fixed
    }
}

/// Available output speed/slew rate settings.
///
/// *Note: ATmega328P outputs do not have configurable slew rate.*
#[derive(Copy, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Speed {
    /// Slew rate is fixed by the hardware.
    #[default]
    Fixed,
}

impl ariel_os_embassy_common::gpio::FromSpeed for Speed {
    fn from(_speed: ariel_os_embassy_common::gpio::Speed<Self>) -> Self {
        Self::Fixed
    }
}
