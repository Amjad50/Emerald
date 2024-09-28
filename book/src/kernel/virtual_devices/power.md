{{ #include ../../links.md }}

# Power

> This is implemented in [`power`][power_dev]

This is a very basic virtual device accessible from `/devices/power`, and it is used
to issue a power related event, for now `shutdown` or `reboot`.

Basic usage will be `echo "shutdown" > /devices/power` or `echo "reboot" > /devices/power`.

This will make the kernel shutdown or reboot the system.
