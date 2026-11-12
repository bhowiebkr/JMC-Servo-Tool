---
title: Position control
group: 3
order: 60
see: P03-01 P03-09 P03-15 P03-06 topic:basics
---
Group P03 controls how the drive reads position commands and turns them into motor turns. For a CNC machine with step and direction, this is the most important group.

## Where the command comes from

P03-00 selects the source. 0 is the pulse input, which LinuxCNC uses. 1 is a position that a field bus sends.

## The pulse signals

| Format | Signal 1 | Signal 2 | P03-01 |
|---|---|---|---|
| Pulse and direction | Each pulse is one step | High or low sets the direction | 1 (the factory setting) |
| CW and CCW | Pulses for forward | Pulses for reverse | 2 |
| A/B quadrature | A square wave | A second square wave, 90° later or earlier | 0 |

The format must match the output of the controller. LinuxCNC with a step generator uses pulse and direction.

The drive has two pulse inputs. P03-02 selects one.

- The low-speed input accepts up to 500 kHz. It takes a 5 V or 24 V differential signal or an open-collector output.
- The high-speed input accepts up to 4 MHz. It takes a differential signal with a level of 3.3 V to 5 V.

If the axis moves the wrong way, set P03-03 to reverse the direction. This is easier than a change of the wires.

P03-04 filters noise on the pulse input. P03-43 selects which edge of the pulse the drive counts.

## Electronic gear: pulses for each turn

The electronic gear sets how far the motor turns for each pulse. The simple way is P03-09, the number of pulses for one motor turn. The factory value is 10000.

An example for a CNC axis:

- The ball screw moves 5 mm for each turn.
- Set P03-09 to 5000. Then one pulse moves the axis 5 mm ÷ 5000 = 0.001 mm.
- In LinuxCNC, the step scale is then 1000 steps for each mm.
- At 3000 rpm, the motor makes 50 turns each second. The pulse rate is 50 × 5000 = 250000 pulses each second, or 250 kHz.

More pulses for each turn give finer steps, but they need a higher pulse rate for the same speed. Make sure that the pulse rate at full speed is less than the limit of the input and of the controller.

If P03-09 is 0, the drive uses a gear ratio instead: P03-10 ÷ P03-11. A digital input can select a second ratio, P03-13 ÷ P03-14. P03-12 extends the numerator for very large ratios.

The gear settings take effect after a power cycle.

## In position

The drive sets an in-position output when the motor is near the end of a move. P03-06 is the distance that counts as near. P03-05 selects the condition.

The unit of P03-06 is encoder counts, not command pulses. The encoder gives 131072 counts in each turn. So the factory value of 100 counts is less than 0.3° of the motor shaft. On a 5 mm ball screw, that is less than 0.004 mm.

## The following error alarm

The following error is the gap between the commanded position and the real position. If it is larger than P03-15, the drive stops with AL.501.

With step and direction, LinuxCNC cannot see the real motor position. It sends the steps and trusts the drive. So P03-15 is your only protection if the axis jams, crashes or stalls. Without the alarm, LinuxCNC continues, and the part or the machine can be damaged.

The factory value is 30000. The unit is 10 pulses, so the limit is 300000 pulses. With 10000 pulses for each turn, that is 30 motor turns, which is very loose. A better value is 2 to 3 times the largest following error in your fastest normal move. You can see the following error on the Live tab.

## Command smoothing

P03-16 filters the position command. It rounds sharp changes in the command, so the motion is softer. But it also adds a delay. LinuxCNC already plans smooth motion, so keep this filter at 0 or small on a CNC machine.

## A second encoder: the linear scale

Some machines have a glass scale on the axis. P03-20 lets the position loop use this scale instead of the motor encoder. This is called full closed loop control. P03-30 and P03-31 set the signals of the scale.

P03-07 selects the format of the position feedback.

## Encoder output

The drive can repeat the encoder position on connector CN1 as A/B pulses. A controller can read these pulses to know the real position. P03-40 selects the signal, and P03-22, P03-23 and P03-25 set the pulse count. P03-41 and P03-42 set the polarity of the signals.

## Digital position commands

P03-45 and P03-46 apply when a field bus sends the position (P03-00 = 1).

## Gantry: two motors on one axis

Some machines drive one axis with two motors, one on each side. If one motor moves more than the other, the gantry twists. P03-50 to P03-55 let the drive keep two motors together.

On a LinuxCNC machine, LinuxCNC can control the two motors as two joints instead. Then each drive works alone, and these settings stay off.

## Homing

P03-60 to P03-70 control a homing function in the drive. The manual does not explain all the values. On a LinuxCNC machine, LinuxCNC does the homing, so these settings stay at their factory values.
