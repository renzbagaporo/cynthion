//! This build script prepares the HAL build.

use imxrt_rt::{Family, Memory, RuntimeBuilder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    RuntimeBuilder::from_flexspi(Family::Imxrt1010, 16 * 1024 * 1024)
        .flexram_banks(imxrt_rt::FlexRamBanks {
            ocram: 1,
            itcm: 2,
            dtcm: 1,
        })
        .uninit(Memory::Dtcm)
        .stack_size(16 * 1024)
        .build()?;
    println!("cargo:rustc-cfg=board=\"imxrt1010evk\"");
    println!("cargo:rustc-cfg=chip=\"imxrt1010\"");
    return Ok(());
}