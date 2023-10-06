use std::{
    error,
    fs::File,
    io::{self, Cursor, Read},
    path::{Path, PathBuf},
};

use fatfs::{format_volume, Dir, FatType, FileSystem, FormatVolumeOptions, FsOptions};
use log::debug;
use walkdir::WalkDir;

use crate::{Partition, PartitionConfig};

const FAT_BYTES_PER_CLUSTER: u64 = 512;
const FAT_ALIGN: u64 = FAT_BYTES_PER_CLUSTER - 1;
const FAT_BYTES_PER_SECTOR: u64 = 512;

// Reserved by the `fatfs` crate
const RESERVED_SECTORS: u64 = FAT_BYTES_PER_SECTOR * 8;

fn estimate_size(input_dir: &Path) -> u64 {
    let root_entries = WalkDir::new(input_dir)
        .max_depth(1)
        .into_iter()
        .skip(1)
        .filter_map(Result::ok)
        .count() as u64;

    let (contents_size, number_of_fats) = WalkDir::new(input_dir)
        .into_iter()
        .skip(1) // Skip the root directory
        .filter_map(|e| e.ok())
        .fold((0, 0), |(total_size, n_fats), entry| {
            if let Ok(metadata) = entry.metadata() {
                let fats = (metadata.len() + FAT_ALIGN) / FAT_BYTES_PER_CLUSTER;
                let size = metadata.len();
                (total_size + size, n_fats + fats)
            } else {
                (total_size, n_fats)
            }
        });

    // https://en.wikipedia.org/wiki/Design_of_the_FAT_file_system
    // Reserved sectors
    // FAT Region: # of FATs * Sectors per FAT
    // Root Dir Region: # of root entries * 32 / Bytes per Sector
    // Data Region: # of Clusters * Sectors per Cluster

    let fat_region = (number_of_fats * FAT_BYTES_PER_CLUSTER);
    let root_dir_region = (root_entries * 32 / FAT_BYTES_PER_CLUSTER);

    debug!("{} reserved sectors", RESERVED_SECTORS);
    debug!("{} fat region", fat_region);
    debug!("{} root dir region", root_dir_region);
    debug!("{} contents size", contents_size);
    debug!(
        "{} total",
        fat_region + root_dir_region + RESERVED_SECTORS + contents_size
    );

    let size = fat_region + root_dir_region + RESERVED_SECTORS + contents_size;

    let alignment = size % FAT_BYTES_PER_SECTOR;

    if alignment != 0 {
        size + FAT_BYTES_PER_SECTOR - alignment
    } else {
        size
    }
}

pub fn build_partition(partition: &PartitionConfig) -> Result<Partition, ()> {
    let size = estimate_size(&PathBuf::from(&partition.path));
    debug!("estimated size: {} bytes", size);

    let mut file: Vec<u8> = vec![0u8; 0];

    let part_start = 0;
    let part_len = 1024 * 100;

    let cursor = Cursor::new(&mut file);

    let mut fat_slice =
        fscommon::StreamSlice::new(cursor, part_start, part_start + part_len).unwrap();

    let mut buf_stream = fscommon::BufStream::new(&mut fat_slice);

    format_volume(
        &mut buf_stream,
        FormatVolumeOptions::new()
            .bytes_per_cluster(FAT_BYTES_PER_CLUSTER as u32)
            .fat_type(FatType::Fat32),
    )
    .unwrap();

    // Copy files onto the partition
    {
        let fs = FileSystem::new(buf_stream, FsOptions::new()).unwrap();

        let mut count = 0;
        let root_dir = fs.root_dir();
        let pb = PathBuf::from(&partition.path);

        WalkDir::new(pb)
            .into_iter()
            .skip(1)
            .filter_map(|e| e.ok())
            .for_each(|entry| {
                let metadata = entry.metadata().unwrap();
                let path = entry.path();
                let name = path.file_name().unwrap().to_str().unwrap();
                if metadata.is_dir() {
                    debug!("DIR: {name}");
                    root_dir.create_dir(name);
                } else {
                    count += 1;
                    debug!("FILE {count}: {name}");
                    let mut orig_file = File::open(path).unwrap();
                    let mut file = root_dir.create_file(name).unwrap();
                    std::io::copy(&mut orig_file, &mut file).unwrap();
                }
            });
        // fs.unmount().unwrap();
    }

    Ok(Partition {
        size: file.len() as u64,
        data: Box::new(Cursor::new(file)),
    })
}
