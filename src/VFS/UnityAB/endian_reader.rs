// Adapted from Myrtle (assets/unpacker/src/unity).
use std::io;

/// An `InvalidData` error for malformed serialized data.
pub fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// Fixed-width numeric readers honoring the current byte order.
macro_rules! read_numbers {
    ($($name:ident -> $ty:ty),* $(,)?) => {$(
        pub fn $name(&mut self) -> Result<$ty, io::Error> {
            let bytes = self.read_array()?;
            Ok(if self.big_endian {
                <$ty>::from_be_bytes(bytes)
            } else {
                <$ty>::from_le_bytes(bytes)
            })
        }
    )*};
}

pub struct EndianReader<'a> {
    data: &'a [u8],
    pos: usize,
    big_endian: bool,
}

impl<'a> EndianReader<'a> {
    #[must_use]
    pub const fn new(data: &'a [u8], big_endian: bool) -> Self {
        Self {
            data,
            pos: 0,
            big_endian,
        }
    }

    pub const fn set_big_endian(&mut self, big_endian: bool) {
        self.big_endian = big_endian;
    }

    #[must_use]
    pub const fn position(&self) -> usize {
        self.pos
    }

    #[must_use]
    pub const fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn read_u8(&mut self) -> Result<u8, io::Error> {
        self.ensure(1)?;
        let val = self.data[self.pos];
        self.pos += 1;
        Ok(val)
    }

    read_numbers! {
        read_i16 -> i16,
        read_u16 -> u16,
        read_i32 -> i32,
        read_u32 -> u32,
        read_i64 -> i64,
        read_u64 -> u64,
    }

    pub fn read_bool(&mut self) -> Result<bool, io::Error> {
        Ok(self.read_u8()? != 0)
    }

    pub fn read_cstring(&mut self) -> Result<String, io::Error> {
        let start = self.pos;
        while self.pos < self.data.len() && self.data[self.pos] != 0 {
            self.pos += 1;
        }
        let bytes = &self.data[start..self.pos];
        if self.pos < self.data.len() {
            self.pos += 1; // skip the null terminator
        }
        String::from_utf8(bytes.to_vec()).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    fn read_array<const N: usize>(&mut self) -> Result<[u8; N], io::Error> {
        self.ensure(N)?;
        let bytes = self.data[self.pos..self.pos + N].try_into().unwrap();
        self.pos += N;
        Ok(bytes)
    }

    pub fn read_bytes(&mut self, count: usize) -> Result<Vec<u8>, io::Error> {
        self.ensure(count)?;
        let val = self.data[self.pos..self.pos + count].to_vec();
        self.pos += count;
        Ok(val)
    }

    pub const fn align(&mut self, n: usize) {
        let remainder = self.pos % n;
        if remainder != 0 {
            self.pos += n - remainder;
        }
    }

    fn ensure(&self, count: usize) -> Result<(), io::Error> {
        if self.data.len().saturating_sub(self.pos) < count {
            Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "read past end",
            ))
        } else {
            Ok(())
        }
    }
}
