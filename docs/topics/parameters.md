---
title: Parameters
order: 20
see: topic:basics topic:tuning topic:protection
---
A parameter is one setting in the drive. Each parameter has a code, a value and a unit. Together, the parameters tell the drive how the machine is built and how it must move.

## How to read a code

A code like P03-15 has two parts.

- **03** is the group. A group holds the settings for one area of the drive.
- **15** is the number in the group.

The drive keeps each parameter at an address. The address is the group × 100 + the number. So P03-15 is at address 315.

## The groups

| Group | Area | Help page |
|---|---|---|
| P00 | Motor data, communication and the braking resistor | [Motor and drive](topic:motor-and-drive) |
| P01 | Control mode, auto-tuning and brakes | [Control mode and brakes](topic:control-modes) |
| P02 | Loop gains | [Control loops](topic:control-loops) |
| P03 | Position control and the step input | [Position control](topic:position-control) |
| P04 | Speed control and speed limits | [Speed control](topic:speed-control) |
| P05 | Torque control and torque limits | [Torque control and torque limits](topic:torque-control) |
| P06 | Digital and analog inputs and outputs | [Inputs and outputs](topic:inputs-outputs) |
| P08 | Inertia, filters and vibration | [Inertia and vibration](topic:vibration) |

The groups P07, P09 and P10 have no settings for normal use.

## Units and decimals

The drive keeps each value as a whole number. Some parameters have a decimal point. For example, the drive keeps 48.0 as 480. This tool shows the value with the decimal point and the unit, so you do not have to calculate.

Some units need care. P03-15 counts in steps of 10 pulses. P00-31 counts in steps of 10 W. The help page of each parameter tells you its unit.

## When a change takes effect

The facts at the top of each parameter page tell you when a change takes effect.

| Fact | Meaning |
|---|---|
| Takes effect now | The drive uses the new value at once. |
| Takes effect after a power cycle | The drive uses the new value after you turn it off and on again. |
| Change only with the servo off | The drive refuses a new value while the motor is energized. |

## RAM and EEPROM

The drive has two memories.

- RAM is the working memory. A change in RAM is lost when the power goes off.
- EEPROM is the permanent memory. At power-on, the drive copies the EEPROM into RAM.

This window writes to RAM only. So you can try a value, and a power cycle brings back the old value. To keep a value, save it to EEPROM with the command-line tool. Save only when you are sure that the value is good.

A parameter that takes effect after a power cycle needs an EEPROM save. The power cycle clears the RAM, so a value in RAM only never takes effect.

## Who can change what

| Access | Meaning |
|---|---|
| User | Normal settings for your machine. |
| Motor | Data for the motor. The factory sets it for the matched motor. Do not change it. |
| Factory | Internal settings of the drive. This tool hides them. To see them, use the View menu. |

## Defaults and spare slots

Each parameter has a factory default. The default suits many machines, but not all.

Some codes have no parameter. These are spare slots. The View menu can show them, but they do nothing.

## Change parameters safely

1. Save a snapshot of all the parameters first. Then you can go back.
2. Change one parameter at a time.
3. Test the machine after each change. Start with slow moves, and keep a hand near the emergency stop.
4. Write down what you changed and why.
