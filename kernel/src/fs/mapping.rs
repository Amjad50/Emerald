use alloc::{
    boxed::Box,
    collections::{btree_map, BTreeMap},
    sync::{Arc, Weak},
};
use tracing::info;

use crate::{
    io::NoDebug,
    sync::{once::OnceLock, spin::rwlock::RwLock},
};

use super::{
    path::{Component, Path, PathBuf},
    EmptyFileSystem, FileSystem, FileSystemError,
};

static FILESYSTEM_MAPPING: OnceLock<FileSystemMapping> = OnceLock::new();

/// Retrieves the mapping for the given path.
///
/// This function traverses the filesystem mapping tree to find the appropriate mapping node for the given path.
/// It returns a tuple containing the mapping path, the remaining path after the mapping, and the corresponding mapping node.
///
/// # Parameters
///
/// * `path`: A reference to the path for which the mapping needs to be retrieved.
///
/// # Returns
///
/// * `Ok`: If the mapping is found successfully, it returns a tuple containing the mapping path, the remaining path, and the mapping node.
/// * `Err`: If the mapping is not found or an error occurs, it returns a `FileSystemError`.
pub fn get_mapping(path: &Path) -> Result<(PathBuf, &Path, Arc<MappingNode>), FileSystemError> {
    FILESYSTEM_MAPPING.get().get_mapping(path)
}

/// Mounts a given filesystem at the specified path.
///
/// This function allows mounting a new filesystem at a specific path within the virtual filesystem.
/// If the path is "/", the provided filesystem becomes the root filesystem (if not already mounted).
/// If the path is not "/", the provided filesystem is mounted as a child of the filesystem at the specified path.
///
/// # Parameters
///
/// * `arg`: A reference to a string representing the path where the filesystem should be mounted.
///   The path must be absolute and not contain any ".." or "." components.
///
/// * `filesystem`: An `Arc` smart pointer to a trait object implementing the `FileSystem` trait.
///   This trait defines the behavior of the filesystem to be mounted.
///
/// # Returns
///
/// * `Ok(())`: If the filesystem is successfully mounted at the specified path.
///
/// * `Err(MappingError)`: If an error occurs during the mounting process.
///   The specific error can be one of the following:
///   - `MappingError::MustBeAbsolute`: If the provided path is not absolute.
///   - `MappingError::InvalidPath`: If the provided path contains ".." or "." components.
///   - `MappingError::PartOfParentNotMounted`: If the parent path of the provided path is not mounted.
///   - `MappingError::AlreadyMounted`: If the provided path is already mounted.
pub fn mount(arg: &str, filesystem: Arc<dyn FileSystem>) -> Result<(), MappingError> {
    let mapping = FILESYSTEM_MAPPING.get_or_init(FileSystemMapping::empty_root);

    if arg == "/" {
        mapping.set_root(filesystem)
    } else {
        mapping.mount(arg, filesystem)
    }
}

/// Unmounts all filesystems from the virtual filesystem.
/// This function removes all mounted filesystems from the virtual filesystem, effectively clearing
/// the filesystem mapping tree.
pub fn unmount_all() {
    // The `Drop` will call `unmount` for each filesystem
    FILESYSTEM_MAPPING.get().root.unmount_all(Path::new("/"));
}

/// Traverses the filesystem mapping tree and applies a handler function to all matching mappings.
///
/// This function iterates through the filesystem mapping tree, starting from the root, and applies a handler function
/// to all nodes whose paths match the provided input path. The matching is performed by comparing the components
/// of the input path with the components of the mapping paths.
///
/// # Parameters
///
/// * `inp_path`: A reference to the input path for which matching mappings need to be found.
///   The input path must be absolute and not contain any ".." or "." components.
///
/// * `handler`: A closure or function that takes a reference to a path and an `Arc` smart pointer to a trait object
///   implementing the `FileSystem` trait. This closure or function will be applied to each matching mapping.
///
/// # Returns
///
/// * `Ok(())`: If the traversal and application of the handler function are successful.
///
/// * `Err(FileSystemError)`: If an error occurs during the traversal or application of the handler function.
///   The specific error can be one of the following:
///   - `FileSystemError::MustBeAbsolute`: If the provided input path is not absolute.
///   - `FileSystemError::InvalidPath`: If the provided input path contains ".." or "." components.
pub fn on_all_matching_mappings(
    inp_path: &Path,
    handler: impl FnMut(&Path, Arc<dyn FileSystem>),
) -> Result<(), FileSystemError> {
    FILESYSTEM_MAPPING
        .get()
        .on_all_matching_mappings(inp_path, handler)
}

#[derive(Debug)]
pub enum MappingError {
    MustBeAbsolute,
    InvalidPath,
    PartOfParentNotMounted,
    AlreadyMounted,
}

impl From<MappingError> for FileSystemError {
    fn from(value: MappingError) -> Self {
        Self::MappingError(value)
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct MappingNode {
    filesystem: NoDebug<RwLock<Arc<dyn FileSystem>>>,
    parent: Weak<MappingNode>,
    children: RwLock<BTreeMap<Box<str>, Arc<MappingNode>>>,
}

impl MappingNode {
    fn check_and_treverse(
        &self,
        target: &Path,
        this_component: Component<'_>,
        handler: &mut dyn FnMut(&Path, Arc<dyn FileSystem>),
    ) -> Result<(), FileSystemError> {
        let mut components = target.components();

        // doesn't match anything from here, stop
        if components.next() != Some(this_component) {
            return Ok(());
        }

        if components.peek().is_none() {
            for (name, node) in self.children.read().iter() {
                node.treverse(name.into(), handler);
            }
        } else {
            for (name, node) in self.children.read().iter() {
                node.check_and_treverse(
                    components.as_path(),
                    Component::Normal(name.as_ref()),
                    handler,
                )?;
            }
        }

        Ok(())
    }

    fn treverse(&self, current_path: PathBuf, handler: &mut dyn FnMut(&Path, Arc<dyn FileSystem>)) {
        handler(&current_path, self.filesystem());

        for (name, node) in self.children.read().iter() {
            node.treverse(current_path.join(name.as_ref()), handler);
        }
    }

    pub fn try_find_child(&self, component_name: &str) -> Option<Arc<MappingNode>> {
        self.children.read().get(component_name).cloned()
    }

    pub fn filesystem(&self) -> Arc<dyn FileSystem> {
        self.filesystem.0.read().clone()
    }

    pub fn parent(&self) -> Option<Arc<MappingNode>> {
        self.parent.upgrade()
    }

    fn unmount_all(&self, this_name: &Path) {
        let mut children = self.children.write();
        while let Some((name, node)) = children.pop_first() {
            node.unmount_all(&this_name.join(name.as_ref()));
        }

        info!("Unmounting {}", this_name.display());
        let fs = core::mem::replace(&mut *self.filesystem.0.write(), Arc::new(EmptyFileSystem));
        assert_eq!(
            Arc::strong_count(&fs),
            fs.number_global_refs() + 1, // number of global refs + this one
            "Filesystem still in use"
        );
        fs.unmount();
    }
}

#[derive(Debug)]
struct FileSystemMapping {
    root: Arc<MappingNode>,
}

impl FileSystemMapping {
    fn empty_root() -> Self {
        Self {
            root: Arc::new(MappingNode {
                filesystem: NoDebug(RwLock::new(Arc::new(EmptyFileSystem))),
                parent: Weak::new(),
                children: RwLock::new(BTreeMap::new()),
            }),
        }
    }

    fn set_root(&self, filesystem: Arc<dyn FileSystem>) -> Result<(), MappingError> {
        // Only `EmptyFileSystem` does this
        if let Err(FileSystemError::FileNotFound) = self.root.filesystem().open_root() {
            // FIXME: very bad. Not sure if there is race condition here, seems very suspicious

            *self.root.filesystem.0.write() = filesystem;
            Ok(())
        } else {
            Err(MappingError::AlreadyMounted)
        }
    }

    fn get_mapping<'p>(
        &self,
        path: &'p Path,
    ) -> Result<(PathBuf, &'p Path, Arc<MappingNode>), FileSystemError> {
        let mut current = self.root.clone();
        // must start with `/`
        let mut mapping_path = PathBuf::from("/");

        let mut components = path.components();

        if components.next() != Some(Component::RootDir) {
            return Err(FileSystemError::MustBeAbsolute);
        }

        while let Some(component) = components.peek() {
            match component {
                Component::Normal(name) => {
                    if let Some(child) = current.try_find_child(name) {
                        mapping_path.push(name);
                        current = child;
                    } else {
                        break;
                    }
                }
                _ => {
                    break;
                }
            }

            // consume
            components.next();
        }

        Ok((mapping_path, components.as_path(), current))
    }

    fn on_all_matching_mappings(
        &self,
        path: &Path,
        mut handler: impl FnMut(&Path, Arc<dyn FileSystem>),
    ) -> Result<(), FileSystemError> {
        self.root
            .check_and_treverse(path, Component::RootDir, &mut handler)
    }

    fn mount<P: AsRef<Path>>(
        &self,
        arg: P,
        filesystem: Arc<dyn FileSystem>,
    ) -> Result<(), MappingError> {
        let mut components: super::path::Components = arg.as_ref().components();

        if components.next() != Some(Component::RootDir) {
            return Err(MappingError::MustBeAbsolute);
        }

        {
            // no `..` or `.` in the path
            if components
                .clone()
                .any(|c| !matches!(c, Component::Normal(_)))
            {
                return Err(MappingError::InvalidPath);
            }
        }

        let mut current_element = self.root.clone();

        let size = components.clone().count();

        for (i, component) in components.enumerate() {
            let Component::Normal(component_path) = component else {
                unreachable!("Already chacked all the components")
            };
            let is_last = i == size - 1;

            match current_element
                .clone()
                .children
                .write()
                .entry(component_path.into())
            {
                btree_map::Entry::Vacant(entry) => {
                    if is_last {
                        entry.insert(Arc::new(MappingNode {
                            filesystem: NoDebug(RwLock::new(filesystem)),
                            parent: Arc::downgrade(&current_element),
                            children: RwLock::new(BTreeMap::new()),
                        }));
                        return Ok(());
                    } else {
                        return Err(MappingError::PartOfParentNotMounted);
                    }
                }
                btree_map::Entry::Occupied(entry) => {
                    if is_last {
                        return Err(MappingError::AlreadyMounted);
                    } else {
                        current_element = entry.get().clone();
                    }
                }
            }
        }

        unreachable!("For some reason, it wasn't mounted")
    }
}
