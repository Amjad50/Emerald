{{ #include ../../links.md }}

# ACPI Machine Language (AML)

> This is implemented in [`aml`][kernel_aml].

`AML` is a language used to describe the hardware of the system, and it is used by the [ACPI](./index.md) to configure the system, check spec [here](https://uefi.org/htmlspecs/ACPI_Spec_6_4_html/20_AML_Specification/AML_Specification.html) (this is what's used to implement this parser).


This is an `AML` parser that is able to parse the code inside the `DSDT` and `SSDT` tables.

Currently, we do not use the parsed code, as we need to build an interpreter for it, so that we can emulate executing it
and get the data we need.

## Why this is needed?

There are some details of the hardware that is hidden in `AML`, such as:
- Finding out the sleep commands for the system, i.e. which registers to write to, and what values to write.
- Finding out interrupts configuration, for devices that share the same interrupt line.

Generally, most OSes will use [`acpica`] which is a tool that can parse and execute `AML` code.

## Some annoying parts

Just to share here, since this is a documentation and all.

Generally, parsing `AML` is not that hard, it's a binary format, we can read the `opcode` and know what term type we need.

The issue is one thing, `method calls`.

Method calls are encoded as such, `NameString` followed by `N` arguments as `TermArg` (which is a term type).
The issue is:
- The expression itself doesn't provide the number of arguments.
- The method call can happen before the method is defined (method definition provide number of arguments).
- The method can be external (I think).

So, we have one choice ([`acpica`] also does it) and that is to "**guess**" the number of arguments.
And this is very error-prone, as a `Term` can be also considered an expression, like `NameString` can be considered as
a variable if it is not a method call. *very messy :'(*

The current implementation I have I think is quite good, and I got to fix all the bugs I found, but of course there could be more.

> This seems kinda "**complaining**" XD, but I just wanted to share the experience.

[`acpica`]: https://acpica.org/