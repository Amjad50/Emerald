{{ #include ../../links.md }}

# Time Stamp Counter (TSC)

> This is implemented in [`tsc`][tsc].

The TSC is a 64-bit register present in all x86 processors since the Pentium.
It counts the number of cycles since the processor was reset.
It is used to provide a high-precision time base for the system.

But it doesn't count human time, so we have to calibrate it using a more accurate timely based clock.
Like the [HPET] or the [RTC], `RTC` is very slow (1 second interval), so we use the [HPET] for now to calibrate.

We also may need to recalibrate the `TSC` once a while, as it may drift.

[HPET]: ./hpet.md
[RTC]: ./rtc.md
