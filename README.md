# Directory Structure 2 disk Image

WIP tool to create disk images + copy files on to partitions based on a config file

## Why?

I'm currently trying to create a bootloader capable of booting a Linux kernel with [uefi-rs](https://docs.rs/uefi/latest/uefi/),
but didn't want to keep manually creating/partitioning disk images, mounting them, and copying files over
every time I recompiled, but couldn't find a better tool for automating the process, so I'm writing this to
speed up iteration on that process.

## Usage

> Note: the config file is not fully implemented

```
Usage: ds2img [OPTIONS]

Options:
  -c, --config <CONFIG>  Path to partition config file [default: ds2img.toml]
  -o, --output <OUTPUT>  Path to create the image [default: root.img]
  -h, --help             Print help
```

To mount the image, run:

```
$ sudo kpartx -av root.img
```

example output:

```
add map loop1p1 (254:1): 0 45 linear 7:1 34
```

Partitions will show up under `/dev/mapper`, e.g.:

```
$ ls -l /dev/mapper
lrwxrwxrwx  254,1 root root  5 Oct 16:53  loop1p1 -> ../dm-1
```

To unmount the image, run:

```
$ sudo kpartx -d root.img
```
> Note: Make sure you unmount any mounted partitions or it will fail to remove the loopback devices

# Credit

[mkimg](https://github.com/h33p/mkimg/) was very helpful for figuring out how to use
[fatfs](https://docs.rs/fatfs/latest/fatfs/) to create FAT32 partitions and [gpt](https://docs.rs/gpt/latest/gpt/)
to create GPT partition tables.
