{{ #include ../../links.md }}

# FAT (File Allocation Table) filesystem

> This is implemented in [`fat`][kernel_fat]

The FAT filesystem is a simple filesystem that is widely used in many devices, such as USB drives, SD cards, and floppy disks.

In this kernel, we have FAT12, FAT16, and FAT32 support. Along with long file names (LFN) support.

We don't have write support yet, but we can read files and directories.
