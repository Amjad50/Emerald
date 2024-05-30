# Programs

> These can be found in [userspace](https://github.com/Amjad50/Emerald/tree/master/userspace)

Here are the list of programs that are found in the userspace of the operating system by default.

## `init`

The first program that is run when the operating system boots, for now the operating system requires this program and expects it to be found at `/init`. 

Currently, `init` performs the following:
- Sets the `stdin` as blocking (will see why in a bit).
- Creates a new `/shell` process, using `stdin: Piped` and pass `stdout` and `stderr` normally inherited.
- Stays in the following loop:
    - Check if `/shell` has exited (not blocking).
    - Reads from `stdin` and buffers it until a newline is found, then it sends it to the pipe of `/shell`'s `stdin`, effectively, giving
    us behavior similar to a normal terminal in linux.
- If the process exits, it will spawn a new `/shell` process and goes back to the loop.

This is a temporary behavior (maybe?), but we still need to improve file operations as `init` is looping a lot.


## `shell`

This is a basic shell, that can change directories, and execute programs.

It also support output redirect (no piping between processes yet), so you can do something like:
```sh
ls > file.txt
```
or even append to a file:
```sh
ls >> file.txt
```

### List of commands/programs

| Name               | Description                                                   |
|--------------------|---------------------------------------------------------------|
| `cd` (internal)    | Change directory                                              |
| `pwd` (internal)   | Print working directory                                       |
| `exit` (internal)  | Exit the shell, which will just cause another to come back up |
| `touch` (internal) | Create a file, if not present                                 |
| `ls`               | List directory contents                                       |
| `tree`             | List directory contents recursively                           |
| `echo`             | Write arguments to the standard output                        |
| `cat`              | Print 1 file on the standard output (no concat yet XD)        |
| `xxd`              | Hexdump utility                                               |
| `keyboard`         | Keyboard test program                                         |
| `mouse`            | Mouse test program                                            |

## `graphics`
Here we have simple graphics programs that will take control of the graphics controller from the kernel and thus
will look like exiting from shell, upon exiting the program, the shell will come back up.

### List of commands/Programs
| Name       | Description                                                                                                                                                                                                                                                                                      |
|------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `graphics` | Simple graphics demo program, it will display a red ball and it will bounce around the screen                                                                                                                                                                                                    |
| `video`    | Video player, it will take a video in image zip format, that is a zip file with jpg images inside it, check [`tools/video_to_zip.sh`] for how to convert normal videos to this format. You can specify the fps upon creation (default is `30`), and specify it as will when running the program. |

[`tools/video_to_zip.sh`]: https://github.com/Amjad50/Emerald/blob/master/tools/video_to_zip.sh

