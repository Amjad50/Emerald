{{ #include ../../links.md }}

# Clocks

> This is implemented in [`clocks`][clocks].

Here we implement the clock functionality of the system.

This includes functionalities used in implementing:
- system time.
- timers.
- sleep (see [scheduler](../processes/scheduler.md#sleeping)).

We have several clock sources, and these are devices that provide us with a "**time**", this "**time**"
is not bound to a specific start, but it just guarantees that:
- The time is always increasing.
    - TODO: We still haven't handled wrap arounds.
- Querying the time at 2 points will give you the time difference based on real time speed.
    - This depends on the `granularity` of the source, for example, if we implement it for `RTC`,
      then the `granularity` is 1 second, i.e. querying it within 1 second will mostly give you
      the same time.

And thus with these, we can keep track of the time.

We have 2 **time**s in the system:
- boot time (uptime).
    - This is the time since the system booted.
    - This is calculated by periodically updating the time based on the best `clock source` we have.
- real time (unix time).
    - On boot, we get the current time from the [RTC].
    - When we need this value, we calculate it based on the `boot time` and the `start` time
      we got at   boot.

These times can be fetched with the `get_time` [syscall](../processes/syscalls.md#syscalls-list).

[RTC]: ./rtc.md

