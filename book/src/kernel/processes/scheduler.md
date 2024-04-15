{{ #include ../../links.md }}

# Scheduler

> This is implemented in [`scheduler`][scheduler]

The `scheduler` is responsible for scheduling the processes, and managing the CPU time between them.

## Scheduling Algorithm

We are using priority-queue based approach for scheduling processes.

The queue order is determined by a value `priority_counter`, that starts at `u64::MAX`, its decremented by
a value generated from the process's priority level, higher priority level will decrease the value less, and thus staying
on top for more times.

Each time we schedule a process we perform the following:
- Check all waiting processes, and wake them if its time, currently, we have `ProcessState::WaitingForTime` and `ProcessState::WaitingForPid` states that support waiting.
- After waking them (moving them to `scheduled` list), pick the top scheduled process, and run it, moving it to the `running_and_waiting` list.
- If we have `exited` processes, handle notifying waiters and parents and remove the process. Its important we remove the process
here, since we can't do it while the process is running (still handling the `exit` syscall) since we still hold the virtual memory, deleting the process will free it up and cause a page fault.

Running the process is simple:
- copy the `context` of the `process` to the saved `context` of the `CPU`, see [processor saved state](../processor/index.md#saved-cpu-state), which will be used by the [scheduler interrupt](#scheduler-interrupt) to jump to it.
- Set the `pid` of the `process` to the `process_id` of the `CPU`.
- Mark the `process` as `ProcessState::Running`, and move it to the `running_and_waiting` list as mentioned.

## Yielding

When a `process` is running, it can yield to the scheduler through 2 ways now:
- **Timer**: The `APIC` timer, see [APIC](../processor/apic.md#interrupts), will interrupt the CPU every once in a while,
and this gives us preemptive multitasking.
- **System Call**: When a `syscall` is executed, after the `syscall`, we perform `yield` as well, see [syscalls](./syscalls.md).

When yielding, we perform the following:
- Save the `all_state` of the CPU to the `context` of the `process`, and this `all_state` comes from the interrupt, i.e. we can only yield when an interrupt occurs from that process, since we have to save the exact `cpu` before the interrupt.
- reschedule the `process`, by putting it in the `scheduled` list and fixing up the `priority_counter` to be similar to the top process. This is important, as if a process was sleeping for some time, we don't want it to hog the execution when it wakes up because at that point, its `priority_counter` will be much higher than any other process.

## Sleeping
    
When a `process` is running, it can sleep, and this is done through the `syscall` `sleep`, see [syscalls](./syscalls.md).
    
When sleeping, we perform the following:
- Mark the `process` as `ProcessState::WaitingForTime(deadline)`, 
  where `deadline` is the expected time to finish the sleep from the `current` time. 
  See [Clocks](../clocks/index.md).
- the process would already be in `running_and_waiting` list, so no movement is done here.

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

