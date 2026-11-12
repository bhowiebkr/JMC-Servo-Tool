---
title: Torque control and torque limits
group: 5
order: 80
see: P05-10 P05-11 P05-12 P05-02 topic:protection
---
Group P05 has two jobs. In torque mode, it sets the torque command. In every mode, it limits the torque.

## What torque means

Torque is the turning force of the motor. On a ball screw axis, more torque gives more pushing force on the table. The drive makes torque with current. Twice the current gives about twice the torque.

The settings give torque in percent of the rated torque (P00-02). 100 % is the torque that the motor can give all the time. For short times, the motor can give up to about 300 %.

## Torque limits: a slipping clutch

A torque limit works like a slipping clutch. The motor never pushes harder than the limit. In a crash, this limits the force on the machine.

| Setting | Direction | Active when |
|---|---|---|
| P05-10 | Forward | Always |
| P05-11 | Reverse | Always |
| P05-12 | Forward | A digital input with function 7 is on |
| P05-13 | Reverse | A digital input with function 8 is on |

P05-05 selects where the limit comes from. The factory setting uses these parameters.

A low limit has a cost. During a fast move, the motor needs a high torque to accelerate. If the limit is too low, the motor cannot follow, the following error grows, and the drive stops with AL.501. So set the limit above the torque that your fastest moves need. The Live tab shows the torque during a move.

The torque limit and the following error alarm (P03-15) work together. The limit keeps the force low in a crash. The alarm stops the machine when the axis cannot follow.

P05-06 delays the torque limit output. P05-07 corrects the torque if the real torque is different from the command.

## Torque mode

In torque mode (P01-01 = 2), the command is a torque, not a position. The motor pushes with this force, and the speed is whatever the load allows. Winders and presses use torque mode.

P05-00 selects the source of the command, and P05-03 is a fixed command. P05-02 limits the speed. A motor in torque mode with no load accelerates until something stops it.

## The screwdriver sequence

P05-08, P05-09 and P05-14 to P05-28 control a sequence that tightens screws. It runs up to four steps, each with its own speed and torque limit. Then it turns back a little. A CNC machine does not use it.
