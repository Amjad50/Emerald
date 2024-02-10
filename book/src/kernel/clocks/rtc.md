{{ #include ../../links.md }}

# RTC

> This is implemented in [`rtc`][rtc].

The RTC (Real Time Clock) is a hardware clock that is used to provide the current time and date for the system.

The `RTC` is used as the base time to determine the `unix` time of the system, and provide this time to the user space.

Beside that, the kernel generally doesn't care about `unix`, and everything is based on "system boot" time.

The `RTC` can technically be used as a clock source, such as [HPET] and [TSC], but its accuracy is very low, so for now its not used as such.

[HPET]: ../clocks/hpet.md
[TSC]: ../clocks/tsc.md
