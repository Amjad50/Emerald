# Profiler

Profiling the kernel is one of the most important tasks we have in order to improve its performance.

I was thinking first, about making a trace of the kernel state interrupts, but that would have some issues:
- Hard to implement and export the data (maybe export to disk?).
- The analysis would also be part of the trace, which would make it more annoying to separate the real data from trace
  processing calls.

## Qemu (QMP)
Qemu has an interface to communicate and analyze the guest system from outside, called QMP (Qemu Machine Protocol).
It allows us to send commands to the Qemu instance and get the results back.

Such as:
- `stop`: Pause the Qemu instance.
- `cont`: Continue the Qemu instance.
- `human-monitor`: Send gdb like commands to the Qemu instance.
    - such as `info registers` to get the current state of the registers.
    - `x/1x ...` read memory at the given address.

Using all of that we have built a profiler in `xtask` that does the following:
- Read the dwarf information and the symbols information form the kernel binary, and store it.
- Load userspace program symbols from ELF files in the userspace output directory, using `.text` section hashes for identification.
- Connect to the Qemu instance using QMP.
- On every interval (configured)
    - Stop the Qemu instance. (to keep the memory state consistent, even though it can sample without stopping).
    - Get `RIP`, `RSP`, `RBP` registers of the CPU (currently, we have 1 CPU only).
    - Detect whether execution is in kernel or userspace based on the instruction pointer address.
    - For userspace execution: read process metadata, dynamically load userspace module symbols and unwinding info.
    - Use [`framehop`](https://github.com/mstange/framehop) to trace the stack using these variables as well as the gdb command to
      read any memory address.
    - Use the symbols to resolve the function names for each stack frame (both kernel and userspace).
    - Group them together in 1 collapsed stack trace with process identification.
- Collect all collapsed stack traces and their counts into one file.

The collected collapsed stacks are then used to generate flamegraph ([https://www.speedscope.app](https://www.speedscope.app) is a good tool for viewing these files).

## How to use

In order to profile the OS, the easiest way is to use `xtask` with profiling commands:

```sh
# Run the kernel in profiling mode
cargo xtask --profile run

# In another terminal, run the profiler
cargo xtask profiler -o stack.folded
```

## Features

### Kernel and Userspace Profiling
The profiler supports both kernel and userspace execution profiling:
- **Kernel profiling**: Traces execution within the kernel using kernel symbols and DWARF debug information
- **Userspace profiling**: Dynamically detects userspace processes and loads their symbols for accurate stack unwinding
- **Process identification**: Tracks individual processes by PID and program name in stack traces

### Symbol Resolution
- Loads ELF debug information from kernel binary at startup
- Caches userspace program symbols using `.text` section hashes for efficient lookup
- Falls back to raw addresses when symbols are unavailable (or couldn't load the process metadata)

### Stack Unwinding
- Uses the [`framehop`](https://github.com/mstange/framehop) library
- Supports x86_64 DWARF-based unwinding using `.eh_frame` sections
- Handles both kernel and userspace stack frames in mixed traces
- Dynamically switches between kernel and userspace unwinding contexts

### Sampling Modes
- **One-shot mode**: Captures a single stack trace for immediate analysis
- **Continuous sampling**: Collects samples over a specified duration with configurable intervals
- **Configurable timing**: Adjustable sampling interval (default 10ms) and duration (default 5s)

### Output Formats
- **Console output**: Real-time stack traces with statistics
- **Folded stack format**: Compatible with flamegraph tools for visualization
- **Process tracking**: Identifies samples by kernel vs userspace and specific process names

## Usage

The profiler is run through the `xtask` command:

```bash
# Basic profiling for 5 seconds
cargo xtask --profile profiler

# One-shot stack trace
cargo xtask --profile profiler --one-shot

# Extended sampling with custom parameters
cargo xtask --profile profiler --duration 30 --interval 5 --output profile.folded

# Verbose output with addresses
cargo xtask --profile profiler --verbose --show-addresses --one-shot
```

### Command Line Options
- `--qmp-socket <path>`: QMP socket path (default: ./qmp-socket)
- `--interval <ms>`: Sampling interval in milliseconds (default: 10)
- `--duration <sec>`: Sampling duration in seconds (default: 5)
- `--output <file>`: Output file for folded stack samples
- `--verbose`: Enable detailed output including timing information
- `--show-addresses`: Show raw addresses alongside symbols (only in `--one-shot` mode)
- `--one-shot`: Capture single stack trace and exit
- `--profile <mode>`: Override profile mode (debug/release/profile)
- `--kernel-only`: Only profile kernel execution, skip userspace
- `--user-only`: Only profile userspace execution, skip kernel

## Implementation Details

### Process Detection
The profiler distinguishes between kernel and userspace execution by examining the instruction pointer:
- Addresses ≥ `0xFFFFFFFF80000000` are considered kernel space
- Lower addresses trigger userspace process metadata collection

### Dynamic Symbol Loading
For userspace processes, the profiler:
1. Reads process metadata from a fixed kernel memory location
2. Extracts `.text` and `.eh_frame` sections from process memory
3. Computes a hash of the `.text` section for symbol cache lookup
4. Dynamically loads matching symbols and unwinding information
