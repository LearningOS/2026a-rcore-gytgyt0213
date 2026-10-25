// AI 学习协助：本文件的实验增量由 AI 助手协助实现，逐处说明见 Study/修改记录。
// 参考：https://learningos.cn/rCore-Tutorial-Guide/chapter6/；原始代码来自课程模板，未改动官方评测器。
use super::{
    block_cache_sync_all, get_block_cache, BlockDevice, DirEntry, DiskInode, DiskInodeType,
    EasyFileSystem, DIRENT_SZ,
};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::{Mutex, MutexGuard};
/// Virtual filesystem layer over easy-fs
pub struct Inode {
    block_id: usize,
    block_offset: usize,
    fs: Arc<Mutex<EasyFileSystem>>,
    block_device: Arc<dyn BlockDevice>,
}

impl Inode {
    /// Count names in the flat root directory without changing the disk layout.
    fn count_links(&self, fs: &EasyFileSystem) -> u32 {
        let id = fs.get_inode_id(self.block_id, self.block_offset);
        let (block, offset) = fs.get_disk_inode_pos(0);
        get_block_cache(block as usize, Arc::clone(&self.block_device)).lock()
            .read(offset, |root: &DiskInode| {
                let mut count = 0;
                for i in 0..root.size as usize / DIRENT_SZ {
                    let mut entry = DirEntry::empty();
                    root.read_at(i * DIRENT_SZ, entry.as_bytes_mut(), &self.block_device);
                    if !entry.name().is_empty() && entry.inode_id() == id { count += 1; }
                }
                count
            })
    }

    /// Reclaim data, indirect blocks and the inode bitmap bit.
    fn reclaim(&self, fs: &mut EasyFileSystem) {
        self.modify_disk_inode(|inode| {
            for block in inode.clear_size(&self.block_device) { fs.dealloc_data(block); }
        });
        let id = fs.get_inode_id(self.block_id, self.block_offset);
        fs.inode_bitmap.dealloc(&self.block_device, id as usize);
    }

    /// Keep the inode alive for one open file description.
    pub fn open(&self) {
        let mut fs = self.fs.lock();
        let id = fs.get_inode_id(self.block_id, self.block_offset);
        *fs.open_counts.entry(id).or_insert(0) += 1;
    }

    /// Last close reclaims an inode whose final name was already removed.
    pub fn close(&self) {
        let mut fs = self.fs.lock();
        let id = fs.get_inode_id(self.block_id, self.block_offset);
        let count = fs.open_counts.get_mut(&id).unwrap();
        *count -= 1;
        if *count == 0 {
            fs.open_counts.remove(&id);
            if self.count_links(&fs) == 0 { self.reclaim(&mut fs); }
        }
        block_cache_sync_all();
    }

    /// Return the inode number, directory flag and live hard-link count.
    pub fn stat(&self) -> (u32, bool, u32) {
        let fs = self.fs.lock();
        (fs.get_inode_id(self.block_id, self.block_offset),
         self.read_disk_inode(|inode| inode.is_dir()), self.count_links(&fs))
    }

    /// Insert a directory entry, reusing a slot cleared by unlink.
    fn insert_entry(&self, name: &str, id: u32, fs: &mut MutexGuard<EasyFileSystem>) {
        self.modify_disk_inode(|root| {
            let count = root.size as usize / DIRENT_SZ;
            let mut slot = count;
            for i in 0..count {
                let mut entry = DirEntry::empty();
                root.read_at(i * DIRENT_SZ, entry.as_bytes_mut(), &self.block_device);
                if entry.name().is_empty() { slot = i; break; }
            }
            if slot == count { self.increase_size(((count + 1) * DIRENT_SZ) as u32, root, fs); }
            root.write_at(slot * DIRENT_SZ, DirEntry::new(name, id).as_bytes(), &self.block_device);
        });
    }

    /// Add another name for an existing regular file.
    pub fn link(&self, old: &str, new: &str) -> bool {
        if old == new || new.is_empty() || new.len() > 27 || new.contains('/') { return false; }
        let mut fs = self.fs.lock();
        let old_id = self.read_disk_inode(|root| self.find_inode_id(old, root));
        let Some(id) = old_id else { return false; };
        if self.read_disk_inode(|root| self.find_inode_id(new, root)).is_some() { return false; }
        self.insert_entry(new, id, &mut fs);
        block_cache_sync_all();
        true
    }

    /// Remove a name, deferring final reclamation while file descriptions exist.
    pub fn unlink(&self, name: &str) -> bool {
        if name.is_empty() { return false; }
        let mut fs = self.fs.lock();
        let Some(id) = self.read_disk_inode(|root| self.find_inode_id(name, root)) else { return false; };
        self.modify_disk_inode(|root| {
            for i in 0..root.size as usize / DIRENT_SZ {
                let mut entry = DirEntry::empty();
                root.read_at(i * DIRENT_SZ, entry.as_bytes_mut(), &self.block_device);
                if entry.name() == name {
                    root.write_at(i * DIRENT_SZ, DirEntry::empty().as_bytes(), &self.block_device);
                    break;
                }
            }
        });
        let (block, offset) = fs.get_disk_inode_pos(id);
        let inode = Self::new(block, offset, Arc::clone(&self.fs), Arc::clone(&self.block_device));
        if inode.count_links(&fs) == 0 && fs.open_counts.get(&id).copied().unwrap_or(0) == 0 {
            inode.reclaim(&mut fs);
        }
        block_cache_sync_all();
        true
    }
    /// Create a vfs inode
    pub fn new(
        block_id: u32,
        block_offset: usize,
        fs: Arc<Mutex<EasyFileSystem>>,
        block_device: Arc<dyn BlockDevice>,
    ) -> Self {
        Self {
            block_id: block_id as usize,
            block_offset,
            fs,
            block_device,
        }
    }
    /// Call a function over a disk inode to read it
    fn read_disk_inode<V>(&self, f: impl FnOnce(&DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .read(self.block_offset, f)
    }
    /// Call a function over a disk inode to modify it
    fn modify_disk_inode<V>(&self, f: impl FnOnce(&mut DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .modify(self.block_offset, f)
    }
    /// Find inode under a disk inode by name
    fn find_inode_id(&self, name: &str, disk_inode: &DiskInode) -> Option<u32> {
        // assert it is a directory
        assert!(disk_inode.is_dir());
        let file_count = (disk_inode.size as usize) / DIRENT_SZ;
        let mut dirent = DirEntry::empty();
        for i in 0..file_count {
            assert_eq!(
                disk_inode.read_at(DIRENT_SZ * i, dirent.as_bytes_mut(), &self.block_device,),
                DIRENT_SZ,
            );
            if !dirent.name().is_empty() && dirent.name() == name {
                return Some(dirent.inode_id() as u32);
            }
        }
        None
    }
    /// Find inode under current inode by name
    pub fn find(&self, name: &str) -> Option<Arc<Inode>> {
        let fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            self.find_inode_id(name, disk_inode).map(|inode_id| {
                let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
                Arc::new(Self::new(
                    block_id,
                    block_offset,
                    self.fs.clone(),
                    self.block_device.clone(),
                ))
            })
        })
    }
    /// Increase the size of a disk inode
    fn increase_size(
        &self,
        new_size: u32,
        disk_inode: &mut DiskInode,
        fs: &mut MutexGuard<EasyFileSystem>,
    ) {
        if new_size < disk_inode.size {
            return;
        }
        let blocks_needed = disk_inode.blocks_num_needed(new_size);
        let mut v: Vec<u32> = Vec::new();
        for _ in 0..blocks_needed {
            v.push(fs.alloc_data());
        }
        disk_inode.increase_size(new_size, v, &self.block_device);
    }
    /// Create inode under current inode by name
    pub fn create(&self, name: &str) -> Option<Arc<Inode>> {
        if name.is_empty() || name.len() > 27 || name.contains('/') { return None; }
        let mut fs = self.fs.lock();
        let op = |root_inode: &DiskInode| {
            // assert it is a directory
            assert!(root_inode.is_dir());
            // has the file been created?
            self.find_inode_id(name, root_inode)
        };
        if self.read_disk_inode(op).is_some() {
            return None;
        }
        // create a new file
        // alloc a inode with an indirect block
        let new_inode_id = fs.alloc_inode();
        // initialize inode
        let (new_inode_block_id, new_inode_block_offset) = fs.get_disk_inode_pos(new_inode_id);
        get_block_cache(new_inode_block_id as usize, Arc::clone(&self.block_device))
            .lock()
            .modify(new_inode_block_offset, |new_inode: &mut DiskInode| {
                new_inode.initialize(DiskInodeType::File);
            });
        self.insert_entry(name, new_inode_id, &mut fs);

        let (block_id, block_offset) = fs.get_disk_inode_pos(new_inode_id);
        block_cache_sync_all();
        // return inode
        Some(Arc::new(Self::new(
            block_id,
            block_offset,
            self.fs.clone(),
            self.block_device.clone(),
        )))
        // release efs lock automatically by compiler
    }
    /// List inodes under current inode
    pub fn ls(&self) -> Vec<String> {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            let file_count = (disk_inode.size as usize) / DIRENT_SZ;
            let mut v: Vec<String> = Vec::new();
            for i in 0..file_count {
                let mut dirent = DirEntry::empty();
                assert_eq!(
                    disk_inode.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device,),
                    DIRENT_SZ,
                );
                if !dirent.name().is_empty() { v.push(String::from(dirent.name())); }
            }
            v
        })
    }
    /// Read data from current inode
    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| disk_inode.read_at(offset, buf, &self.block_device))
    }
    /// Write data to current inode
    pub fn write_at(&self, offset: usize, buf: &[u8]) -> usize {
        let mut fs = self.fs.lock();
        let size = self.modify_disk_inode(|disk_inode| {
            self.increase_size((offset + buf.len()) as u32, disk_inode, &mut fs);
            disk_inode.write_at(offset, buf, &self.block_device)
        });
        block_cache_sync_all();
        size
    }
    /// Clear the data in current inode
    pub fn clear(&self) {
        let mut fs = self.fs.lock();
        self.modify_disk_inode(|disk_inode| {
            let size = disk_inode.size;
            let data_blocks_dealloc = disk_inode.clear_size(&self.block_device);
            assert!(data_blocks_dealloc.len() == DiskInode::total_blocks(size) as usize);
            for data_block in data_blocks_dealloc.into_iter() {
                fs.dealloc_data(data_block);
            }
        });
        block_cache_sync_all();
    }
}
