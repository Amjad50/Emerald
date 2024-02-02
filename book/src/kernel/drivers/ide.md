{{ #include ../../links.md }}

# IDE

> This is implemented in [`ide_device`][ide_device]

IDE devices are the type of PCI device we support now. The PCI device type is `MassStorageController(0x01, ..)`.

We support both `ATA` and `ATAPI` devices. `ATA` devices are hard drives and `ATAPI` devices are CD/DVD drives.

Its basic support without DMA or `async`.

We perform read with [`read_sync`][ide_read_sync]
