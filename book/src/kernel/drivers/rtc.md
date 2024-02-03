{{ #include ../../links.md }}

# RTC

> This is implemented in [`rtc`][rtc].

The RTC (Real Time Clock) is a hardware clock that is used to provide the current time and date for the system.

We fetch the `RTC` time on startup in `kernel_main`, but currently, its not being used beside printing the time on the screen.
