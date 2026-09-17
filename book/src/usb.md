# Universal Serial Bus (USB)

Ariel OS integrates support for USB peripherals built into many microcontrollers.

## Hardware Support

### Supported USB Standards

Many microcontrollers supported by Ariel OS include a USB microcontroller peripheral that can be leveraged to build any USB device.
At the time of writing, most of them support USB 2.0, but their signaling rate is often limited to 12 Mbit/s, even though some also do support 480 Mbit/s.
The following table summarizes the standard signaling rates and their names:

| Signaling rate | Standard                                 |
| -------------: | ---------------------------------------- |
| 1.5 Mbits/s    | Low-Speed USB (now aka Basic-Speed USB)  |
| 12 Mbits/s     | Full-Speed USB (now aka Basic-Speed USB) |
| 480 Mbits/s    | Hi-Speed USB                             |

Some also support USB On-The-Go (OTG), allowing the device to also behave as a USB Targeted Host on the same USB receptacle (by alternatively switching between the peripheral and host roles).
Currently only the USB peripheral (device) role is supported by Ariel OS.
The USB host role is not supported.

### USB Microcontrollers Peripherals

Currently, Ariel OS applications can only make use of "generic" USB peripherals, that is, USB microcontroller peripherals that can be used to implement any USB device class.
Some microcontrollers also feature USB microcontroller peripherals that only support a fixed set of USB classes: e.g., multiple ESP32 MCUs comprise a USB CDC-ACM/JTAG peripheral, which can only be used for the standard [USB CDC-ACM][usb-cdc-acm-book-glossary] device class or a vendor-specific device class implementing JTAG access over USB.
The others may still be integrated by Ariel OS to implement specific functionality, like [logging][logging-transports-book].

> [!TIP]
> Development kits that support USB often feature two USB receptacles: one for the onboard [debug probe][debug-probes-book] (if there is one), and the other connected to the USB peripheral of the microcontroller.
> That second USB connection is usually called "user USB" to differentiate it from that of the debug probe and is the one that must be connected to the computer acting as USB host.

### USB 1.x/USB 2.0 Device Speed Identification Pull-up Resistors

The USB specifications require that the USB device advertise its supported speed to the USB host using a pull-up resistor on one of the data line.
A pull-up resistor[^usb-rpu] (Rpu) must be added on D+ to advertise support for Full-Speed USB, while Low-Speed requires one on D−.
Hi-Speed USB uses the same pull-up as Full-Speed, with additional software negotiation.

<!-- NOTE: See Table 9-1 of the USB 2.0 specification for the definition of "attached". -->
<!-- NOTE: ST AN4879 is also a useful reference. -->
If the USB device is always bus powered, these pull-up resistors may be always connected.
However, because devices are not allowed to supply current on the data lines when VBUS is not present (sections 7.1.5 and 7.2.1 of the [USB 2.0 specification][usb-2.0-spec]), self-powered devices must implement VBUS detection.
VBUS detection (aka VBUS sensing) involves monitoring whether VBUS is present, connecting the pull-up resistor when it is applied, and disconnecting it when VBUS is removed, within 10 seconds (see section 7.2.1 of the [USB 2.0 specification][usb-2.0-spec]).
To this effect, microcontroller USB peripherals often implement VBUS detection in hardware, and require a dedicated GPIO pin to monitor the state of VBUS.
If they do not, or if the pin is not connected on the board, VBUS detection must be implemented manually, to enable/disable the pull-up resistor as necessary.

<!-- NOTE:
HALs do not currently document the pull-up behavior:

- ESP: currently only have OTG-capable MCU peripherals, see section 32.4.2.1 of ESP32-S3 TRM v1.5.
- nRF: <https://github.com/embassy-rs/embassy/blob/3861d3088da30d40c777dc05d282352e68ec5511/embassy-nrf/src/usb/mod.rs#L220-L221>
- RP: <https://github.com/embassy-rs/embassy/blob/84444a19eb57d978e3c09fcdc8e60cdd7278eb03/embassy-rp/src/usb.rs#L347>
- STM32: <https://github.com/embassy-rs/embassy/blob/84444a19eb57d978e3c09fcdc8e60cdd7278eb03/embassy-stm32/src/usb/usb.rs#L539-L542>
-->
<!-- NOTE: For pull-up management with OTG, see section 72.4.4 of ST RM0456 Rev 6 for instance. -->
Ariel OS always uses the internal pull-up resistors when available in hardware, and supports enabling VBUS detection when supported by the hardware (so they are only connected when VBUS is present).
When the microcontroller USB peripheral in use is USB OTG-capable, the pull-up is always managed by hardware, as part of OTG's protocols.
Otherwise, if VBUS detection is required and is not made possible by the hardware, it must currently be implemented manually in the application.

<!-- NOTE: These resistance values show up in RP2040's datasheet regarding `RPU_OPT`. -->
[^usb-rpu]: Originally, the pull-up resistance was required to be 1.5 kΩ ±5% (see section 7.1.3 of the USB 1.0 specification, section 7.1.5 of the USB 1.1 specification, and section 7.1.5 of the [USB 2.0 specification][usb-2.0-spec]).
Following the [Resistor ECN][usb-2.0-spec], the resistance value is given more tolerance and is allowed to fall into two different ranges: one around 1.2 kΩ when the bus is idle, and the other around 2.3 kΩ when the upstream device (i.e., the USB host or hub) is transmitting.
The higher tolerance allowed integrating these pull-up resistors into the chip as internal resistors (as these have higher manufacturing tolerance than discrete resistors).

## Software Integration

Ariel OS provides support for the USB peripheral role through [`embassy-usb`][embassy-usb-docsrs], which provides a consistent API across the supported hardware.
It can be enabled with the `usb` [laze module][laze-modules-book], which is only made available when the board features a USB device port.

An instance of [`embassy_usb::Builder`][ariel-os-usbbuilder-rustdoc] is created by Ariel OS, on which support for well-known USB device classes can be added using a dedicated [Ariel OS task hook][task-attr-docs]:

```rust
#[ariel_os::task(autostart, usb_builder_hook)]
async fn main() {
    let mut usb_class = USB_BUILDER_HOOK
        .with(|builder| {
            // USB class constructor that mutates the builder.
        })
        .await;
}
```

Support for well-known USB device classes is provided by `embassy-usb`, which is re-exported as [`ariel_os::reexports::embassy_usb`][ariel-os-reexports-embassy-usb-rustdoc].
Custom USB device classes can also be implemented.
Additionally, multiple USB device classes can be added on the builder, to create a composite USB device.

### USB Device Classes

The table below lists some of the well-known USB device classes and how to use them in Ariel OS :

| Device class                             | How to use                                                                                                                                    |
| ---------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| [USB CDC-ACM][usb-cdc-acm-book-glossary] | Apply the [`CdcAcmClass`][ariel-os-embassy-usb-cdcacmclass-rustdoc] constructor on the builder                                                |
| [USB CDC-NCM][usb-cdc-ncm-glossary-book] | Enable the [`usb-ethernet` laze module][usb-ethernet-laze-module-book]                                                                        |
| USB HID                                  | Enable the `usb-hid` Cargo feature and apply the [`HidReaderWriter`][ariel-os-embassy-usb-hidreaderwriter-rustdoc] constructor on the builder |

[Other well-known device classes][ariel-os-embassy-usb-class-rustdoc] are supported, and it is also possible to implement custom ones.

### Device Configuration

[Configuration for the USB device][ariel-os-embassy-usb-config-rustdoc] created can be provided using the [`#[ariel_os::config(usb)]`][config-attr-macro-rustdoc] attribute.
In particular, it allows setting the Vendor ID (VID) and Product ID (PID), and the manufacturer and product names.

Additionally, some environment variables are used by [`embassy-usb`][ariel-os-reexports-embassy-usb-rustdoc] for configuration.
See its documentation for more.

### Clock Configuration

<!-- NOTE: THE STM32F4 MCUs do *not* support crystal-less USB. -->
As USB microcontroller peripherals rely on specific clock frequencies (to accommodate the signaling rates of USB), they are usually provided with a dedicated clock signal, that is often not shared with other peripherals.
Because USB requires accurate timings[^usb-timings-requirements], the clock source typically relies on a [crystal resonator][crystal-resonator-book] (or an [external crystal oscillator][external-crystal-oscillator]).
When that is not the case, the microcontroller must feature a clock recovery system that is able to recover a clock from the USB Start Of Frame (SOF) packets (sent by the USB host every 1 ms for Full-Speed USB) and that trims an internal oscillator, keeping it in sync with the USB host and thus enabling crystal-less USB.
Many STM32 MCUs feature such clock recovery system (CRS).

To be able to use USB, the [clock configuration][clock-tree-configuration-book] must enable and configure the clock source required for the USB microcontroller peripheral.
[The default clock configuration provided by Ariel OS][clock-tree-configuration-book] usually already configures it appropriately when made possible by the board.
Otherwise, an appropriate clock configuration must be [provided in the application][clock-tree-configuration-book].

[^usb-timings-requirements]: Full-Speed USB requires a bit rate accuracy of 2500 ppm, while Hi-Speed USB requires 500 ppm (see section 7.1.11 of the [USB 2.0 specification][usb-2.0-spec]).

[usb-cdc-acm-book-glossary]: ./glossary.md#usb-cdc-acm
[logging-transports-book]: ./logging.md#logging-transports
[debug-probes-book]: ./flashing-debugging.md#debug-interfaces-protocols-and-probes
[embassy-usb-docsrs]: https://docs.rs/embassy-usb/latest/embassy_usb/
[ariel-os-usbbuilder-rustdoc]: https://ariel-os.github.io/ariel-os/dev/docs/api/ariel_os/usb/type.UsbBuilder.html
[task-attr-docs]: https://ariel-os.github.io/ariel-os/dev/docs/api/ariel_os/attr.task.html
[ariel-os-reexports-embassy-usb-rustdoc]: https://ariel-os.github.io/ariel-os/dev/docs/api/ariel_os/reexports/embassy_usb/index.html
[ariel-os-embassy-usb-cdcacmclass-rustdoc]: https://ariel-os.github.io/ariel-os/dev/docs/api/ariel_os/reexports/embassy_usb/class/cdc_acm/struct.CdcAcmClass.html
[ariel-os-embassy-usb-hidreaderwriter-rustdoc]: https://ariel-os.github.io/ariel-os/dev/docs/api/ariel_os/reexports/embassy_usb/class/hid/struct.HidReaderWriter.html
[usb-ethernet-laze-module-book]: ./networking.md#network-link-selection
[usb-cdc-ncm-glossary-book]: ./glossary.md#usb-cdc-ncm
[ariel-os-embassy-usb-class-rustdoc]: https://ariel-os.github.io/ariel-os/dev/docs/api/ariel_os/reexports/embassy_usb/class/index.html
[ariel-os-embassy-usb-config-rustdoc]: https://ariel-os.github.io/ariel-os/dev/docs/api/ariel_os/reexports/embassy_usb/struct.Config.html
[config-attr-macro-rustdoc]: https://ariel-os.github.io/ariel-os/dev/docs/api/ariel_os/attr.config.html
[crystal-resonator-book]: ./clocks.md#piezoelectric-oscillators
[external-crystal-oscillator]: ./clocks.md#external-clock-signals
[clock-tree-configuration-book]: ./clocks.md#configuring-the-clock-tree
[usb-2.0-spec]: https://www.usb.org/document-library/usb-20-specification
[laze-modules-book]: ./build-system.md#laze-modules
