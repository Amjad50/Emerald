{{ #include ../../links.md }}

# FAT (File Allocation Table) filesystem

> This is implemented in [`fat`][kernel_fat]

The FAT filesystem is a simple filesystem that is widely used in many devices, such as USB drives, SD cards, and floppy disks.

In this kernel, we have support for:
- FAT12
- FAT16
- FAT32 
- Long file names (LFN).
- reading and writing files, changing file size, and creating files and directories.
