use crate::{pgs_enum, pgs_struct};

pgs_enum! { SegmentType {
    PaletteDef = 0x14,
    ObjDataDef = 0x15,
    PresentationComp = 0x16,
    WindowDef = 0x17,
    EndDisplaySet = 0x80,
}}

pgs_struct! { SegmentHeader {
    ty: SegmentType,
    len: u16,
}}

pgs_struct! { PaletteDef {
    palette_id: u8,
    version: u8,
}}

pgs_struct! { PaletteEntry {
    palette_entry_id: u8,
    luminance: u8, // Y
    color_difference_red: u8, // Cr
    color_difference_blue: u8, // Cb
    alpha: u8,
}}

#[derive(Debug)]
pub struct ObjectDataLength(pub u32);

impl super::parsing::Deserialize for ObjectDataLength {
    fn deserialize<R: std::io::Read>(reader: &mut dyn std::io::Read) -> std::io::Result<Self> {
        let mut bytes = [0u8; 4];
        reader.read_exact(&mut bytes[1..])?;
        let value = u32::from_be_bytes(bytes);
        Ok(ObjectDataLength(value))
    }
}

pgs_struct! { ObjectDef {
    id: u16,
    version: u8,
    last_seq_in_flag: u8,
    object_data_legnth: ObjectDataLength,
    width: u16,
    height: u16,
}}
