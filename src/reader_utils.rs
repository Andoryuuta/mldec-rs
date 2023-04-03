use anyhow::{anyhow, Result};
use byteorder::{LittleEndian, ReadBytesExt};
// use byteorder::{ReadBytesExt, LittleEndian};
use encoding::all::GBK;
use encoding::{DecoderTrap, Encoding};

const MAX_STRING_SIZE: usize = 4 * 1024 * 1024;

pub trait StringReadExt {
    fn read_until_byte(&mut self, byte: u8, max_size: usize) -> Result<Vec<u8>>;
    fn read_fixed_size_utf8_string(&mut self, length: u32) -> Result<String>;
    fn read_null_terminated_utf8_string(&mut self) -> Result<String>;
    fn read_null_terminated_gbk_string(&mut self) -> Result<String>;
    fn read_null_terminated_gbk_string_i32_offset_pointer(&mut self) -> Result<String>;
}

impl<T> StringReadExt for T
where
    T: std::io::Read,
    T: std::io::Seek,
{
    // This mimics the read_until method available on BufRead,
    // but is intentionally reimplemented (poorly) here on a regular
    // std::io::Read + std::io::Seek reader.
    fn read_until_byte(&mut self, delimiter: u8, max_size: usize) -> Result<Vec<u8>> {
        let mut data = Vec::<u8>::new();

        for _index in 0..max_size {
            let mut buf = [0; 1];
            self.read_exact(&mut buf)?;
            if buf[0] == delimiter {
                return Ok(data);
            }
            data.extend(buf);
        }

        Err(anyhow!("Read MAX_STRING_SIZE bytes!"))
    }

    fn read_fixed_size_utf8_string(&mut self, length: u32) -> Result<String> {
        let mut buf = vec![0; length.try_into()?];
        self.read_exact(&mut buf)?;

        // Terminate at NULL byte
        let null_position = buf.iter().position(|&c| c == b'\0').unwrap_or(buf.len());

        Ok(String::from_utf8_lossy(&buf[0..null_position]).into())
    }

    fn read_null_terminated_utf8_string(&mut self) -> Result<String> {
        let buf = self.read_until_byte(b'\x00', MAX_STRING_SIZE)?;

        // Terminate at NULL byte
        let null_position = buf.iter().position(|&c| c == b'\0').unwrap_or(buf.len());

        Ok(String::from_utf8_lossy(&buf[0..null_position]).into())
    }

    fn read_null_terminated_gbk_string(&mut self) -> Result<String> {
        let buf = self.read_until_byte(b'\x00', MAX_STRING_SIZE)?;

        match GBK.decode(&buf, DecoderTrap::Replace) {
            Ok(s) => Ok(s),
            Err(err) => Err(anyhow::anyhow!("Error trying to decode: {err}")),
        }
    }

    fn read_null_terminated_gbk_string_i32_offset_pointer(&mut self) -> Result<String> {
        let offset = self.read_i32::<LittleEndian>()?;
        if offset == -1 {
            return Ok("".to_string());
        }

        let pos = self.stream_position()?;
        _ = self.seek(std::io::SeekFrom::Start(offset as u64))?;
        let s = self.read_null_terminated_gbk_string()?;
        _ = self.seek(std::io::SeekFrom::Start(pos))?;
        Ok(s)
    }
}
