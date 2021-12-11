use quartz_nbt::NbtCompound;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PaletteBlock {
    pub name: String,
    pub properties: Option<NbtCompound>,
}

#[derive(Serialize, Deserialize)]
pub struct StructureBlock {
    pub state: i32,
    pub pos: [i32; 3],
    pub nbt: Option<NbtCompound>,
}

#[derive(Serialize, Deserialize)]
pub struct Structure {
    #[serde(rename = "DataVersion")]
    pub data_version: i32,
    pub size: [i32; 3],
    pub palette: Vec<PaletteBlock>,
    pub blocks: Vec<StructureBlock>,
}
