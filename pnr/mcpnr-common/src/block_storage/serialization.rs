use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Serialize, Serializer};

use super::{Block, BlockStorage, PropertyValue};

impl Serialize for PropertyValue {
    fn serialize<S>(&self, s: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        match self {
            Self::String(st) => s.serialize_str(st),
            Self::Byte(b) => s.serialize_i8(*b),
        }
    }
}

impl Serialize for Block {
    fn serialize<S>(&self, s: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        let map_size = if self.properties.is_some() { 1 } else { 0 };
        let mut map = s.serialize_map(Some(1 + map_size))?;
        map.serialize_entry("name", &self.name)?;
        if let Some(ref props) = self.properties {
            map.serialize_entry("properties", props)?;
        }

        map.end()
    }
}

impl Serialize for BlockStorage {
    fn serialize<S>(&self, s: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        let mut map = s.serialize_map(Some(3))?;

        map.serialize_entry("extents", &ArrayAsExtentsMapWrapper(&self.extents))?;
        map.serialize_entry("palette", &self.palette)?;
        map.serialize_entry("blocks", &BlockIndexSynth(&self.blocks))?;

        map.end()
    }
}

/// Cursed workaround to dump a 3-entry u32 slice as x/y/z map
struct ArrayAsExtentsMapWrapper<'a>(&'a [u32; 3]);

impl<'a> Serialize for ArrayAsExtentsMapWrapper<'a> {
    fn serialize<S>(&self, s: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        let mut map = s.serialize_map(Some(3))?;
        map.serialize_entry("x", &self.0[0])?;
        map.serialize_entry("y", &self.0[1])?;
        map.serialize_entry("z", &self.0[2])?;
        map.end()
    }
}

/// Cursed workaround to dump list of numbers as individual objects
struct BlockIndexSynth<'a>(&'a [u32]);

impl<'a> Serialize for BlockIndexSynth<'a> {
    fn serialize<S>(&self, s: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        let mut seq = s.serialize_seq(Some(self.0.len()))?;

        for item in self.0 {
            seq.serialize_element(&BlockIndexEntrySynth(item))?;
        }

        seq.end()
    }
}

/// Cursed workaround to dump individual numbers as objects
struct BlockIndexEntrySynth<'a>(&'a u32);

impl<'a> Serialize for BlockIndexEntrySynth<'a> {
    fn serialize<S>(&self, s: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        let mut map = s.serialize_map(Some(1))?;
        map.serialize_entry("pi", self.0)?;
        map.end()
    }
}
