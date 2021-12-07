use quartz_nbt::NbtCompound;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PaletteBlock {
    name: String,
    properties: Option<NbtCompound>,
}

#[derive(Serialize, Deserialize)]
pub struct StructureBlock {
    state: i32,
    pos: [i32; 3],
    nbt: Option<NbtCompound>,
}

#[derive(Serialize, Deserialize)]
pub struct Structure {
    #[serde(rename = "DataVersion")]
    data_version: i32,
    size: [i32; 3],
    palette: Vec<PaletteBlock>,
    blocks: Vec<StructureBlock>,
}
