---
title: Motor and drive
group: 0
order: 30
see: P00-00 P00-21 P00-30 P00-40 topic:protection
---
Group P00 describes the hardware: the motor, the links to other devices, the braking resistor and the temperature protection. Most of these settings stay at their factory values. This page explains what they are, so that you know when to leave them alone.

## Motor data

The drive must know the motor that it drives. It needs the rated speed, the rated torque, the rated current, the rotor inertia and the number of magnetic poles. These values are in P00-01 to P00-17.

The motor code P00-00 is normally 2000. This code means that the motor has an absolute encoder with a memory. At power-on, the drive reads the motor data from the encoder, so you do not have to enter it.

Do not change the motor data. Wrong motor data can make the motor turn the wrong way, overheat or run out of control.

## Rated torque: the 100 % mark

Many settings give torque in percent. 100 % is the rated torque of the motor (P00-02). The rated torque is the torque that the motor can give all the time without overheating. For a short time, the motor can give much more, often up to 300 %.

So a torque limit of 200 % means twice the rated torque.

## The front panel display

P00-20 selects what the small display on the drive shows at power-on.

## Communication

The drive can talk to other devices in three ways.

| Port | Use | Parameters |
|---|---|---|
| RS232 | The link to a PC | P00-21 |
| RS485 (Modbus) | A link to a PLC, a touch panel or a PC | P00-23 to P00-26, P00-28, P00-29 |
| CAN | A field bus to a motion controller | P00-22, P00-27 |

This tool talks to the drive through a serial link. A wrong link setting can stop the link, and then you can only repair it from the front panel. For this reason, this tool does not let you change P00-21 to P00-26.

A machine with step and direction does not need these links to move. LinuxCNC sends the motion on the step and direction wires.

## The braking resistor

When a motor slows down, it works as a generator. The energy of the moving mass flows back into the drive. It charges the capacitors, and the voltage inside the drive rises.

The drive sends this energy into a braking resistor, which turns it into heat. If the resistor is too small or missing, the voltage rises too high. Then the drive stops with the alarm AL.402 (overvoltage).

| Setting | Meaning |
|---|---|
| P00-30 | Which resistor: internal, external or none |
| P00-31 | The power of the external resistor |
| P00-32 | The resistance of the external resistor |
| P00-33 | A check of the braking circuit |

A light CNC axis returns little energy, and the internal resistor is usually enough. Heavy axes, fast stops and vertical axes return more energy.

## Temperature and power protection

P00-40 selects the sensor that protects the drive from overheating. Keep it on. An overheated drive stops with AL.440.

P00-41 protects the drive when the control power fails. Keep it on.
