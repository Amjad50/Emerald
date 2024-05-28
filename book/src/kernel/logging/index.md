# Logging

We are using [`tracing`] to implement logging in our kernel.

So, you can use `trace!`, `debug!`, `info!`, `warn!`, and `error!` macros to log messages.

Currently, we don't support spans yet.

As of now, the level of tracing is hardcoded to ignore `trace!` and `debug!` messages, but we want to add kernel command-line arguments to change the level of tracing.

The logs are also saved into a file that is replaced every time the kernel is booted. The file is `/kernel.log`.


[`tracing`]: https://docs.rs/tracing/latest/tracing/
