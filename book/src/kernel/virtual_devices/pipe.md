{{ #include ../../links.md }}

# Pipe

> This is implemented in [`pipe`][pipe]

A pipe is a virtual device that allows two processes to communicate with each other.
It is a unidirectional communication channel. It is used to pass data from one process to another. 
t is similar to a file, but it is not stored on the disk. It is stored in the memory. It is a first-in-first-out (FIFO) data structure.

It acts as a special file, i.e. the process just write to it as a normal file.

It is created with [`create_pipe_pair`][create_pipe_pair], which will return 2 `File` objects,
one for reading and one for writing. The kernel then assign those to the process and such.

Internally, the `Pipe` is a `dyn Device`, so its stored in the `INode` as a device. See [filesystem](../filesystem/index.md#inode) for more details on `INode`.
