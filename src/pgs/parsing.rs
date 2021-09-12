use std::io::Read;

use byteorder::{BigEndian, ReadBytesExt};

pub trait Deserialize: Sized {
    fn deserialize<R: Read>(reader: &mut dyn Read) -> std::io::Result<Self>;
}

impl Deserialize for u8 {
    fn deserialize<R: Read>(reader: &mut dyn Read) -> std::io::Result<Self> {
        reader.read_u8()
    }
}

impl Deserialize for u16 {
    fn deserialize<R: Read>(reader: &mut dyn Read) -> std::io::Result<Self> {
        reader.read_u16::<BigEndian>()
    }
}

impl Deserialize for u32 {
    fn deserialize<R: Read>(reader: &mut dyn Read) -> std::io::Result<Self> {
        reader.read_u32::<BigEndian>()
    }
}

#[macro_export]
macro_rules! pgs_struct {
    ( $name:ident { $( $param:ident : $type:ty ),* $(,)* }) => (
        #[derive(Debug)]
        pub struct $name {
            $( pub $param : $type, )*
        }

        impl crate::pgs::parsing::Deserialize for $name {
            fn deserialize<R: std::io::Read>(reader: &mut dyn std::io::Read) -> std::io::Result<Self> {
                Ok(Self {
                    $( $param: <$type>::deserialize::<R>(reader)?, )*
                })
            }
        }
    );
}

#[macro_export]
macro_rules! pgs_enum {
    ( $name:ident { $( $variant:ident = $value:expr ),* $(,)* }) => (
        #[repr(u8)]
        #[derive(Debug, PartialEq)]
        pub enum $name {
            $( $variant = $value, )*
        }

        impl crate::pgs::parsing::Deserialize for $name {
            fn deserialize<R: std::io::Read>(reader: &mut dyn std::io::Read) -> std::io::Result<Self> {
                use byteorder::ReadBytesExt;
                let value = reader.read_u8()?;
                match value {
                    $( $value => Ok($name::$variant), )*
                    _ => panic!("Unknown value: 0x{:X}", value), // TODO: return error
                }
            }
        }
    );
}

pub trait PgsDeserializer {
    fn dezerialize<T: Deserialize + Sized>(&mut self) -> std::io::Result<T>;
    fn ref_bytes(&mut self, len: usize) -> std::io::Result<&[u8]>;
    fn is_at_end(&self) -> bool;
}

impl PgsDeserializer for std::io::Cursor<&[u8]> {
    fn dezerialize<T: Deserialize>(&mut self) -> std::io::Result<T> {
        T::deserialize::<Self>(self)
    }

    fn ref_bytes(&mut self, len: usize) -> std::io::Result<&[u8]> {
        let start = self.position() as usize;
        let end = start + len;
        let slice = &self.get_ref()[start..end];
        assert_eq!(slice.len(), len);
        self.set_position(end as u64);
        Ok(slice)
    }

    fn is_at_end(&self) -> bool {
        self.position() as usize >= self.get_ref().len()
    }
}
