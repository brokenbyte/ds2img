use std::{error, fs::File};
use std::{io::Cursor, path::Path};

use walkdir::WalkDir;

use crate::{Partition, PartitionConfig};

const BLOCK_SIZE: u64 = 1024;
const BLOCK_COUNT: u64 = 1024;

pub fn build_partition(partition: &PartitionConfig) -> Result<Partition, ()> {
    let size = estimate_size(&partition.path).unwrap();
    let img = File::create("rust.img").unwrap();
    img.set_len(size).unwrap();

    let p = Partition {
        size,
        data: Box::new(img),
    };

    todo!()
}

pub fn estimate_size(input_dir: impl AsRef<Path>) -> Result<u64, Box<dyn error::Error>> {
    Ok({
        let contents_size = WalkDir::new(input_dir)
            .into_iter()
            .skip(1) // Skip the root directory
            .filter_map(|e| e.ok())
            .map(|entry| entry.metadata().map(|e| e.len()).unwrap_or(0))
            .sum::<u64>();

        let journal_size = BLOCK_SIZE * BLOCK_COUNT;
        // let multiple = size + 1024 - (size % 1024);
        let size = contents_size + journal_size;

        let alignment = size % 1024;

        if alignment != 0 {
            size + 1024 - alignment
        } else {
            size
        }
    })
}

// https://www.kernel.org/doc/html/latest/filesystems/ext4/dynamic.html#directory-entries
// https://unix.stackexchange.com/questions/124979/where-does-ext4-store-directory-sizes
// https://unix.stackexchange.com/questions/561603/how-many-files-in-a-directory-before-the-size-of-the-directory-file-increase#:~:text=In%20Linux%20or%20more%20particular,the%20internal%20%22file%20list%22.
