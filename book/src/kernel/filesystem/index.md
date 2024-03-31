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
Then will use the resulting [`Filesystem`][kernel_fs_trait] manager to open the file.

For example, if you open the path `/devices/console`, it will use the `Devices` filesystem manager to open the file `/console`.

### Filesystem trait

The "manager" here is an implementor of the [`Filesystem`][kernel_fs_trait] trait, which is a simple interface that controls all
the filesystem operations.

Operations supported are:
- `open_root` - Open the root directory, this is the entry point when treversing the filesystem.
- `read_dir` - Read the directory entries from a [`DirectoryNode`][kernel_fs_dirnode].
- `create_node` - Create a new file or directory inside a [`DirectoryNode`][kernel_fs_dirnode].
- `read_file` - Read the file contents from a [`FileNode`][kernel_fs_filenode].
- `write_file` - Write the file contents to a [`FileNode`][kernel_fs_filenode].
- `close_file` - Send a message that we are closing the file, if you notice, we don't have `open_file`, but instead, the user can 
  treverse the filesystem with `open_root` and `read_dir` until the file node is found, then it can be used directly. This function
  is used to alert the filesystem to clean up any resources that it might have allocated for this file.
- `set_file_size` - Set the file size to a custom value, this is similar to `truncate` in Unix systems, `write_file`, will increase
  the file size if needed.

## Node

> See [Node][kernel_fs_node]

The `Filesystem` will give us an `Node` when we open a directory or a file, and this `Node` can be either [`FileNode`][kernel_fs_filenode] or [`DirectoryNode`][kernel_fs_dirnode]

> I'm calling `Node` even though [FAT] doesn't have this concept, but I'm using it to represent the file information.

## Partition tables

Currently we only support the [MBR][kernel_mbr] partition table, and we can only read the first partition, we don't check the partition type, and just forward it to the [FAT] filesystem.

## Devices

> See [Devices][kernel_devices_map]

This is a basic dictionary that maps a device name, to a `Arc<dyn Device>`. Then, when its opened, the device clone is
returned in a special [`FileNode`][kernel_fs_filenode], so we can act upon it as a file.

[FAT]: ./fat.md
