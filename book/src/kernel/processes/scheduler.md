{{ #include ../../links.md }}

# Scheduler

> This is implemented in [`scheduler`][scheduler]

The `scheduler` is responsible for scheduling the processes, and managing the CPU time between them.

## Scheduling Algorithm

Currently, the algorithm is very stupid and very bad. Not even round-robin.

Its like so:
- Go through all `processes`, if the `process` is one of the following, we will run it next:
    - `ProcessState::Scheduled` or
    - `ProcessState::WaitingForTime(deadline)`, and the `current` time is greater than or equal to `deadline`, this will have 
      more priority than `ProcessState::Scheduled`.
- If the process is `ProcessState::Yielded`, we will skip it, and mark it as `ProcessState::Scheduled`, for next time, and this what prevents us from running the same process forever and give us fake round-robin.
- If the process is `ProcessState::Exited`, we will remove it from the list of processes.

When we schedule a `process` with `ProcessState::Scheduled` (`ProcessState::WaitingForTime` that is done, i.e. running it next), this is what's done:
- copy the `context` of the `process` to the saved `context` of the `CPU`, see [processor saved state](../processor/index.md#saved-cpu-state), which will be used by the [scheduler interrupt](#scheduler-interrupt) to jump to it.
- Set the `pid` of the `process` to the `process_id` of the `CPU`.
- Mark the `process` as `ProcessState::Running`.

## Yielding

When a `process` is running, it can yield to the scheduler through 2 ways now:
- **Timer**: The `APIC` timer, see [APIC](../processor/apic.md#interrupts), will interrupt the CPU every once in a while,
and this gives us preemptive multitasking.
- **System Call**: When a `syscall` is executed, after the `syscall`, we perform `yield` as well, see [syscalls](./syscalls.md).

When yielding, we perform the following:
- Save the `all_state` of the CPU to the `context` of the `process`, and this `all_state` comes from the interrupt, i.e. we can only yield when an interrupt occurs from that process, since we have to save the exact `cpu` before the interrupt.
- Mark the `process` as `ProcessState::Yielded`.

## Sleeping
    
When a `process` is running, it can sleep, and this is done through the `syscall` `sleep`, see [syscalls](./syscalls.md).
    
When sleeping, we perform the following:
- Mark the `process` as `ProcessState::WaitingForTime(deadline)`, 
  where `deadline` is the expected time to finish the sleep from the `current` time. 
  See [Clocks](../clocks/index.md).
- Yield the `process` to the scheduler.

And then, in the scheduler, we handle sleeping processes (see [scheduling algorithm](#scheduling-algorithm)).


## Scheduler Interrupt

This is interrupt `0xFF`, See [interrupts](../processor/interrupts.md#interrupts-and-exceptions) for more information.

This interrupt is used to easily change the execution context of the current `cpu`.

When the interrupt is triggered, we do the following:
- The `cpu` must contain a `context`, which we will move to.
- We must be in the kernel of course, this is a private interrupt for the kernel, and the scheduler alone.
- Switch the `all_state` coming from the interrupt (which will be a point in the `schedule` function in the kernel, where we called this interrupt), and the `context` from the `cpu`, which will be the state of the `process` currently.

And with the last step, we achieve the context switch between two execution states, the `kernel` and the `process`.

### Why?

I found this solution that worked quite well for switch between contexts, is it the best? I don't know, but it works quite well now and is very stable.

