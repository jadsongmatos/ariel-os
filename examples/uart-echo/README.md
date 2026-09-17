# UART echo example

## About

This application shows how to use the UARTs that are defined in a board's SBD descriptions.

## How to run

In this directory, run:

    laze build -b nrf52840dk run

The example initializes the host-facing UART (at 115200baud 8N1) and echoes all incoming bytes.
