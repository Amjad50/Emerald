{{ #include ../../links.md }}

# Power

> This is implemented in [`power`][power_dev]<br>
> Check [Virtual Devices/Power](../virtual_devices/power.md) as well for more information.

## Shutdown/Restart Mechanism

The process to perform shutdown/restart on any system is quite simple in principle. The system needs to:
- Stop all processes.
- Unmount all filesystems and flush data to disk.
- Uninitialize devices and connected peripherals.
- Issue the reboot/shutdown command to the hardware components that can actually do the electrical side, those would be the firmware through ACPI or the PS2 interface.

### Start
The `shutdown`/`reboot` process is initiated by the user or the system itself. And it can be done
by calling [`power::start_power_sequence`][start_power_sequence].


### Stopping Processes
When the "power sequence" is started, the [`Scheduler`](../processes/scheduler.md) is informed
to not schedule anymore and stop all processes as they arrive for rescheduling.

Then the scheduler wait for all processes to exit,
and then returns to `kernel_main`, which will then call
[`power::finish_power_sequence`][finish_power_sequence] which continues the shutdown process below.

### Filesystem Unmounting
When all processes are stopped and cleaned up, we know that no
`File` is being used except the `log_file` (see [Logging](../logging/index.md)).

So, we flush the kernel log, close the file and at this point no more logging is stored in disk, but there
are still some logging messages shown on the screen.

Then, we unmount all filesystems.

### Device Uninitialization

> This is not implemented yet, TODO.

### Shutdown

For shutdown we use `ACPI` to issue a shutdown command to the hardware.

During system startup, we read the `AML` data and extract all the values of `\_Sx` available. These values
are used to transition the system to several types of sleep states.
For now, we are only using `S5` which is the shutdown state.

### Reboot
For reboot, we use the PS2 interface to issue a reset command.

We just execute the below command:
```asm
outb(0x64, 0xFE);
```

`ACPI` does support system reset by using `reset_register` and `reset_value` which can be provided.
But for my qemu environment, it is not provided, so for now the PS2 method is what works in most
cases I guess?
We can implement the ACPI reset later and use it when the hardware supports it.
