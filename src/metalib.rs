use anyhow::{anyhow, Context, Result};
use bitflags::bitflags;
use byteorder::{LittleEndian, ReadBytesExt};
use int_enum::IntEnum;
use reader_utils::StringReadExt;
use std::io::{prelude::*, Cursor, SeekFrom};

use crate::reader_utils;

// None of the structs in this file have unused fields, despite the #[allow(unused)] attribute.
// Rust gives these errors because the fields are not used directly here (e.g. only in a debug print)
// See: https://github.com/rust-lang/rust/issues/88900.

pub const INVALID_METALIB_VALUE: i32 = -1;

bitflags! {
    pub struct TDRMetaFlags: u32 {
        const FIXED_SIZE = 0x0001;
        const HAS_ID = 0x0002;
        const RESOVLED = 0x0004;
        const VARIABLE = 0x0008;
        const STRICT_INPUT = 0x0010;
        const HAS_AUTOINCREMENT_ENTRY = 0x0020;
        const NEED_PREFIX_FOR_UNIQUENAME = 0x0040;
        const HAS_EXTEND_META = 0x0080;
        const IS_EXTEND_META = 0x0100;
        const UNKNOWN_FLAG_512 = 0x0200;
        const ALL = Self::FIXED_SIZE.bits | Self::HAS_ID.bits | Self::RESOVLED.bits | Self::VARIABLE.bits | Self::STRICT_INPUT.bits | Self::HAS_AUTOINCREMENT_ENTRY.bits | Self::NEED_PREFIX_FOR_UNIQUENAME.bits | Self::HAS_EXTEND_META.bits | Self::IS_EXTEND_META.bits | Self::UNKNOWN_FLAG_512.bits;
    }

    pub struct TDRMetaEntryFlags: u16 {
        // Rust bitflags crate has unexpected behavior with all-empty-bit flags (as noted on the docs)
        // const NONE = 0x0000;

        const RESOVLED = 0x0001;

        /// Is a pointer "*" type
        const POINT_TYPE = 0x0002;

        /// Is a refer "@" type
        const REFER_TYPE = 0x0004;

        /// Has SQL DB ID
        const HAS_ID = 0x0008;

        /// Has "maxid" and "minid" attrs on the entry
        const HAS_MAXMIN_ID = 0x0010;

        /// Used if this entry is a fixed-size
        const FIXED_SIZE = 0x0020;

        /// Used if this entry is a "count" field for another member.
        const REFER_COUNT = 0x0040;
        const UNKNOWN_FLAG_X0080 = 0x0080;
        const UNKNOWN_FLAG_X0100 = 0x0100;
        const UNKNOWN_FLAG_X0200 = 0x0200;

    }

    pub struct TDRMetaEntryDBFlags: u8 {
        // Rust bitflags crate has unexpected behavior with all-empty-bit flags (as noted on the docs)
        // const NONE = 0x00;
        const UNIQUE = 0x01;
        const NOT_NULL = 0x02;
        const EXTEND_TO_TABLE = 0x04;
        const PRIMARY_KEY = 0x10;
        const AUTO_INCREMENT = 0x20;
    }
}

#[repr(i32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, IntEnum)]
pub enum MetaPrimativeType {
    UNKNOWN = -1,
    UNION = 0,
    STRUCT = 1,
    CHAR = 2,       // i8
    UCHAR = 3,      // u8
    BYTE = 4,       // u8
    SHORT = 5,      // i16
    USHORT = 6,     // u16
    INT = 7,        // i32
    UINT = 8,       // u32
    LONG = 9,       // i32
    ULONG = 10,     // u32
    LONGLONG = 11,  // i64
    ULONGLONG = 12, // u64
    DATE = 13,      // 4 byte date
    TIME = 14,      // 4 byte time
    DATETIME = 15,  // 8 byte date+time
    MONEY = 16,     // 4 byte money-specific data type
    FLOAT = 17,     // f32
    DOUBLE = 18,    // f64
    IP = 19,        // 4 byte IPv4
    WCHAR = 20,     // u16 / 2 byte
    STRING = 21,    // char[?]
    WSTRING = 22,   // wchar_t[?]
    VOID = 23,      // ???
}

#[derive(Debug)]
#[allow(unused)]
pub struct TDRTypeInfo<'a> {
    pub xml_name: &'a str,
    pub c_name: &'a str,
    pub primative_type: MetaPrimativeType,
    pub size: i32,
}

#[rustfmt::skip]
#[allow(unused)]
pub const TDR_PRIMATIVE_TYPE_INFO: &[TDRTypeInfo] = &[
    TDRTypeInfo { xml_name: "union",     c_name: "union",          primative_type: MetaPrimativeType::UNION,     size: 0 },
    TDRTypeInfo { xml_name: "struct",    c_name: "struct",         primative_type: MetaPrimativeType::STRUCT,    size: 0 },
    TDRTypeInfo { xml_name: "tinyint",   c_name: "int8_t",         primative_type: MetaPrimativeType::CHAR,      size: 1 },
    TDRTypeInfo { xml_name: "tinyuint",  c_name: "uint8_t",        primative_type: MetaPrimativeType::UCHAR,     size: 1 },
    TDRTypeInfo { xml_name: "smallint",  c_name: "int16_t",        primative_type: MetaPrimativeType::SHORT,     size: 2 },
    TDRTypeInfo { xml_name: "smalluint", c_name: "uint16_t",       primative_type: MetaPrimativeType::USHORT,    size: 2 },
    TDRTypeInfo { xml_name: "int",       c_name: "int32_t",        primative_type: MetaPrimativeType::INT,       size: 4 },
    TDRTypeInfo { xml_name: "uint",      c_name: "uint32_t",       primative_type: MetaPrimativeType::UINT,      size: 4 },
    TDRTypeInfo { xml_name: "bigint",    c_name: "int64_t",        primative_type: MetaPrimativeType::LONGLONG,  size: 8 },
    TDRTypeInfo { xml_name: "biguint",   c_name: "uint64_t",       primative_type: MetaPrimativeType::ULONGLONG, size: 8 },
    TDRTypeInfo { xml_name: "int8",      c_name: "int8_t",         primative_type: MetaPrimativeType::CHAR,      size: 1 },
    TDRTypeInfo { xml_name: "uint8",     c_name: "uint8_t",        primative_type: MetaPrimativeType::UCHAR,     size: 1 },
    TDRTypeInfo { xml_name: "int16",     c_name: "int16_t",        primative_type: MetaPrimativeType::SHORT,     size: 2 },
    TDRTypeInfo { xml_name: "uint16",    c_name: "uint16_t",       primative_type: MetaPrimativeType::USHORT,    size: 2 },
    TDRTypeInfo { xml_name: "int32",     c_name: "int32_t",        primative_type: MetaPrimativeType::INT,       size: 4 },
    TDRTypeInfo { xml_name: "uint32",    c_name: "uint32_t",       primative_type: MetaPrimativeType::UINT,      size: 4 },
    TDRTypeInfo { xml_name: "int64",     c_name: "int64_t",        primative_type: MetaPrimativeType::LONGLONG,  size: 8 },
    TDRTypeInfo { xml_name: "uint64",    c_name: "uint64_t",       primative_type: MetaPrimativeType::ULONGLONG, size: 8 },
    TDRTypeInfo { xml_name: "float",     c_name: "float",          primative_type: MetaPrimativeType::FLOAT,     size: 4 },
    TDRTypeInfo { xml_name: "double",    c_name: "double",         primative_type: MetaPrimativeType::DOUBLE,    size: 8 },
    TDRTypeInfo { xml_name: "decimal",   c_name: "float",          primative_type: MetaPrimativeType::FLOAT,     size: 4 },
    TDRTypeInfo { xml_name: "date",      c_name: "tdr_date_t",     primative_type: MetaPrimativeType::DATE,      size: 4 },
    TDRTypeInfo { xml_name: "time",      c_name: "tdr_time_t",     primative_type: MetaPrimativeType::TIME,      size: 4 },
    TDRTypeInfo { xml_name: "datetime",  c_name: "tdr_datetime_t", primative_type: MetaPrimativeType::DATETIME,  size: 8 },
    TDRTypeInfo { xml_name: "string",    c_name: "char",           primative_type: MetaPrimativeType::STRING,    size: 1 },
    TDRTypeInfo { xml_name: "byte",      c_name: "uint8_t",        primative_type: MetaPrimativeType::UCHAR,     size: 1 },
    TDRTypeInfo { xml_name: "ip",        c_name: "tdr_ip_t",       primative_type: MetaPrimativeType::IP,        size: 4 },
    TDRTypeInfo { xml_name: "wchar",     c_name: "tdr_wchar_t",    primative_type: MetaPrimativeType::WCHAR,     size: 2 },
    TDRTypeInfo { xml_name: "wstring",   c_name: "tdr_wchar_t",    primative_type: MetaPrimativeType::WSTRING,   size: 2 },
    TDRTypeInfo { xml_name: "void",      c_name: "void",           primative_type: MetaPrimativeType::VOID,      size: 1 },
    TDRTypeInfo { xml_name: "char",      c_name: "char",           primative_type: MetaPrimativeType::CHAR,      size: 1 },
    TDRTypeInfo { xml_name: "uchar",     c_name: "unsigned char",  primative_type: MetaPrimativeType::UCHAR,     size: 1 },
    TDRTypeInfo { xml_name: "short",     c_name: "int16_t",        primative_type: MetaPrimativeType::SHORT,     size: 2 },
    TDRTypeInfo { xml_name: "ushort",    c_name: "uint16_t",       primative_type: MetaPrimativeType::USHORT,    size: 2 },
    TDRTypeInfo { xml_name: "long",      c_name: "int32_t",        primative_type: MetaPrimativeType::LONG,      size: 4 },
    TDRTypeInfo { xml_name: "ulong",     c_name: "uint32_t",       primative_type: MetaPrimativeType::ULONG,     size: 4 },
    TDRTypeInfo { xml_name: "longlong",  c_name: "int64_t",        primative_type: MetaPrimativeType::LONGLONG,  size: 8 },
    TDRTypeInfo { xml_name: "ulonglong", c_name: "uint64_t",       primative_type: MetaPrimativeType::ULONGLONG, size: 8 },
];

/// Serialized size of the MetalibHeader struct.
pub const METALIB_HEADER_SIZE: u32 = 0x114;

#[derive(Debug)]
#[allow(unused)]
pub struct MetalibHeader {
    pub magic: u16,
    pub build: u16,
    pub platform_arch: u32,

    /// Total: size of the metalib in bytes(TDRMetaLib + all data)
    pub size: u32,

    pub field_c: u32,
    pub field_10: u32,
    pub field_14: u32,
    pub field_18: u32,
    pub id: i32,
    pub xml_tag_set_ver: u32,
    pub field_24: u32,

    /// Max count of TDRMeta entries in this metalib.
    pub max_meta_num: i32,

    /// Count of TDRMeta entries in this metalib.
    pub cur_meta_num: i32,

    /// Max count of TDRMacro entries in this metalib.
    pub max_macro_num: i32,

    /// Count of TDRMacro entries in this metalib.
    pub cur_macro_num: i32,

    /// Max count of TDRMacroGroup entries in this metalib.
    pub max_macros_group_num: i32,

    /// Count of TDRMacroGroup entries in this metalib.
    pub cur_macros_group_num: i32,

    pub field_40: u32,
    pub field_44: u32,
    pub version: u32,

    /// Post-header file offset to array of TDRMacro instances (of size `[self.cur_macro_num]`).
    pub ptr_macro: u32,

    /// Post-header file offset to array of TDRIdEntry instances (of size `[self.cur_meta_num]`).
    pub ptr_id: u32,

    /// Post-header file offset to array of TDRNameEntry instances (of size `[self.cur_meta_num]`).
    pub ptr_name: u32,

    /// Post-header file offset to array of TDRMapEntry instances (of size `[self.cur_meta_num]`).
    pub ptr_map: u32,

    /// Post-header file offset to array of TDRMeta instances (of size `[self.cur_meta_num]`).
    pub ptr_meta: u32,

    /// Post-header file offset to last TDRMeta entry in the array starting at `ptr_meta`.
    pub ptr_last_meta: u32,

    /// Total size of string buf table
    pub free_str_buf_size: i32,

    /// Start of string table
    pub ptr_str_buf: u32,

    /// End of string table
    pub ptr_free_str_buf: u32,

    /// Post-header file offset to array of TDRMapEntry instances (of size `[self.cur_macros_group_num]`).
    pub ptr_macro_group_map: u32,

    /// Post-header file offset to array of TDRMacroGroup instances (of size `[self.cur_macros_group_num]`).
    pub ptr_macros_group: u32,

    pub field_78: u32,
    pub field_7c: i32,
    pub field_80: i32,
    pub field_84: u32,
    pub field_88: u32,
    pub field_8c: i32,
    pub field_90: i32,

    /// Name of this metalib.
    /// (Stored on-disk as fixed size string buffer: `[u8; 128]`)
    pub name: String,
}
// fn read_metalib_header(rdr: &mut impl ReadBytesExt) -> Result<MetalibHeader>
fn read_metalib_header<T>(rdr: &mut T) -> Result<MetalibHeader>
where
    T: Read + std::io::Seek,
{
    let header = MetalibHeader {
        magic: rdr.read_u16::<LittleEndian>()?,
        build: rdr.read_u16::<LittleEndian>()?,
        platform_arch: rdr.read_u32::<LittleEndian>()?,
        size: rdr.read_u32::<LittleEndian>()?,
        field_c: rdr.read_u32::<LittleEndian>()?,
        field_10: rdr.read_u32::<LittleEndian>()?,
        field_14: rdr.read_u32::<LittleEndian>()?,
        field_18: rdr.read_u32::<LittleEndian>()?,
        id: rdr.read_i32::<LittleEndian>()?,
        xml_tag_set_ver: rdr.read_u32::<LittleEndian>()?,
        field_24: rdr.read_u32::<LittleEndian>()?,
        max_meta_num: rdr.read_i32::<LittleEndian>()?,
        cur_meta_num: rdr.read_i32::<LittleEndian>()?,
        max_macro_num: rdr.read_i32::<LittleEndian>()?,
        cur_macro_num: rdr.read_i32::<LittleEndian>()?,
        max_macros_group_num: rdr.read_i32::<LittleEndian>()?,
        cur_macros_group_num: rdr.read_i32::<LittleEndian>()?,
        field_40: rdr.read_u32::<LittleEndian>()?,
        field_44: rdr.read_u32::<LittleEndian>()?,
        version: rdr.read_u32::<LittleEndian>()?,
        ptr_macro: rdr.read_u32::<LittleEndian>()?,
        ptr_id: rdr.read_u32::<LittleEndian>()?,
        ptr_name: rdr.read_u32::<LittleEndian>()?,
        ptr_map: rdr.read_u32::<LittleEndian>()?,
        ptr_meta: rdr.read_u32::<LittleEndian>()?,
        ptr_last_meta: rdr.read_u32::<LittleEndian>()?,
        free_str_buf_size: rdr.read_i32::<LittleEndian>()?,
        ptr_str_buf: rdr.read_u32::<LittleEndian>()?,
        ptr_free_str_buf: rdr.read_u32::<LittleEndian>()?,
        ptr_macro_group_map: rdr.read_u32::<LittleEndian>()?,
        ptr_macros_group: rdr.read_u32::<LittleEndian>()?,
        field_78: rdr.read_u32::<LittleEndian>()?,
        field_7c: rdr.read_i32::<LittleEndian>()?,
        field_80: rdr.read_i32::<LittleEndian>()?,
        field_84: rdr.read_u32::<LittleEndian>()?,
        field_88: rdr.read_u32::<LittleEndian>()?,
        field_8c: rdr.read_i32::<LittleEndian>()?,
        field_90: rdr.read_i32::<LittleEndian>()?,
        name: rdr.read_fixed_size_utf8_string(128)?,
    };

    Ok(header)
}

#[derive(Debug)]
#[allow(unused)]
pub struct TDRSizeInfo {
    pub _offset: u64,
    pub n_off: i32,
    pub h_off: i32,
    pub unit_size: i32,
    pub idx_size_type: i32,
}

fn read_tdr_size_info<T>(rdr: &mut T) -> Result<TDRSizeInfo>
where
    T: ReadBytesExt + std::io::Seek,
{
    Ok(TDRSizeInfo {
        _offset: rdr.stream_position()?,
        n_off: rdr.read_i32::<LittleEndian>()?,
        h_off: rdr.read_i32::<LittleEndian>()?,
        unit_size: rdr.read_i32::<LittleEndian>()?,
        idx_size_type: rdr.read_i32::<LittleEndian>()?,
    })
}

#[derive(Debug)]
#[allow(unused)]
pub struct TDRRedirector {
    pub _offset: u64,
    pub n_off: i32,
    pub h_off: i32,
    pub unit_size: i32,
}

fn read_tdr_redirector<T>(rdr: &mut T) -> Result<TDRRedirector>
where
    T: ReadBytesExt + std::io::Seek,
{
    Ok(TDRRedirector {
        _offset: rdr.stream_position()?,
        n_off: rdr.read_i32::<LittleEndian>()?,
        h_off: rdr.read_i32::<LittleEndian>()?,
        unit_size: rdr.read_i32::<LittleEndian>()?,
    })
}

#[derive(Debug)]
#[allow(unused)]
pub struct TDRSelector {
    pub _offset: u64,
    pub unit_size: i32,
    pub h_off: i32,
    pub ptr_entry: i32,
}

fn read_tdr_selector<T>(rdr: &mut T) -> Result<TDRSelector>
where
    T: ReadBytesExt + std::io::Seek,
{
    Ok(TDRSelector {
        _offset: rdr.stream_position()?,
        unit_size: rdr.read_i32::<LittleEndian>()?,
        h_off: rdr.read_i32::<LittleEndian>()?,
        ptr_entry: rdr.read_i32::<LittleEndian>()?,
    })
}

#[derive(Debug)]
#[allow(unused)]
pub struct TDRSortKeyInfo {
    pub _offset: u64,
    pub idx_sort_entry: i32,
    pub sort_key_offset: i32,
    pub ptr_sort_key_meta: i32,
}

fn read_tdr_sort_key_info<T>(rdr: &mut T) -> Result<TDRSortKeyInfo>
where
    T: ReadBytesExt + std::io::Seek,
{
    Ok(TDRSortKeyInfo {
        _offset: rdr.stream_position()?,
        idx_sort_entry: rdr.read_i32::<LittleEndian>()?,
        sort_key_offset: rdr.read_i32::<LittleEndian>()?,
        ptr_sort_key_meta: rdr.read_i32::<LittleEndian>()?,
    })
}

#[derive(Debug)]
#[allow(unused)]
pub struct TDRDBKeyInfo {
    pub _offset: u64,
    pub h_off: i32,
    pub ptr_entry: i32,
}

fn read_tdr_db_key_info<T>(rdr: &mut T) -> Result<TDRDBKeyInfo>
where
    T: ReadBytesExt + std::io::Seek,
{
    Ok(TDRDBKeyInfo {
        _offset: rdr.stream_position()?,
        h_off: rdr.read_i32::<LittleEndian>()?,
        ptr_entry: rdr.read_i32::<LittleEndian>()?,
    })
}

#[derive(Debug)]
#[allow(unused)]
pub struct TDRIdEntry {
    pub _offset: u64,

    /// Unknown ID, always set to -1.
    pub id: i32,

    /// Offset to a TDRMeta
    pub idx: i32,
}

fn read_tdr_id_entry<T>(rdr: &mut T) -> Result<TDRIdEntry>
where
    T: ReadBytesExt + std::io::Seek,
{
    Ok(TDRIdEntry {
        _offset: rdr.stream_position()?,
        id: rdr.read_i32::<LittleEndian>()?,
        idx: rdr.read_i32::<LittleEndian>()?,
    })
}

#[derive(Debug)]
#[allow(unused)]
pub struct TDRNameEntry {
    pub _offset: u64,

    /// Offset to a GBK encoded string
    pub ptr: i32,

    /// Offset to a TDRMeta
    pub idx: i32,
}

fn read_tdr_name_entry<T>(rdr: &mut T) -> Result<TDRNameEntry>
where
    T: ReadBytesExt + std::io::Seek,
{
    Ok(TDRNameEntry {
        _offset: rdr.stream_position()?,
        ptr: rdr.read_i32::<LittleEndian>()?,
        idx: rdr.read_i32::<LittleEndian>()?,
    })
}

#[derive(Debug)]
#[allow(unused)]
pub struct TDRMapEntry {
    pub _offset: u64,

    /// Offset to a TDRMeta
    pub ptr: i32,

    /// Matches TDRMeta.mem_size
    pub size: i32,
}

fn read_tdr_map_entry<T>(rdr: &mut T) -> Result<TDRMapEntry>
where
    T: ReadBytesExt + std::io::Seek,
{
    Ok(TDRMapEntry {
        _offset: rdr.stream_position()?,
        ptr: rdr.read_i32::<LittleEndian>()?,
        size: rdr.read_i32::<LittleEndian>()?,
    })
}

#[derive(Debug)]
#[allow(unused)]
pub struct TDRMacro {
    pub _offset: u64,
    pub name: String, // ptr to name string
    pub value: i32,
    pub desc: String, // ptr to name string
    pub unk: i32,
}

fn read_tdr_macro<T>(rdr: &mut T) -> Result<TDRMacro>
where
    T: ReadBytesExt + std::io::Seek,
{
    Ok(TDRMacro {
        _offset: rdr.stream_position()?,
        name: rdr.read_null_terminated_gbk_string_i32_offset_pointer()?,
        value: rdr.read_i32::<LittleEndian>()?,
        desc: rdr.read_null_terminated_gbk_string_i32_offset_pointer()?,
        unk: rdr.read_i32::<LittleEndian>()?,
    })
}

#[derive(Debug)]
#[allow(unused)]
pub struct TDRMetaEntry {
    pub _offset: u64,
    pub id: i32,
    pub version: i32,
    pub type_: MetaPrimativeType,
    pub name: String,
    pub h_real_size: i32,
    pub n_real_size: i32,
    pub h_unit_size: i32,
    pub n_unit_size: i32,
    pub custom_h_unit_size: i32,
    pub count: i32,
    pub n_off: i32,
    pub h_off: i32,
    pub idx_id: i32,
    pub idx_version: i32,
    pub idx_count: i32,
    pub idx_type: i32,
    pub idx_custom_h_unit_size: i32,
    pub flag: TDRMetaEntryFlags,
    pub db_flag: TDRMetaEntryDBFlags,
    pub order: u8,
    pub size_info: TDRSizeInfo,
    pub referer: TDRSelector,
    pub selector: TDRSelector,
    pub io: i32,
    pub idx_io: i32,
    pub ptr_meta: i32,
    pub max_id: i32,
    pub min_id: i32,
    pub max_id_idx: i32,
    pub min_id_idx: i32,
    pub default_val_len: i32,
    pub desc: String,
    pub chinese_name: String,
    pub ptr_default_val: i32,
    pub ptr_macros_group: i32,
    pub ptr_custom_attr: i32,
    pub off_to_meta: i32,
    pub field_a8: i32,
    pub field_ac: i32,
    pub field_b0: i32,

    /// Parsed string of value at `ptr_default_val`.
    pub default_value_string: String,
}

fn read_tdr_meta_entry<T>(rdr: &mut T) -> Result<TDRMetaEntry>
where
    T: ReadBytesExt + std::io::Seek,
{
    let mut meta_entry = TDRMetaEntry {
        _offset: rdr.stream_position()?,
        id: rdr.read_i32::<LittleEndian>()?,
        version: rdr.read_i32::<LittleEndian>()?,
        type_: MetaPrimativeType::from_int(rdr.read_i32::<LittleEndian>()?)?,
        name: rdr.read_null_terminated_gbk_string_i32_offset_pointer()?,
        h_real_size: rdr.read_i32::<LittleEndian>()?,
        n_real_size: rdr.read_i32::<LittleEndian>()?,
        h_unit_size: rdr.read_i32::<LittleEndian>()?,
        n_unit_size: rdr.read_i32::<LittleEndian>()?,
        custom_h_unit_size: rdr.read_i32::<LittleEndian>()?,
        count: rdr.read_i32::<LittleEndian>()?,
        n_off: rdr.read_i32::<LittleEndian>()?,
        h_off: rdr.read_i32::<LittleEndian>()?,
        idx_id: rdr.read_i32::<LittleEndian>()?,
        idx_version: rdr.read_i32::<LittleEndian>()?,
        idx_count: rdr.read_i32::<LittleEndian>()?,
        idx_type: rdr.read_i32::<LittleEndian>()?,
        idx_custom_h_unit_size: rdr.read_i32::<LittleEndian>()?,
        flag: TDRMetaEntryFlags {
            bits: rdr.read_u16::<LittleEndian>()?,
        },
        db_flag: TDRMetaEntryDBFlags {
            bits: rdr.read_u8()?,
        },
        order: rdr.read_u8()?,
        size_info: read_tdr_size_info(rdr)?,
        referer: read_tdr_selector(rdr)?,
        selector: read_tdr_selector(rdr)?,
        io: rdr.read_i32::<LittleEndian>()?,
        idx_io: rdr.read_i32::<LittleEndian>()?,
        ptr_meta: rdr.read_i32::<LittleEndian>()?,
        max_id: rdr.read_i32::<LittleEndian>()?,
        min_id: rdr.read_i32::<LittleEndian>()?,
        max_id_idx: rdr.read_i32::<LittleEndian>()?,
        min_id_idx: rdr.read_i32::<LittleEndian>()?,
        default_val_len: rdr.read_i32::<LittleEndian>()?,
        desc: rdr.read_null_terminated_gbk_string_i32_offset_pointer()?,
        chinese_name: rdr.read_null_terminated_gbk_string_i32_offset_pointer()?,
        ptr_default_val: rdr.read_i32::<LittleEndian>()?,
        ptr_macros_group: rdr.read_i32::<LittleEndian>()?,
        ptr_custom_attr: rdr.read_i32::<LittleEndian>()?,
        off_to_meta: rdr.read_i32::<LittleEndian>()?,
        field_a8: rdr.read_i32::<LittleEndian>()?,
        field_ac: rdr.read_i32::<LittleEndian>()?,
        field_b0: rdr.read_i32::<LittleEndian>()?,
        default_value_string: "".to_string(),
    };

    if meta_entry.ptr_default_val != INVALID_METALIB_VALUE {
        let original_position = rdr.stream_position()?;
        _ = rdr.seek(SeekFrom::Start(meta_entry.ptr_default_val as u64))?;

        // Get the type info to determine how many bytes to read.
        let type_info = TDR_PRIMATIVE_TYPE_INFO
            .get(meta_entry.idx_type as usize)
            .context("Failed to get type info")?;

        // Read it and set string
        // let mut buf = vec![0; type_info.size.try_into()?];
        let default_string: String = match type_info.primative_type {
            MetaPrimativeType::UNKNOWN => unreachable!(),
            MetaPrimativeType::UNION => unreachable!(),
            MetaPrimativeType::STRUCT => unreachable!(),
            MetaPrimativeType::CHAR => format!("{:?}", rdr.read_i8()?),
            MetaPrimativeType::UCHAR => format!("{:?}", rdr.read_u8()?),
            MetaPrimativeType::BYTE => format!("{:?}", rdr.read_u8()?),
            MetaPrimativeType::SHORT => format!("{:?}", rdr.read_i16::<LittleEndian>()?),
            MetaPrimativeType::USHORT => format!("{:?}", rdr.read_u16::<LittleEndian>()?),
            MetaPrimativeType::INT => format!("{:?}", rdr.read_i32::<LittleEndian>()?),
            MetaPrimativeType::UINT => format!("{:?}", rdr.read_u32::<LittleEndian>()?),
            MetaPrimativeType::LONG => format!("{:?}", rdr.read_i32::<LittleEndian>()?),
            MetaPrimativeType::ULONG => format!("{:?}", rdr.read_u32::<LittleEndian>()?),
            MetaPrimativeType::LONGLONG => format!("{:?}", rdr.read_i64::<LittleEndian>()?),
            MetaPrimativeType::ULONGLONG => format!("{:?}", rdr.read_u64::<LittleEndian>()?),
            MetaPrimativeType::DATE => todo!(),
            MetaPrimativeType::TIME => todo!(),
            MetaPrimativeType::DATETIME => todo!(),
            MetaPrimativeType::MONEY => todo!(),
            MetaPrimativeType::FLOAT => format!("{:?}", rdr.read_f32::<LittleEndian>()?),
            MetaPrimativeType::DOUBLE => format!("{:?}", rdr.read_f64::<LittleEndian>()?),
            MetaPrimativeType::IP => todo!(),
            MetaPrimativeType::WCHAR => todo!(),
            MetaPrimativeType::STRING => {
                // println!("Reading string default at {:X}", METALIB_HEADER_SIZE as u64 + rdr.stream_position()?);
                let data = rdr.read_null_terminated_utf8_string()?;
                // println!("Data: {}", data);
                data
            },
            MetaPrimativeType::WSTRING => todo!(),
            MetaPrimativeType::VOID => unreachable!(),
        };
        // rdr.read_exact(&mut buf)?;
        // meta_entry.default_value_string = format!("{buf:?}");
        meta_entry.default_value_string = default_string;

        // Return back to read position.
        _ = rdr.seek(SeekFrom::Start(original_position))?;
    }

    Ok(meta_entry)
}

#[derive(Debug)]
#[allow(unused)]
pub struct TDRMeta {
    pub _offset: u64,
    pub flags: TDRMetaFlags,
    pub id: i32,
    pub base_version: i32,
    pub cur_version: i32,
    pub type_: MetaPrimativeType,
    pub mem_size: i32,
    pub n_unit_size: i32,
    pub h_unit_size: i32,
    pub custom_h_unit_size: i32,
    pub idx_custom_h_unit_size: i32,
    pub uncertain_max_sub_id: i32,
    pub entries_num: i32,
    pub unk_table_count: i32,
    pub unk_table_ptr: i32,
    pub unk_table_unk: i32,

    /// The start address of this TDRMeta (file address)
    pub ptr_meta: i32,

    ///  Index of this TDRMeta within the `Metalib.ptr_meta` table.
    pub idx: i32,

    /// UNCLEAR: Often exceeds the max value in the `Metalib.ptr_id` lookup map
    pub idx_id: i32,
    pub idx_type: i32,

    /// Index into the macros table of an entry containing the meta version.
    pub idx_version: i32,

    pub custom_align: i32,
    pub valid_align: i32,
    pub uncertain_version_indicator_min_ver: i32,
    pub size_type: TDRSizeInfo,
    pub version_indicator: TDRRedirector,
    pub sort_key: TDRSortKeyInfo,
    pub name: String,
    pub desc: String,
    pub chinese_name: String,
    pub split_table_factor: i32,
    pub split_table_rule_id: i16,
    pub primary_key_member_num: i16,
    pub idx_split_table_factor: i32,
    pub split_table_key: TDRDBKeyInfo,
    pub ptr_primary_key_base: i32,
    pub ptr_dependon_struct: i32,
    pub field_ac: i32,
    pub field_b0: i32,
    pub field_b4: i32,

    //entries: Array(this.iEntriesNum, TDRMetaEntry),
    pub entries: Vec<TDRMetaEntry>,
}

fn read_tdr_meta<T>(rdr: &mut T) -> Result<TDRMeta>
where
    T: ReadBytesExt + std::io::Seek,
{
    let mut meta = TDRMeta {
        _offset: rdr.stream_position()?,
        flags: TDRMetaFlags {
            bits: rdr.read_u32::<LittleEndian>()?,
        },
        id: rdr.read_i32::<LittleEndian>()?,
        base_version: rdr.read_i32::<LittleEndian>()?,
        cur_version: rdr.read_i32::<LittleEndian>()?,
        type_: MetaPrimativeType::from_int(rdr.read_i32::<LittleEndian>()?)?,
        mem_size: rdr.read_i32::<LittleEndian>()?,
        n_unit_size: rdr.read_i32::<LittleEndian>()?,
        h_unit_size: rdr.read_i32::<LittleEndian>()?,
        custom_h_unit_size: rdr.read_i32::<LittleEndian>()?,
        idx_custom_h_unit_size: rdr.read_i32::<LittleEndian>()?,
        uncertain_max_sub_id: rdr.read_i32::<LittleEndian>()?,
        entries_num: rdr.read_i32::<LittleEndian>()?,
        unk_table_count: rdr.read_i32::<LittleEndian>()?,
        unk_table_ptr: rdr.read_i32::<LittleEndian>()?,
        unk_table_unk: rdr.read_i32::<LittleEndian>()?,
        ptr_meta: rdr.read_i32::<LittleEndian>()?,
        idx: rdr.read_i32::<LittleEndian>()?,
        idx_id: rdr.read_i32::<LittleEndian>()?,
        idx_type: rdr.read_i32::<LittleEndian>()?,
        idx_version: rdr.read_i32::<LittleEndian>()?,
        custom_align: rdr.read_i32::<LittleEndian>()?,
        valid_align: rdr.read_i32::<LittleEndian>()?,
        uncertain_version_indicator_min_ver: rdr.read_i32::<LittleEndian>()?,
        size_type: read_tdr_size_info(rdr)?,
        version_indicator: read_tdr_redirector(rdr)?,
        sort_key: read_tdr_sort_key_info(rdr)?,
        name: rdr.read_null_terminated_gbk_string_i32_offset_pointer()?,
        desc: rdr.read_null_terminated_gbk_string_i32_offset_pointer()?,
        chinese_name: rdr.read_null_terminated_gbk_string_i32_offset_pointer()?,
        split_table_factor: rdr.read_i32::<LittleEndian>()?,
        split_table_rule_id: rdr.read_i16::<LittleEndian>()?,
        primary_key_member_num: rdr.read_i16::<LittleEndian>()?,
        idx_split_table_factor: rdr.read_i32::<LittleEndian>()?,
        split_table_key: read_tdr_db_key_info(rdr)?,
        ptr_primary_key_base: rdr.read_i32::<LittleEndian>()?,
        ptr_dependon_struct: rdr.read_i32::<LittleEndian>()?,
        field_ac: rdr.read_i32::<LittleEndian>()?,
        field_b0: rdr.read_i32::<LittleEndian>()?,
        field_b4: rdr.read_i32::<LittleEndian>()?,
        entries: Vec::new(),
    };

    for _i in 0..meta.entries_num {
        meta.entries.push(read_tdr_meta_entry(rdr)?);
    }

    Ok(meta)
}

#[derive(Debug)]
#[allow(unused)]
pub struct TDRMacroGroup {
    pub _offset: u64,

    pub cur_macro_count: i32,
    pub max_macro_count: i32,
    pub desc: String,
    pub _ptr_name_idx_map: i32,
    pub _ptr_value_idx_map: i32,
    pub name: String, // 128-byte

    pub name_idx_map: Vec<i32>,
    pub value_idx_map: Vec<i32>,
}

fn read_tdr_macros_group<T>(rdr: &mut T) -> Result<TDRMacroGroup>
where
    T: ReadBytesExt + std::io::Seek,
{
    let offset = rdr.stream_position()?;
    let mut macros_group = TDRMacroGroup {
        _offset: offset,
        cur_macro_count: rdr.read_i32::<LittleEndian>()?,
        max_macro_count: rdr.read_i32::<LittleEndian>()?,
        desc: rdr.read_null_terminated_gbk_string_i32_offset_pointer()?,
        _ptr_name_idx_map: rdr.read_i32::<LittleEndian>()?,
        _ptr_value_idx_map: rdr.read_i32::<LittleEndian>()?,
        name: rdr.read_fixed_size_utf8_string(128)?,
        name_idx_map: Vec::new(),
        value_idx_map: Vec::new(),
    };

    // let original_position = rdr.stream_position()?;
    assert_eq!(
        macros_group._ptr_name_idx_map as u64,
        rdr.stream_position()? - offset
    );
    //_ = rdr.seek(SeekFrom::Start(offset + macros_group._ptr_name_idx_map as u64))?;
    for _i in 0..macros_group.cur_macro_count {
        macros_group
            .name_idx_map
            .push(rdr.read_i32::<LittleEndian>()?);
    }

    assert_eq!(
        macros_group._ptr_value_idx_map as u64,
        rdr.stream_position()? - offset
    );
    // _ = rdr.seek(SeekFrom::Start(offset + macros_group._ptr_value_idx_map as u64))?;
    for _i in 0..macros_group.cur_macro_count {
        macros_group
            .value_idx_map
            .push(rdr.read_i32::<LittleEndian>()?);
    }

    // _ = rdr.seek(SeekFrom::Start(original_position))?;

    Ok(macros_group)
}

#[derive(Debug)]
#[allow(unused)]
pub struct Metalib {
    pub _offset: u64,
    pub header: MetalibHeader,

    pub macros: Vec<TDRMacro>,
    pub ids: Vec<TDRIdEntry>,
    pub names: Vec<TDRNameEntry>,
    pub meta_map: Vec<TDRMapEntry>,
    pub metas: Vec<TDRMeta>,
    // pub macrogroup_map: Vec<TDRMapEntry>,
    pub macrogroups: Vec<TDRMacroGroup>,
}

impl Metalib {
    /// Returns the first TDRMeta found with the given ID
    #[allow(unused)]
    pub fn get_meta_by_id(&self, id: i32) -> Result<&TDRMeta> {
        if id == INVALID_METALIB_VALUE {
            return Err(anyhow!("Invalid meta ID (-1)"));
        }

        for entry in self.metas.iter() {
            if entry.id == id {
                return Ok(entry);
            }
        }

        Err(anyhow!("Failed to get meta by id"))
    }

    /// Get a meta by the given (file) offset.
    #[allow(unused)]
    pub fn get_meta_by_offset(&self, offset: i32) -> Result<&TDRMeta> {
        if offset == INVALID_METALIB_VALUE {
            return Err(anyhow!("Invalid meta offset (-1)"));
        }

        for entry in self.metas.iter() {
            if entry._offset == offset as u64 {
                return Ok(entry);
            }
        }

        Err(anyhow!("Failed to get meta by offset"))
    }

    /// Get a macrogroup by the given (file) offset.
    #[allow(unused)]
    pub fn get_macrogroup_by_offset(&self, offset: i32) -> Result<&TDRMacroGroup> {
        if offset == INVALID_METALIB_VALUE {
            return Err(anyhow!("Invalid meta offset (-1)"));
        }

        for entry in self.macrogroups.iter() {
            if entry._offset == offset as u64 {
                return Ok(entry);
            }
        }

        Err(anyhow!("Failed to get macrogroup by offset"))
    }

    /// Returns true if the provided macro is in ANY macrogroup.
    pub fn is_macro_in_group(&self, tdr_macro: &TDRMacro) -> Result<bool> {
        // Doesn't need to be fast, but I probably should have done better than this:
        for group in self.macrogroups.iter() {
            for &tdr_macro_idx in group.value_idx_map.iter() {
                let cur_tdr_macro = self.macros.get(tdr_macro_idx as usize).context("Failed toget macro by idx")?;
                if cur_tdr_macro._offset == tdr_macro._offset {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }
}

pub fn read_metalib<T>(rdr: &mut T) -> Result<Metalib>
where
    T: Read + ReadBytesExt + std::io::Seek,
{
    let _offset = rdr.stream_position()?;
    let header = read_metalib_header(rdr)?;

    let mut metadata_body: Vec<u8> = vec![0; (header.size - METALIB_HEADER_SIZE).try_into()?];
    rdr.read_exact(&mut metadata_body)?;
    let mut rdr = Cursor::new(metadata_body);

    // Macro Table
    _ = rdr.seek(SeekFrom::Start(header.ptr_macro as u64));
    let mut macros: Vec<TDRMacro> = Vec::new();
    for _ in 0..header.cur_macro_num {
        let entry = read_tdr_macro(&mut rdr)?;
        macros.push(entry);
    }

    // ID Table
    _ = rdr.seek(SeekFrom::Start(header.ptr_id as u64));
    let mut ids: Vec<TDRIdEntry> = Vec::new();
    for _ in 0..header.cur_meta_num {
        let entry = read_tdr_id_entry(&mut rdr)?;
        //assert_eq!(entry.id, -1);
        ids.push(entry);
    }

    // Name Table
    _ = rdr.seek(SeekFrom::Start(header.ptr_name as u64));
    let mut names: Vec<TDRNameEntry> = Vec::new();
    for _ in 0..header.cur_meta_num {
        let entry = read_tdr_name_entry(&mut rdr)?;
        names.push(entry);
    }

    // Meta Map
    _ = rdr.seek(SeekFrom::Start(header.ptr_map as u64));
    let mut meta_map: Vec<TDRMapEntry> = Vec::new();
    for _ in 0..header.cur_meta_num {
        let entry = read_tdr_map_entry(&mut rdr)?;
        meta_map.push(entry);
    }

    // Meta Table
    _ = rdr.seek(SeekFrom::Start(header.ptr_meta as u64));
    let mut metas: Vec<TDRMeta> = Vec::new();
    for _ in 0..header.cur_meta_num {
        let entry = read_tdr_meta(&mut rdr)?;
        metas.push(entry);
    }

    // // MacroGroup Map
    // _ = rdr.seek(SeekFrom::Start(header.ptr_macro_group_map as u64));
    // let mut macrogroup_map: Vec<TDRMapEntry> = Vec::new();
    // for _ in 0..header.cur_meta_num {
    //     let entry = read_tdr_map_entry(&mut rdr)?;
    //     macrogroup_map.push(entry);
    // }

    // MacroGroup table
    _ = rdr.seek(SeekFrom::Start(header.ptr_macros_group as u64));
    let mut macrogroups: Vec<TDRMacroGroup> = Vec::new();
    for _ in 0..header.cur_macros_group_num {
        let entry = read_tdr_macros_group(&mut rdr)?;
        macrogroups.push(entry);
    }

    Ok(Metalib {
        _offset,
        macros,
        header,
        ids,
        names,
        meta_map,
        metas,
        // macrogroup_map,
        macrogroups,
    })
}
