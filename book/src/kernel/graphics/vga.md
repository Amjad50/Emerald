{{ #include ../../links.md }}

# VGA

> This is implemented in [`vga`][vga].

This is a basic graphics driver implementation.

We take the framebuffer info from `multiboot2` structure coming from [boot](../boot.md), 
and use it to write to that memory, which is then displayed on the screen.

This framebuffer is controlled by the kernel, the kernel can use it internally to display stuff, for example the [console](../virtual_devices/console.md) uses it to display characters on the screen.

But then, the userspace processes can take ownership of the framebuffer, what this means is that the kernel
will stop rendering to it, but still owns the memory region. At this stage the kernel will just stay there waiting
for rendering commands coming from the owner process.

The rendering commands here are just `Blit`, which is an operation that copies a region from one framebuffer
(user allocated) to another (the vga framebuffer, that the kernel owns).

Which means that all the rendering is done by the userspace processes, and the kernel just copies 
the images to the screen.

Userspace processes can be more efficient by telling the kernel which regions of the framebuffer have changed
and only sending those regions to the kernel, so the kernel can copy only the changed regions to the screen.

These operations are accessible by the [`graphics` syscall](../processes/syscalls.md#syscalls-list)

## Graphics Command
There are 4 commands supported:
- `TakeOwnership`: This is used to take ownership of the graphics device.
- `ReleaseOwnership`: This is used to release ownership of the graphics device, and is executed automatically when the process exits if it was not released manually.
- `GetFrameBufferInfo(&mut info_out)`: This is used to get information about the framebuffer, see [FrameBufferInfo](https://docs.rs/emerald_std/latest/emerald_std/graphics/struct.FrameBufferInfo.html).
- `Blit(&BlitCommand)`: This is used to blit a region from userspace memory into the graphics framebuffer, it can control (See [BlitCommand](https://docs.rs/emerald_std/latest/emerald_std/graphics/struct.BlitCommand.html) for more info):
    - `src_framebuffer`: memory reference to the source framebuffer (only read by the kernel)
    - `src_framebuffer_info`: The framebuffer info of the source framebuffer, i.e. its shape in the memory, so that
      we can copy correctly from it.
    - `src_x`, `src_y`: The top-left corner of the source region (user memory)
    - `dest_x`, `dest_y`: The top-left corner of the destination region (kernel)
    - `width`, `height`: The width and height of the region to copy, applies to both