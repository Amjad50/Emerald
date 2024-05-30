{{ #include ../../links.md }}

# Processes

> This is implemented in [`process`][process]

This module, implements the process management of the kernel, including creating, destroying, and scheduling processes.
As well as managing the process's memory, and the process's resources.

Currently, when we say `process`, we also mean `thread`, as we don't have multi-threading support yet.

## Process Data
The process structure [`Process`][process_structure] contain all the information relating to the process, some of the important ones:
- `id`: The process id.
- `parent_id`: The id of the parent process.
- `vm`: The process's virtual memory, an instant of `VirtualMemoryMapper`, see [virtual mapper](../memory/virtual_mapper.md).
- `context`: A saved state of the CPU before the process is being scheduled. i.e. while the `process` is running, this
  is considered invalid and doesn't represent the current state of the process.
- `open_filesystem_nodes`: A map of open file nodes, see [filesystem](../filesystem/index.md) a node can be a file or a directory, the mapping here is from `usize`, we use map instead of a list since we can remove a file from the middle of the list, and we don't want to have to shift all the elements after it.
- `argv`: A string list of the arguments passed to the process.
- `stack_ptr_end`: The end of the stack, the stack grows down, so this is the highest address of the stack, and where the stack starts when the process is created.
- `stack_size`: The current size of the stack, currently, its constant, until we get growing stack support.
- `heap_start`: The start address of the heap, this will be padded by around `1MB` from the end of the `ELF` file loaded into memory.
- `heap_size`: The current size of the heap. The user process can request more heap space with the [`inc_dec_heap`](./syscalls.md#syscalls-list) system call.
- `heap_max`: The maximum possible size of the heap, this is not changed, currently set to `1GB`.
- `priority`: The priority of the process, this is used by the scheduler. see [`PriorityLevel`](https://docs.rs/emerald_kernel_user_link/latest/emerald_kernel_user_link/process/enum.PriorityLevel.html).)
- `exit_code`: The exit code of the process, if the process is exited, this will be set to the exit code.
- `children_exits`: A list of the children processes that have exited, with their exit code (see #process-exit later for more information).

## Process Creation

Process creation (structure creation) is as follows:
- Load the `ELF` file, this doesn't load the whole thing, just the header to make sure its valid.
- Maps the stack region.
- Loads argv into the stack (check [argv structure](#argv-structure) for more information).
- Load `ELF` regions into memory.
- Load the `Process Metadata` structure (check [Process Metadata](#process-metadata-structure) for more information).
- Add process-specific kernel memory regions, like the kernel stack (**this must be done after loading the ELF, and last modification to the VM manually, because we can't switch to this VM after this point unless its by the scheduler, see the comments `process/mod.rs::allocate_process` for more details**)
- Add data about the heap, with size `0` and max size `1GB`, i.e. no memory allocated yet.
- Default `context` is created, everything is `0`, except for:
    - `rip`: The entry point of the `ELF` file.
    - `rsp`: The end of the stack.
    - `rflags`: enable the interrupts.
    - `cs`: The code segment for the `USER_RING` (see [GDT](../processor/gdt.md)).
    - `ss/ds`: The data segment for the `USER_RING` (see [GDT](../processor/gdt.md)).
    - `rdi`: The value of `argc`, since we use `SYSV` calling convention from the userspace.
    - `rsi`: The address of the `argv` array, since we use `SYSV` calling convention from the userspace.

### Argv Structure
The `argv` structure in the stack is as follows:

```txt
+-------------------+   <- stack_ptr_end
| arg0              |
+-------------------+
| arg1              |
+-------------------+
| ....              |
+-------------------+
| null              |
+-------------------+
| argv              |   <- pointer to `arg0`
+-------------------+
| argc              |   <- number of arguments
+-------------------+
| ....              |   <- this is where `rsp` will be set to, when the process is created
+-------------------+
```

### Process Metadata Structure

> Structure definition at [`ProcessMetadata`](https://docs.rs/emerald_kernel_user_link/latest/emerald_kernel_user_link/process/struct.ProcessMetadata.html).

This structure is placed in a static location in userspace memory (see [user memory layout](../memory/memory_layout.md#user-layout)), and is used to store information about the process, such as:
- process id.
- image base address.
- image size.
- program header offset (even though it can probably be obtained by reading the image header).
- `eh_frame` address and size, this is used to implement unwinding.
- `text` address and size, this is useful for debugging and getting backtrace from usermode.

## Process Exit

When the syscall `exit` is called, the process moved to `exited` list, and the exit code is set.

The `Exited` process will be removed from the `scheduler`'s list, at the next `schedule` call, see [scheduler](./scheduler.md) for more information.

When the process exits, it does the following as well:
- It will notify all processes that are in the state `WaitingForPid` with the process's id, it will give it the `exit_code`, and continue those processes.
- It will add itself to the parent's `children_exits` list, with the `exit_code`, so that parents can know when their children have exited without blocking on `WaitingForPid` (i.e. they can call `waitpid` without blocking, only because they are parents).
