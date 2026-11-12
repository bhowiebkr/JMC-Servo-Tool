---
title: Protection and alarms
order: 120
see: P03-15 P04-05 P05-10 P01-40 P00-30 topic:inputs-outputs
---
A CNC machine can break a tool, ruin a part or hurt someone in a fraction of a second. The drive watches many things all the time. When something is wrong, it stops the motor and shows an alarm. This page explains the main protections and how to set them.

## What happens at an alarm

1. The drive stops the motor.
2. The front panel shows the alarm code, for example AL.501.
3. The alarm output changes state, so that the controller can stop the machine.
4. The drive stays stopped until you remove the cause and reset the alarm.

Some alarms need a power cycle before the drive runs again. Always find the cause before you reset an alarm.

## The main protections

| Protection | Setting | Alarm | What it catches |
|---|---|---|---|
| Following error | P03-15 | AL.501 | A jam, a crash, a stall or a move that is too fast |
| Overspeed | P04-05 | AL.420 | A runaway motor or a noisy step signal |
| Runaway | P01-40 | AL.422 | A motor that turns out of control |
| Torque limit | P05-10, P05-11 | No alarm. It limits the force. | Too much force in a crash |
| Overtravel | P01-10 | No alarm. It stops at the limit switch. | An axis at the end of its travel |
| Braking resistor | P00-30 | AL.402 | Too much energy from fast stops |
| Temperature | P00-40 | AL.440 | An overheated drive |
| Gantry difference | P03-53 | AL.510 | Two gantry motors out of line |

## The following error alarm protects the machine

With step and direction, LinuxCNC sends steps and never sees the real position. If an axis jams, LinuxCNC does not know. It continues to cut with the other axes.

The following error alarm is the protection for this case. Set P03-15 to 2 to 3 times the largest following error of your fastest normal move. The factory value allows 30 motor turns of error, which is much too loose for a CNC machine.

## Connect the alarm output

An alarm stops one drive only. The other drives continue unless the controller stops them. So connect the alarm output of each drive to the fault or emergency stop chain of LinuxCNC. See [Inputs and outputs](topic:inputs-outputs).

## How the motor stops

- With the dynamic brake, the motor stops faster. P01-21 to P01-24 select when the drive uses it.
- Without the dynamic brake, the motor coasts to a stop.
- A holding brake holds a vertical axis after the stop. See [Control mode and brakes](topic:control-modes).

## Common alarm codes

| Code | Meaning | First thing to check |
|---|---|---|
| AL.401 | The main voltage is too low | The main power supply |
| AL.402 | The main voltage is too high | The braking resistor and P00-30 |
| AL.410 to AL.412 | Overload | A jam, too much friction or too much acceleration |
| AL.420 | Overspeed | P04-05, the step signal and the motor wires |
| AL.440 | The drive is too hot | The cooling and the air flow |
| AL.501 | The following error is too large | A jam, P03-15, the acceleration and the torque limit |
| AL.505 | The pulse input is not correct | The wires of the step signal and its pulse rate |
| AL.610 to AL.652 | Encoder fault | The encoder cable, its connector and the encoder battery |
