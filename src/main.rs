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

pub struct Partition {
    pub data: Box<dyn Read>,
    pub size: u64,
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

fn main() {
    env_logger::init();
    let cli = Cli::parse();
    let config: ConfigToml =
        toml::de::from_str(std::fs::read_to_string(cli.config).unwrap().as_str()).unwrap();

    let partitions = config
        .partitions
        .iter()
        .map(|p| {
            dbg!(&p.name);
            dbg!(&p.path);
            dbg!(&p.format);
            match p.format.as_str() {
                "fat32" => fat32::build_partition(p).unwrap(),
                "ext4" => ext4::build_partition(p).unwrap(),
                _ => unreachable!(),
            }
        })
        .collect::<Vec<_>>();

    write_partitions(partitions, &cli.output);
}

fn init_gpt(mem_device: &mut Box<File>, total_size: u64) -> GptDisk {
    // Set up the MBR
    let mbr_size = u32::try_from((total_size / 512) - 1).unwrap_or(0xFF_FF_FF_FF);
    let mbr = gpt::mbr::ProtectiveMBR::with_lb_size(mbr_size);
    mbr.overwrite_lba0(mem_device).expect("failed to write MBR");

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

    gdisk
}

fn write_partitions(partitions: Vec<Partition>, output: &String) {
    // Partition + MBR size
    let total_size = partitions.iter().map(|p| p.size).sum::<u64>() + 0x20000;
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

    // Set up the partition table
    let mut gdisk = init_gpt(&mut mem_device, total_size);

    partitions.iter().enumerate().for_each(|(idx, p)| {
        println!("Writing partition {} with size {} bytes", idx, p.size);
        gdisk
            .add_partition(
                &format!("test_name{idx}"),
                p.size,
                gpt::partition_types::EFI,
                0,
                None,
            )
            .unwrap();
    });

    let lb_size = gdisk.logical_block_size();

    // Copy the data
    let data = gdisk
        .partitions()
        .iter()
        .map(|p| p.1)
        .zip(partitions)
        .map(|(part, cfg)| {
            let start = part.bytes_start(*lb_size).unwrap();
            let len = part.bytes_len(*lb_size).unwrap();
            let data = cfg.data;

            (start, len, data)
        })
        .collect::<Vec<_>>();

    // Write the image to disk
    let mut file = gdisk.write().unwrap();

    // Write the partition data in to place
    data.into_iter()
        .for_each(|(part_start, part_len, mut data)| {
            let mut fat_slice =
                fscommon::StreamSlice::new(&mut file, part_start, part_start + part_len).unwrap();

            std::io::copy(&mut data, &mut fat_slice).unwrap();
        })
}
