---
title: Inputs and outputs
group: 6
order: 90
see: P06-01 P06-21 P06-00 topic:protection
---
Group P06 sets the signal wires between the drive and the rest of the machine. The digital inputs tell the drive what to do. The digital outputs tell the controller what the drive is doing.

## Optocouplers

Each digital input and output goes through an optocoupler. An optocoupler is a small LED and a light sensor in one package. The signal crosses a small gap as light. So the wires of the controller and the electronics of the drive have no electrical connection, and noise and faults cannot cross.

The settings talk about "optocoupler on" and "optocoupler off". On means that current flows through the LED of the input.

## Digital inputs

The drive has eight digital inputs, DI1 to DI8. Each input has two settings: the function and the active state. The function is what the input does, for example servo on. The active state is the signal that switches the function on.

| Input | Function | Active state | Factory function |
|---|---|---|---|
| DI1 | P06-01 | P06-00 | Servo on |
| DI2 | P06-03 | P06-02 | Alarm reset |
| DI3 | P06-05 | P06-04 | Forward limit |
| DI4 | P06-07 | P06-06 | Reverse limit |
| DI5 | P06-09 | P06-08 | External forward torque limit |
| DI6 | P06-11 | P06-10 | External reverse torque limit |
| DI7 | P06-13 | P06-12 | Control mode switch |
| DI8 | P06-17 | P06-16 | Position command clear |

Do not give the same function to two inputs.

## Digital outputs

The drive has five digital outputs, DO1 to DO5.

| Output | Function | Active state | Factory function |
|---|---|---|---|
| DO1 | P06-21 | P06-20 | Servo ready |
| DO2 | P06-23 | P06-22 | Holding brake release |
| DO3 | P06-25 | P06-24 | Alarm |
| DO4 | P06-27 | P06-26 | In position |
| DO5 | P06-29 | P06-28 | Torque limit active |

DO2 accepts only the functions 0 to 2.

## Wiring to LinuxCNC

A typical machine with step and direction uses only a few signals.

- The enable output of LinuxCNC goes to servo on (DI1). When LinuxCNC stops, it turns the servo off.
- The alarm output of the drive (DO3) goes to a fault input of LinuxCNC. When the drive stops with an alarm, LinuxCNC must stop all the axes.
- Alarm reset (DI2) is optional. LinuxCNC can reset an alarm after you repair the cause.

The alarm wire is the most important. If one drive stops while LinuxCNC continues, the other axes still move. This can damage the part, the tool or the machine.

## When a change takes effect

The settings of the digital inputs and outputs take effect after a power cycle. So save them to EEPROM, then turn the drive off and on.

## Analog inputs

The drive has two analog inputs for −10 V to +10 V. Speed mode and torque mode use them.

| Setting | Speed input | Torque input |
|---|---|---|
| Scale | P06-40 | P06-43 |
| Filter | P06-41 | P06-44 |
| Offset | P06-42 | P06-45 |
| Dead band | P06-46 | P06-47 |

A machine in position mode with step and direction does not use them.

P06-50 and P06-51 belong to the screwdriver sequence. See [Torque control and torque limits](topic:torque-control).
