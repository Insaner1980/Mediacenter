//! Phone directory handles; paths never cross the protocol boundary.
use super::{adapters::checked, protocol::*};
use crate::{
    contracts::AppError,
    library::{confirmation, files},
};
use std::{
    collections::HashMap,
    fs::File,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::ffi::OsStrExt,
    },
    path::{Path, PathBuf},
    sync::Arc,
};
const TTL_MS: u64 = 300_000;
const MAX_HANDLES: usize = 4096;
const PAGE_SIZE: usize = 100;
#[derive(Clone)]
pub struct Selection {
    root: PathBuf,
    relative: PathBuf,
    anchor: Arc<File>,
    root_identity: confirmation::MountConfirmation,
    identity: confirmation::MountConfirmation,
    expires_at: u64,
}
impl Selection {
    pub fn check_expiry(&self, now: u64) -> Result<(), AppError> {
        if now >= self.expires_at {
            return Err(missing());
        }
        Ok(())
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn path(&self) -> PathBuf {
        self.root.join(&self.relative)
    }
    /// Reopen from the held root descriptor, rejecting every symlink and mount crossing.
    pub fn revalidate(&self) -> Result<File, AppError> {
        if confirmation::capture(&self.root)? != self.root_identity {
            return Err(stale());
        }
        let file = open_beneath(&self.anchor, &self.relative)?;
        if confirmation::capture(&files::descriptor_path(&file))? != self.identity {
            return Err(stale());
        }
        Ok(file)
    }
}
#[derive(Clone)]
struct Handle {
    owner: Id,
    selection: Selection,
    expires: u64,
}
struct PageCursor {
    owner: Id,
    node: Id,
    after: std::ffi::OsString,
    expires: u64,
}
#[derive(Default)]
pub struct Folders {
    handles: HashMap<String, Handle>,
    cursors: HashMap<String, PageCursor>,
}
fn stale() -> AppError {
    AppError::new(
        "stale_state",
        "The folder changed. Browse and choose it again.",
    )
}
fn missing() -> AppError {
    AppError::new("not_found", "The folder handle expired. Browse again.")
}
fn opaque() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn open_beneath(root: &File, relative: &Path) -> Result<File, AppError> {
    if relative
        .components()
        .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(stale());
    }
    if relative.as_os_str().is_empty() {
        return Ok(root.try_clone()?);
    }
    let name = std::ffi::CString::new(relative.as_os_str().as_bytes()).map_err(|_| stale())?;
    #[repr(C)]
    struct How {
        flags: u64,
        mode: u64,
        resolve: u64,
    }
    let how = How {
        flags: (nix::libc::O_RDONLY | nix::libc::O_DIRECTORY | nix::libc::O_CLOEXEC) as u64,
        mode: 0,
        resolve: 0x08 | 0x04 | 0x02 | 0x01,
    };
    // SAFETY: the root descriptor is live; name/how buffers remain valid; fd has unique ownership.
    let fd = unsafe {
        nix::libc::syscall(
            nix::libc::SYS_openat2,
            root.as_raw_fd(),
            name.as_ptr(),
            &how,
            std::mem::size_of::<How>(),
        )
    };
    if fd < 0 {
        return Err(stale());
    }
    Ok(unsafe { File::from_raw_fd(fd as i32) })
}
impl Folders {
    fn prune(&mut self, now: u64) {
        self.handles.retain(|_, h| h.expires > now);
        self.cursors.retain(|_, h| h.expires > now);
    }
    fn insert(
        &mut self,
        owner: &Id,
        selection: Selection,
        label: &str,
        expires: u64,
    ) -> Result<DirectoryNode, AppError> {
        if self.handles.len() >= MAX_HANDLES {
            return Err(AppError::new(
                "busy",
                "Too many browsed folders. Wait for old choices to expire.",
            ));
        }
        let id = opaque();
        let label = label.chars().take(256).collect::<String>();
        let node = checked(serde_json::json!({"nodeId":id,"label":label})).map_err(|_| stale())?;
        self.handles.insert(
            id,
            Handle {
                owner: owner.clone(),
                selection,
                expires,
            },
        );
        Ok(node)
    }
    pub fn roots(
        &mut self,
        owner: &Id,
        roots: Vec<PathBuf>,
        now: u64,
    ) -> Result<DirectoryPage, AppError> {
        self.prune(now);
        self.handles.retain(|_, h| &h.owner != owner);
        self.cursors.retain(|_, h| &h.owner != owner);
        let mut items = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for root in roots {
            if items.len() == PAGE_SIZE {
                break;
            }
            if !seen.insert(root.clone()) || root.parent().is_none() {
                continue;
            }
            let Ok(file) = files::open_directory(&root, Path::new("")) else {
                continue;
            };
            let identity = confirmation::capture(&files::descriptor_path(&file))?;
            let label = root
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if label.starts_with('.') {
                continue;
            }
            let selection = Selection {
                root,
                relative: PathBuf::new(),
                anchor: Arc::new(file),
                root_identity: identity.clone(),
                identity,
                expires_at: now + TTL_MS,
            };
            items.push(self.insert(owner, selection, &label, now + TTL_MS)?);
        }
        Ok(DirectoryPage {
            items: checked(items).map_err(|_| stale())?,
            next_cursor: None,
        })
    }
    pub fn selection(&self, owner: &Id, node: &Id, now: u64) -> Result<Selection, AppError> {
        let handle = self
            .handles
            .get(node.as_str())
            .filter(|h| &h.owner == owner && now < h.expires)
            .ok_or_else(missing)?;
        handle.selection.revalidate()?;
        Ok(handle.selection.clone())
    }
    pub fn children(
        &mut self,
        owner: &Id,
        node: &Id,
        cursor: Option<&Cursor>,
        limit: Option<&PageLimit>,
        now: u64,
    ) -> Result<DirectoryPage, AppError> {
        self.prune(now);
        let selection = self.selection(owner, node, now)?;
        let expires = self.handles[node.as_str()].expires;
        let after = match cursor {
            None => None,
            Some(c) => Some(
                self.cursors
                    .get(c.as_str())
                    .filter(|p| &p.owner == owner && &p.node == node)
                    .ok_or_else(missing)?
                    .after
                    .clone(),
            ),
        };
        let file = selection.revalidate()?;
        let page_size = limit.map_or(PAGE_SIZE, |l| l.value() as usize);
        // Keep only the requested page and one lookahead, even for very large directories.
        let mut names = std::collections::BTreeSet::new();
        for entry in std::fs::read_dir(files::descriptor_path(&file))?.filter_map(Result::ok) {
            let name = entry.file_name();
            if name.as_bytes().starts_with(b".")
                || after.as_ref().is_some_and(|old| &name <= old)
                || !entry.file_type().is_ok_and(|t| t.is_dir())
            {
                continue;
            }
            names.insert(name);
            if names.len() > page_size + 1 {
                names.pop_last();
            }
        }
        let has_more = names.len() > page_size;
        let last = names.iter().nth(page_size - 1).cloned();
        let mut items = Vec::new();
        for name in names.into_iter().take(page_size) {
            let relative = selection.relative.join(&name);
            let Ok(child) = open_beneath(&selection.anchor, &relative) else {
                continue;
            };
            let child_selection = Selection {
                relative,
                identity: confirmation::capture(&files::descriptor_path(&child))?,
                ..selection.clone()
            };
            items.push(self.insert(owner, child_selection, &name.to_string_lossy(), expires)?);
        }
        let next_cursor = if has_more {
            if self.cursors.len() >= MAX_HANDLES {
                return Err(AppError::new("busy", "Too many folder pages."));
            }
            let id = opaque();
            self.cursors.insert(
                id.clone(),
                PageCursor {
                    owner: owner.clone(),
                    node: node.clone(),
                    after: last.expect("full page"),
                    expires,
                },
            );
            Some(checked(id).map_err(|_| stale())?)
        } else {
            None
        };
        Ok(DirectoryPage {
            items: checked(items).map_err(|_| stale())?,
            next_cursor,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authorized_roots_only_and_no_raw_paths() {
        let home = tempfile::tempdir().unwrap();
        let music = home.path().join("Music");
        std::fs::create_dir(&music).unwrap();
        std::fs::create_dir(home.path().join(".ssh")).unwrap();
        let page = Folders::default()
            .roots(&checked("phone").unwrap(), vec![music], 0)
            .unwrap();
        assert_eq!(page.items.as_slice().len(), 1);
        let json = serde_json::to_string(&page).unwrap();
        assert!(json.contains("Music"));
        assert!(!json.contains(&home.path().to_string_lossy().to_string()));
        assert!(!json.contains(".ssh"));
    }
}

#[cfg(test)]
mod security_tests {
    use super::*;
    fn owner() -> Id {
        checked("phone").unwrap()
    }
    #[test]
    fn handles_reject_paths_other_devices_and_expire_at_five_minutes() {
        let dir = tempfile::Builder::new()
            .prefix("folders-")
            .tempdir()
            .unwrap();
        let mut f = Folders::default();
        let root = f
            .roots(&owner(), vec![dir.path().into()], 10)
            .unwrap()
            .items
            .as_slice()[0]
            .node_id
            .clone();
        assert!(f.selection(&owner(), &root, 300009).is_ok());
        assert!(f.selection(&owner(), &root, 300010).is_err());
        assert!(f.selection(&checked("other").unwrap(), &root, 11).is_err());
        assert!(f
            .selection(&owner(), &checked("unknown-opaque").unwrap(), 11)
            .is_err());
        for raw in ["/etc", "../etc", "..", "home/Music"] {
            assert!(checked::<Id, _>(raw).is_err());
        }
    }
    #[test]
    fn children_hide_credentials_and_symlinks_and_page_at_one_hundred() {
        let dir = tempfile::Builder::new()
            .prefix("folders-")
            .tempdir()
            .unwrap();
        std::fs::create_dir(dir.path().join(".ssh")).unwrap();
        std::os::unix::fs::symlink("/etc", dir.path().join("escape")).unwrap();
        for i in 0..101 {
            std::fs::create_dir(dir.path().join(format!("Folder{i:03}"))).unwrap();
        }
        let mut f = Folders::default();
        let root = f
            .roots(&owner(), vec![dir.path().into()], 0)
            .unwrap()
            .items
            .as_slice()[0]
            .node_id
            .clone();
        let page = f.children(&owner(), &root, None, None, 1).unwrap();
        assert_eq!(page.items.as_slice().len(), 100);
        assert_eq!(
            f.children(&owner(), &root, page.next_cursor.as_ref(), None, 2)
                .unwrap()
                .items
                .as_slice()
                .len(),
            1
        );
        assert!(!serde_json::to_string(&page).unwrap().contains(".ssh"));
        assert!(f
            .children(
                &checked("other").unwrap(),
                &root,
                page.next_cursor.as_ref(),
                None,
                2
            )
            .is_err());
    }
    #[test]
    fn final_selection_rejects_symlink_replacement_of_node_and_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Music");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("Albums")).unwrap();
        let mut f = Folders::default();
        let node = f
            .roots(&owner(), vec![root.clone()], 0)
            .unwrap()
            .items
            .as_slice()[0]
            .node_id
            .clone();
        let child = f
            .children(&owner(), &node, None, None, 1)
            .unwrap()
            .items
            .as_slice()[0]
            .node_id
            .clone();
        let selection = f.selection(&owner(), &child, 2).unwrap();
        std::fs::rename(root.join("Albums"), root.join("old")).unwrap();
        std::os::unix::fs::symlink("/etc", root.join("Albums")).unwrap();
        assert!(selection.revalidate().is_err());
        assert!(f.selection(&owner(), &child, 3).is_err());
        std::fs::rename(&root, dir.path().join("old-root")).unwrap();
        std::fs::create_dir(&root).unwrap();
        assert!(f.selection(&owner(), &node, 4).is_err());
    }
}
