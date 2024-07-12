{{ #include ../../links.md }}

# Command Line

The kernel receives a command line from the bootloader, where we can control
some aspects of the kernel boot behavior and enabled features.

## Command Line Format

The format is of
```
property1=value1 property2=value2,property3=value3, proerty4=value4
```

between properties, comma `,` can be used and/or space ` `.

Each property is key-value pair, a property can duplicated, in that case, the
last value will be used.

## Properties

> This is implemented in [`cmdline`][kernel_cmdline]
> implemented as
> ```rust
> cmdline_struct! {
>     pub struct Cmd<'a> {
>         #[default = true]
>         pub uart: bool,
>         #[default = 115200]
>         pub uart_baud: u32,
>         #[default = LogLevel::Info]
>         pub max_log_level: LogLevel,
>         #[default = "/kernel.log"]
>         pub log_file: &'a str,
>         #[default = true]
>         pub allow_hpet: bool,
>     }
> }
> ```

Here is the supported properties:


| Property        | Type                                       | Description                                              | Default          |
|-----------------|--------------------------------------------|----------------------------------------------------------|------------------|
| `uart`          | `bool`                                     | Enable UART/serial interface                             | `true`           |
| `uart_baud`     | `u32`                                      | UART baud rate                                           | `115200`         |
| `max_log_level` | `LogLevel` (`trace/debug/info/warn/error`) | Maximum log level                                        | `LogLevel::Info` |
| `log_file`      | `&str`                                     | Log file path                                            | `"/kernel.log"`  |
| `allow_hpet`    | `bool`                                     | Allow `HPET` (if present), otherwise always use `PIT`    | `true`           |
| `log_aml`       | `bool`                                     | Log the AML content as ASL code on boot from ACPI tables | `true`           |


If we write these in a command line, it will look like:
```
uart=true uart_baud=115200 max_log_level=info log_file=/kernel.log allow_hpet=true
```
