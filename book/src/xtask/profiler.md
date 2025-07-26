# Profiler

Profiling the kernel is one of the most important tasks we have in order to improve its performance.

I was thinking first, about making a trace of the kernel state interrupts, but that would have some issues:
- Hard to implement and export the data (maybe export to disk?).
- The analysis would also be part of the trace, which would make it more annoying to separate the real data from trace
  processing calls.

## Qemu (QMP)
Qemu has an interface to communicate and analyze the guest system from outside, called QMP (Qemu Machine Protocol).
It allows us to send commands to the Qemu instance and get the results back.

Such as:
- `query-cpus`: Get the current state of the CPUs.
- `stop`: Pause the Qemu instance.
- `cont`: Continue the Qemu instance.
- `human-monitor`: Send gdb like commands to the Qemu instance.
    - such as `info registers` to get the current state of the registers.
    - `x/1x ...` read memory at the given address.

Using all of that we have built a profiler in `xtask` that does the following:
- Read the dwarf information and the symbols information form the kernel binary, and store it.
- Connect to the Qemu instance using QMP.
- On every interval (configured)
    - Stop the Qemu instance. (to keep the memory state consistent, even though it can sample without stopping).
    - Get `RIP`, `RSP`, `RBP` registers of the CPU (currently, we have 1 CPU only).
    - Use [`framehop`](https://github.com/mstange/framehop) to trace the stack using these variables as well as the gdb command to
      read any memory address.
    - Use the symbols to resolve the function names for each stack frame.
    - Group them together in 1 collapsed stack trace.
- Collect all collapsed stack traces and their counts into one file.

The collected collapsed stacks are then used to generate flamegraph ([https://www.speedscope.app](https://www.speedscope.app) is a good tool for viewing these files).
