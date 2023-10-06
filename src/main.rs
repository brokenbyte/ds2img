#![allow(unused)]

mod ext4;
mod fat32;

use std::{
    error,
    fs::{self, File, Metadata, OpenOptions},
    io::{self, BufReader, Cursor, Read, Seek, Write},
    os::fd::AsFd,
    path::{Path, PathBuf},
};

use clap::Parser;
use fatfs::{format_volume, Dir, FileSystem, FormatVolumeOptions, FsOptions};
use gpt::{DiskDeviceObject, GptDisk};
use log::{debug, info};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct ConfigToml {
    disk: DiskConfig,
    #[serde(rename = "partition")]
    partitions: Vec<PartitionConfig>,
}

#[derive(Deserialize, Debug)]
pub struct PartitionConfig {
    name: String,
    path: String,
    format: String,
    size: Option<u64>,
}

#[derive(Deserialize, Debug)]
struct DiskConfig {
    size: u64,
}

#[derive(Parser, Debug)]
struct Cli {
    #[clap(short, long, default_value = "ds2img.toml")]
    /// Path to partition config file
    config: String,

    #[clap(short, long, default_value = "root.img")]
    /// Path to create the image
    output: String,
}

pub struct Partition {
    pub data: Box<dyn Read>,
    pub size: u64,
}

fn main() {
    env_logger::init();
    // write_fat();
    write_ext4();
}

fn write_ext4() {
    let cli = Cli::parse();
    let config: ConfigToml =
        toml::de::from_str(std::fs::read_to_string(cli.config).unwrap().as_str()).unwrap();

    // Build the raw partition data
    info!("Building partitions");
    let mut ext4_partition = ext4::build_partition(&config.partitions[0]).unwrap();
    debug!("part size: {} bytes", ext4_partition.size);

    write_partition(&mut ext4_partition, &cli.output);
}

fn write_fat() {
    let cli = Cli::parse();
    let config: ConfigToml =
        toml::de::from_str(std::fs::read_to_string(cli.config).unwrap().as_str()).unwrap();

    // Build the raw partition data
    info!("Building partitions");
    let mut fat_partition = fat32::build_partition(&config.partitions[0]).unwrap();
    debug!("part size: {} bytes", fat_partition.size);

    write_partition(&mut fat_partition, &cli.output);
}

fn write_partition(partition: &mut Partition, output: &String) {
    // Partition + MBR size
    // let total_size = fat_partition.size + (1024 * 50);
    let total_size = partition.size + 0x20000;
    debug!("file size: {}", total_size);

    // Set up the file to hold the disk image
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .read(true)
        .open(output)
        .unwrap();

    file.set_len(total_size).unwrap();
    let mut mem_device = Box::new(file);

    // Set up the MBR
    let mbr_size = u32::try_from((total_size / 512) - 1).unwrap_or(0xFF_FF_FF_FF);
    let mbr = gpt::mbr::ProtectiveMBR::with_lb_size(mbr_size);

    mbr.overwrite_lba0(&mut mem_device)
        .expect("failed to write MBR");

    // Set up the GPT
    let mut gdisk = gpt::GptConfig::default()
        .initialized(false)
        .writable(true)
        .logical_block_size(gpt::disk::LogicalBlockSize::Lb512)
        .create_from_device(Box::new(mem_device), None)
        .unwrap();

    // Write an empty partition table
    gdisk
        .update_partitions(std::collections::BTreeMap::<u32, gpt::partition::Partition>::new())
        .unwrap();

    // Add the partition
    let part = gdisk
        .add_partition(
            "test_name",
            partition.size,
            gpt::partition_types::EFI,
            0,
            None,
        )
        .unwrap();

    // Get the partition offsets
    let part = gdisk.partitions().get(&part).unwrap();

    let lb_size = gdisk.logical_block_size();
    let part_start = part.bytes_start(*lb_size).unwrap();
    let part_len = part.bytes_len(*lb_size).unwrap();

    // Write the image to disk
    let mut file = gdisk.write().unwrap();

    // Write the partition data in to place
    let mut fat_slice =
        fscommon::StreamSlice::new(file, part_start, part_start + part_len).unwrap();

    std::io::copy(&mut partition.data, &mut fat_slice).unwrap();
}
