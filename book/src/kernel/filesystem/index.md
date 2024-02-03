{{ #include ../../links.md }}

# Filesystem

> This is implemented in [`filesystem`][kernel_filesystem]

In this kernel, the filesystem is implemented by several layers, and components.

When you try to open a file path, the `filesystem` will look for the best entity that contains this path.
And this is achieved by the **mapping** system.

Check [FAT] for more information about the FAT filesystem.

## Mapping

The mapping, is a dictionary that maps a path prefix to a `Filesystem` manager.
For example, currently we have the following mappings:
```
'/' -> FAT (filesystem backed by disk)
'/devices' -> Devices (a virtual filesystem)
```

When you open a path, it will find the best mapping, i.e. the longest prefix that matches the path.
Then will use the resulting `Filesystem` manager to open the file.

For example, if you open the path `/devices/console`, it will use the `Devices` manager to open the file `/console`.

## INode

> See [INode][kernel_inode]

The `Filesystem` will give us an `INode` when we open a directory or a file, and this `INode` gives information
about the file, as well as the `device` that it may contain.

> I'm calling `INode` even though [FAT] doesn't have this concept, but I'm using it to represent the file information.

## Partition tables

Currently we only support the [MBR][kernel_mbr] partition table, and we can only read the first partition, we don't check the partition type, and just forward it to the [FAT] filesystem.

## Devices

> See [Devices][kernel_devices_map]

This is a basic dictionary that maps a device name, to a `Arc<dyn Device>`. Then, when its opened, the device clone is
returned in a special `INode`.

[FAT]: ./fat.md
