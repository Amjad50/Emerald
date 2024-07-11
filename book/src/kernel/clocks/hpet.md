{{ #include ../../links.md }}

# High Precision Event Timer (HPET)

> This is implemented in [`hpet`][kernel_hpet].

> See Intel's [High Precision Event Timer (HPET) Specification][HPET_spec] for more details.

The HPET is a hardware timer that is used to provide a high-precision time base for the system.

The other clocks such as [TSC] is calibrated using the `HPET`, then [TSC] is used to provide the time for the system as it is faster than the `HPET`.

Currently, we only use 1 timer for the clock, and we don't use the interrupts. But we could use it
in the future to provide a more accurate time based events.

If `HPET` is not available or the user has `allow_hpet=false` in the cmdline (see [Command Line]), `HPET` will be disabled, and we are going to use [PIT].

[HPET_spec]: http://www.intel.com/content/dam/www/public/us/en/documents/technical-specifications/software-developers-hpet-spec-1-0a.pdf
[PIT]: ./pit.md
[TSC]: ./tsc.md

