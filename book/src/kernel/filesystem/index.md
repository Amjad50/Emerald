{{ #include ../../links.md }}

# Filesystem

> This is implemented in [`filesystem`][kernel_filesystem]

In this kernel, the filesystem is implemented by several layers, and components.

When you try to open a file path, the `filesystem` will look for the best entity that contains this path.
And this is achieved by the [mapping](#mapping) system.

Check [FAT] for more information about the FAT filesystem specifically.

Then, when you open a file, you can specify several flags (implemented in [`OpenOptions`][fs_open_options]):
- `read` - Open the file for reading.
- `write` - Open the file for writing.
- `create` - Create the file if it doesn't exist.
- `create_new` - Fail if the file exists.
- `truncate` - Truncate the file if it exists.
- `append` (implicit `write`) - Append to the file if it exists.

With these, you can create new files and choose which mode to open them with. Of course the filesystem may refuse to create the file if the
operation is not supported, such as with `/devices` directory mappings.

## Mapping

> This is implemented in [`fs::mapping`][kernel_fs_mapping]

The mapping, is a structure we use to map a path prefix to a `Filesystem`.
For example, currently we have the following mappings:
```
'/' -> FAT (filesystem backed by disk)
'/devices' -> Devices (a virtual filesystem)
```

When you open a path, it will find the best mapping, i.e. the longest prefix that matches the path.
Then will use the resulting [`Filesystem`][kernel_fs_trait] manager to open the file.

For example, if you open the path `/devices/console`, it will use the `Devices` filesystem manager to open the file `/console`.

Internally, this mapping is stored in a recursive tree structure of [`MappingNode`][kernel_fs_mapping_node],
each will contain:
- The [`Filesystem`][kernel_fs_trait] object.
- Weak ref to parent (to not get into trouble when dropping)
- childern BTreeMap (child component name -> [`MappingNode`][kernel_fs_mapping_node])

So, it will be something like this:
```txt
- / (root) = {
  fs: object
  parent: None
  children: {
    "devices": {
      fs: object
      parent: /
      children: {}
    }
  }
}
```

This is used so that we can treverse between two mappings easily.
i.e., if we are at the beginning of the mapping, and encountered `..` path, we can go back to the parent mapping.
Also, when going forward in mapping, we can check if the component is a child mapping of this mapping and switch to it easily.

With this treversal, we can build canonical path for a node.


### Filesystem trait

The "manager" here is an implementor of the [`Filesystem`][kernel_fs_trait] trait, which is a simple interface that controls all
the filesystem operations.

Operations supported are:
- `open_root` - Open the root directory, this is the entry point when treversing the filesystem.
- `read_dir` - Read the directory entries from a [`DirectoryNode`][kernel_fs_dirnode].
- `treverse_dir` - Look through the dir, and return `Node` that matches the entry name or error if not found.
- `create_node` - Create a new file or directory inside a [`DirectoryNode`][kernel_fs_dirnode].
- `read_file` - Read the file contents from a [`FileNode`][kernel_fs_filenode].
- `write_file` - Write the file contents to a [`FileNode`][kernel_fs_filenode].
- `flush_file` - Force the driver to flush the content to physical media (i.e. clear cache if any).
- `close_file` - Send a message that we are closing the file, if you notice, we don't have `open_file`, but instead, the user can 
  treverse the filesystem with `open_root` and `read_dir` until the file node is found, then it can be used directly. This function
  is used to alert the filesystem to clean up any resources that it might have allocated for this file.
- `set_file_size` - Set the file size to a custom value, this is similar to `truncate` in Unix systems, `write_file`, will increase
  the file size if needed.
- `unmount` - Unmount the filesystem, this is called when the filesystem is no longer needed, and it should clean up all resources.
  You might say we don't use `Drop`, but there are several reasons I went with this.
  - We can't add `Drop` as a trait dependancy to `Filesystem`, so I wanted something
    related to the trait itself.
  - Some filesystems might not be able to be dropped like the `DEVICES` global filesystem, but we still want to tell it to clean itself or the parts that can be cleaned before shutdown/reboot.

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
