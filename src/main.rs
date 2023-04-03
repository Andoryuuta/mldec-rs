mod metalib;
mod reader_utils;

use anyhow::{anyhow, Context, Result};
use metalib::{
    read_metalib, MetaPrimativeType, Metalib, TDRMetaEntryDBFlags, TDRMetaEntryFlags, TDRMetaFlags,
    INVALID_METALIB_VALUE,
};

// Needed to prevent namespace clash.
use std::fmt::Write as _;
use std::io::Write as _;

use std::io::{prelude::*, BufReader, SeekFrom};
use std::path::Path;
use std::{env, fs::File};

fn walk_meta_for_net_offset_field_name(
    metalib: &Metalib,
    meta: &metalib::TDRMeta,
    search_net_offset: i32,
    current_base: i32,
    current_path: String,
) -> Result<String> {
    for entry in meta.entries.iter() {
        // Skip any that don't contain our search range
        let entry_start = current_base + entry.n_off;
        let entry_end = current_base + entry.n_off + entry.n_unit_size;
        if entry_start > search_net_offset || entry_end <= search_net_offset {
            continue;
        }

        if entry.type_ == metalib::MetaPrimativeType::STRUCT {
            let referenced_meta = metalib.get_meta_by_offset(entry.ptr_meta)?;
            return walk_meta_for_net_offset_field_name(
                metalib,
                referenced_meta,
                search_net_offset,
                entry_start,
                format!("{}.", entry.name),
            );
        } else if entry_start == search_net_offset {
            return Ok(format!("{}{}", current_path, entry.name));
        }
    }

    Err(anyhow!("Failed to find meta field by net offset!"))
}

fn walk_meta_for_host_offset_field_name(
    metalib: &Metalib,
    meta: &metalib::TDRMeta,
    search_host_offset: i32,
    current_base: i32,
    current_path: String,
) -> Result<String> {
    for entry in meta.entries.iter() {
        // Skip any that don't contain our search range
        let entry_start = current_base + entry.h_off;
        let entry_end = current_base + entry.h_off + entry.h_unit_size;
        if entry_start > search_host_offset || entry_end <= search_host_offset {
            continue;
        }

        if entry.type_ == metalib::MetaPrimativeType::STRUCT {
            let referenced_meta = metalib.get_meta_by_offset(entry.ptr_meta)?;
            return walk_meta_for_host_offset_field_name(
                metalib,
                referenced_meta,
                search_host_offset,
                entry_start,
                format!("{}.", entry.name),
            );
        } else if entry_start == search_host_offset {
            return Ok(format!("{}{}", current_path, entry.name));
        }
    }

    Err(anyhow!("Failed to find meta field by host offset!"))
}

fn resolve_meta_entry_name_by_net_offset(
    metalib: &Metalib,
    meta: &metalib::TDRMeta,
    search_net_offset: i32,
) -> Result<String> {
    walk_meta_for_net_offset_field_name(metalib, meta, search_net_offset, 0, "".to_string())
}

fn resolve_meta_entry_name_by_host_offset(
    metalib: &Metalib,
    meta: &metalib::TDRMeta,
    search_host_offset: i32,
) -> Result<String> {
    walk_meta_for_host_offset_field_name(metalib, meta, search_host_offset, 0, "".to_string())
}

fn dump_tdr_macro_xml(tdr_macro: &metalib::TDRMacro) -> Result<String> {
    let mut out = String::new();
    write!(&mut out, "<macro")?;
    write!(&mut out, " name=\"{}\"", tdr_macro.name)?;
    write!(&mut out, " value=\"{}\"", tdr_macro.value)?;
    if !tdr_macro.desc.is_empty() {
        write!(&mut out, " desc=\"{}\"", tdr_macro.desc)?;
    }
    write!(&mut out, " />")?;
    Ok(out)
}

fn dump_tdr_macrogroup_xml(
    metalib: &Metalib,
    macrogroup: &metalib::TDRMacroGroup,
) -> Result<String> {
    let mut out = String::new();

    // Open `macrosgroup` tag.
    let mut macrogroup_tag = String::new();
    write!(&mut macrogroup_tag, "\t<macrosgroup")?;
    write!(&mut macrogroup_tag, " name=\"{}\"", macrogroup.name)?;
    if !macrogroup.desc.is_empty() {
        write!(&mut macrogroup_tag, " desc=\"{}\"", macrogroup.desc)?;
    }
    write!(&mut macrogroup_tag, ">")?;
    writeln!(&mut out, "{macrogroup_tag}")?;

    // Write macro entries
    for &tdr_macro_idx in macrogroup.value_idx_map.iter() {
        assert!(tdr_macro_idx >= 0);
        let tdr_macro = metalib.macros.get(tdr_macro_idx as usize).unwrap();
        let macro_tag = dump_tdr_macro_xml(tdr_macro)?;
        writeln!(&mut out, "\t\t{macro_tag}")?;
    }

    // Close `macrosgroup` tag.
    write!(&mut out, "\t</macrosgroup>")?;

    Ok(out)
}

fn dump_tdr_meta_entry_xml(
    metalib: &Metalib,
    meta: &metalib::TDRMeta,
    meta_entry: &metalib::TDRMetaEntry,
) -> Result<String> {
    // Open `entry` tag.
    let mut out = String::new();
    write!(&mut out, "<entry")?;
    write!(&mut out, " name=\"{}\"", meta_entry.name)?;

    // Write "type" attribute
    let type_string: &str = {
        if meta_entry.ptr_meta != INVALID_METALIB_VALUE {
            let type_meta = metalib
                .get_meta_by_offset(meta_entry.ptr_meta)
                .context("Failed to get meta by ptr_meta")?;
            &type_meta.name
        } else if meta_entry.idx_type != INVALID_METALIB_VALUE {
            let type_info = metalib::TDR_PRIMATIVE_TYPE_INFO
                .get(meta_entry.idx_type as usize)
                .context("Failed to get type info")?;

            type_info.xml_name
        } else {
            ""
        }
    };
    let type_prefix = {
        if meta_entry.flag.contains(TDRMetaEntryFlags::POINT_TYPE) {
            "*"
        } else if meta_entry.flag.contains(TDRMetaEntryFlags::REFER_TYPE) {
            "@"
        } else {
            ""
        }
    };
    write!(&mut out, " type=\"{type_prefix}{type_string}\"")?;

    // Write `count` attribute
    if meta_entry.count > 1 {
        if meta_entry.idx_count != INVALID_METALIB_VALUE {
            let count_macro = metalib
                .macros
                .get(meta_entry.idx_count as usize)
                .context("Failed to get macro by meta_entry.idx_count")?;
            write!(&mut out, " count=\"{}\"", count_macro.name)?;
        } else {
            write!(&mut out, " count=\"{}\"", meta_entry.count)?;
        }
    }

    // Write `version` attribute
    if meta_entry.version != meta.base_version{
        if meta_entry.idx_version != INVALID_METALIB_VALUE {
            let version_macro = metalib
                .macros
                .get(meta_entry.idx_version as usize)
                .context("Failed to get macro by meta_entry.idx_version")?;
            write!(&mut out, " version=\"{}\"", version_macro.name)?;
        } else {
            write!(&mut out, " version=\"{}\"", meta_entry.version)?;
        }
    }

    // Write `id` attribute
    if meta_entry.idx_id != INVALID_METALIB_VALUE {
        let id_macro = metalib
            .macros
            .get(meta_entry.idx_id as usize)
            .context("Failed to get macro by meta_entry.idx_id")?;
        write!(&mut out, " id=\"{}\"", id_macro.name)?;
    } else if meta_entry.id != INVALID_METALIB_VALUE {
        write!(&mut out, " id=\"{}\"", meta_entry.id)?;
    }

    // Write `size` attribute
    if meta_entry.idx_custom_h_unit_size != INVALID_METALIB_VALUE {
        let id_macro = metalib
            .macros
            .get(meta_entry.idx_custom_h_unit_size as usize)
            .context("Failed to get macro by meta_entry.idx_custom_h_unit_size")?;
        write!(&mut out, " size=\"{}\"", id_macro.name)?;
    } else if meta_entry.custom_h_unit_size > 0 {
        let type_info = metalib::TDR_PRIMATIVE_TYPE_INFO
            .get(meta_entry.idx_type as usize)
            .context("Failed to get type info")?;
        write!(
            &mut out,
            " size=\"{}\"",
            meta_entry.custom_h_unit_size / type_info.size
        )?;
    }

    if !meta_entry.chinese_name.is_empty() {
        write!(&mut out, " cname=\"{}\"", meta_entry.chinese_name)?;
    }

    if !meta_entry.desc.is_empty() {
        write!(&mut out, " desc=\"{}\"", meta_entry.desc)?;
    }

    if meta_entry.db_flag.contains(TDRMetaEntryDBFlags::UNIQUE) {
        write!(&mut out, " unique=\"true\"")?;
    }

    if meta_entry.db_flag.contains(TDRMetaEntryDBFlags::NOT_NULL) {
        write!(&mut out, " notnull=\"true\"")?;
    }

    // Write `refer` attribute
    // TODO: Can we just do this by net offset?
    if meta_entry.referer.h_off != INVALID_METALIB_VALUE {
        write!(
            &mut out,
            " refer=\"{}\"",
            resolve_meta_entry_name_by_host_offset(metalib, meta, meta_entry.referer.h_off)?
        )?;
    }

    // Write `default` attribute
    // TODO: Update default value reader to parse value instead of bytes if needed.
    if meta_entry.ptr_default_val != INVALID_METALIB_VALUE {
        write!(&mut out, " default=\"{}\"", meta_entry.default_value_string)?;
    }

    // Write `sizeinfo` attribute
    if meta_entry.size_info.unit_size > 0 {
        if meta_entry.size_info.idx_size_type != INVALID_METALIB_VALUE {
            let type_info = metalib::TDR_PRIMATIVE_TYPE_INFO
                .get(meta_entry.size_info.idx_size_type as usize)
                .context("Failed to get type info from meta_entry.size_info.idx_size_type")?;

            if (type_info.primative_type != MetaPrimativeType::STRING
                && type_info.primative_type != MetaPrimativeType::WSTRING)
                || type_info.xml_name == "int"
            {
                write!(&mut out, " sizeinfo=\"{}\"", type_info.xml_name)?;
            }
        } else if meta_entry.size_info.n_off != INVALID_METALIB_VALUE {
            write!(
                &mut out,
                " sizeinfo=\"{}\"",
                resolve_meta_entry_name_by_net_offset(metalib, meta, meta_entry.size_info.n_off)?
            )?;
        }
    }

    // Write `sortMethod` attribute
    if meta_entry.count > 1 && (meta_entry.order == 1 || meta_entry.order == 2) {
        let sort_order = {
            match meta_entry.order {
                1 => "asc",
                2 => "desc",
                _ => unreachable!(),
            }
        };
        write!(&mut out, " sortMethod=\"{sort_order}\"")?;
    }

    // Write `io` attribute
    if meta_entry.io != 0 {
        let io_type = {
            match meta_entry.io {
                1 => "noinput",
                2 => "nooutput",
                3 => "noio",
                _ => unreachable!(),
            }
        };

        write!(&mut out, " io=\"{io_type}\"")?;
    }

    // Write `select` attribute
    if meta_entry.type_ == MetaPrimativeType::UNION
        && meta_entry.selector.h_off != INVALID_METALIB_VALUE
    {
        let select_field =
            resolve_meta_entry_name_by_host_offset(metalib, meta, meta_entry.selector.h_off)?;
        write!(&mut out, " select=\"{select_field}\"")?;
    }

    if meta_entry.flag.contains(TDRMetaEntryFlags::HAS_MAXMIN_ID) {
        // Write `minid` attribute
        if meta_entry.min_id_idx != INVALID_METALIB_VALUE {
            let min_id_macro = metalib
                .macros
                .get(meta_entry.min_id_idx as usize)
                .context("Failed to get macro by meta_entry.min_id_idx")?;
            write!(&mut out, " minid=\"{}\"", min_id_macro.name)?;
        } else {
            write!(&mut out, " minid=\"{}\"", meta_entry.min_id)?;
        }

        // Write `maxid` attribute
        if meta_entry.max_id_idx != INVALID_METALIB_VALUE {
            let max_id_macro = metalib
                .macros
                .get(meta_entry.max_id_idx as usize)
                .context("Failed to get macro by meta_entry.max_id_idx")?;
            write!(&mut out, " maxid=\"{}\"", max_id_macro.name)?;
        } else {
            write!(&mut out, " maxid=\"{}\"", meta_entry.max_id)?;
        }
    }

    // Unused `extendtotable` attribute
    if meta_entry
        .db_flag
        .contains(TDRMetaEntryDBFlags::EXTEND_TO_TABLE)
    {
        todo!()
    }

    // Write `bindmacrosgroup` attribute
    if meta_entry.ptr_macros_group != INVALID_METALIB_VALUE {
        let macro_group = metalib.get_macrogroup_by_offset(meta_entry.ptr_macros_group)?;
        write!(&mut out, " bindmacrosgroup=\"{}\"", macro_group.name)?;
    }

    // Unused `autoincrement` attribute
    if meta_entry
        .db_flag
        .contains(TDRMetaEntryDBFlags::AUTO_INCREMENT)
    {
        todo!()
    }

    // Unused `customattr` attribute
    if meta_entry.ptr_custom_attr != INVALID_METALIB_VALUE {
        todo!()
    }

    // Close tag
    write!(&mut out, "/>")?;

    Ok(out)
}

fn dump_tdr_meta_xml(metalib: &Metalib, meta: &metalib::TDRMeta) -> Result<String> {
    let mut out = String::new();

    let tag_name = match meta.type_ {
        metalib::MetaPrimativeType::UNION => "union",
        metalib::MetaPrimativeType::STRUCT => "struct",
        _ => unreachable!(),
    };
    write!(&mut out, "\t<{tag_name}")?;
    write!(&mut out, " name=\"{}\"", meta.name)?;

    if meta.idx_version != INVALID_METALIB_VALUE {
        let version_macro = metalib
            .macros
            .get(meta.idx_version as usize)
            .context("Error getting macro by idx_version")?;
        write!(&mut out, " version=\"{}\"", version_macro.name)?;
    } else {
        write!(&mut out, " version=\"{}\"", meta.base_version)?;
    }

    if meta.flags.contains(TDRMetaFlags::HAS_ID) {
        if meta.idx_id != INVALID_METALIB_VALUE {
            let id_macro = metalib
                .macros
                .get(meta.idx_id as usize)
                .context("Error getting macro by idx_id")?;
            write!(&mut out, " id=\"{}\"", id_macro.name)?;
        } else {
            write!(&mut out, " id=\"{}\"", meta.id)?;
        }
    }

    if !meta.chinese_name.is_empty() {
        write!(&mut out, " cname=\"{}\"", meta.chinese_name)?;
    }

    if !meta.desc.is_empty() {
        write!(&mut out, " desc=\"{}\"", meta.desc)?;
    }

    // Fields diverge here depending on if this is a union or a struct tag.
    if meta.type_ == metalib::MetaPrimativeType::STRUCT {
        // Write `size` tag
        if meta.idx_custom_h_unit_size != INVALID_METALIB_VALUE {
            let custom_host_size_macro =
                metalib
                    .macros
                    .get(meta.idx_custom_h_unit_size as usize)
                    .context("Error getting macro by idx_custom_h_unit_size")?;
            write!(&mut out, " size=\"{}\"", custom_host_size_macro.name)?;
        } else if meta.custom_h_unit_size > 0 {
            write!(&mut out, " size=\"{}\"", meta.custom_h_unit_size)?;
        }

        // Write custom align tag
        // (Always defaults to 1, tag skipped if default.)
        if meta.custom_align != 1 {
            write!(&mut out, " align=\"{}\"", meta.custom_align)?;
        }

        // Write `versionindicator` tag
        if meta.version_indicator.n_off != INVALID_METALIB_VALUE {
            write!(
                &mut out,
                " versionindicator=\"{}\"",
                resolve_meta_entry_name_by_net_offset(metalib, meta, meta.version_indicator.n_off)?
            )?;
        }

        if meta.size_type.unit_size > 0 {
            if meta.size_type.idx_size_type != INVALID_METALIB_VALUE {
                let type_info = metalib::TDR_PRIMATIVE_TYPE_INFO
                    .get(meta.size_type.idx_size_type as usize)
                    .context("Failed to get type info")?;

                write!(&mut out, " sizeinfo=\"{}\"", type_info.xml_name)?;
            } else if meta.size_type.n_off != INVALID_METALIB_VALUE {
                write!(
                    &mut out,
                    " sizeinfo=\"{}\"",
                    resolve_meta_entry_name_by_net_offset(metalib, meta, meta.size_type.n_off)?
                )?;
            }
        }

        // None of our example metalibs have this field -- untested.
        if meta.sort_key.sort_key_offset != INVALID_METALIB_VALUE {
            write!(
                &mut out,
                " sortkey=\"{}\"",
                resolve_meta_entry_name_by_net_offset(
                    metalib,
                    meta,
                    meta.sort_key.sort_key_offset
                )?
            )?;
        }

        // Unused `primarykey` attribute.
        if meta.primary_key_member_num > 0 && meta.ptr_primary_key_base != INVALID_METALIB_VALUE {
            unimplemented!()
        }

        // Unused `splittablefactor` attribute
        if meta.idx_split_table_factor != INVALID_METALIB_VALUE {
            unimplemented!()
        }

        // Unused `splittablekey` attribute
        if meta.split_table_key.h_off != INVALID_METALIB_VALUE {
            unimplemented!()
        }

        // Unused `splittablerule` attribute
        // Always defaults to 0 if unused.
        if meta.split_table_rule_id != 0 {
            unimplemented!()
        }

        // Unused `dependontable` attribute
        if meta.ptr_dependon_struct != INVALID_METALIB_VALUE {
            unimplemented!()
        }

        // Unused `uniqueentryname` attribute.
        if meta
            .flags
            .contains(TDRMetaFlags::NEED_PREFIX_FOR_UNIQUENAME)
        {
            unimplemented!()
        }
    }
    writeln!(&mut out, ">")?;

    // Write meta entries....
    for entry in meta.entries.iter() {
        writeln!(
            &mut out,
            "\t\t{}",
            dump_tdr_meta_entry_xml(metalib, meta, entry)?
        )?;
    }

    // Something ends the tag.
    writeln!(&mut out, "\t</{tag_name}>")?;

    Ok(out)
}

fn dump_metalib_xml(metalib: &Metalib) -> Result<String> {
    let mut out = String::new();

    let header = &metalib.header;
    writeln!(
        &mut out,
        r#"<?xml version="1.0" encoding="UTF8" standalone="yes" ?>"#
    )?;

    // Open `metalib` tag.
    let mut metaline_tag = String::new();
    write!(&mut metaline_tag, "<metalib")?;
    write!(
        &mut metaline_tag,
        " tagsetversion=\"{}\"",
        header.xml_tag_set_ver
    )?;
    write!(&mut metaline_tag, " name=\"{}\"", header.name)?;
    write!(&mut metaline_tag, " version=\"{}\"", header.version)?;
    if header.id != metalib::INVALID_METALIB_VALUE {
        write!(&mut metaline_tag, " id=\"{}\"", header.id)?;
    }
    write!(&mut metaline_tag, ">")?;
    writeln!(&mut out, "{metaline_tag}")?;

    // Write macros that are unassociated with a group.

    for macro_ in metalib.macros.iter() {
        if !metalib.is_macro_in_group(macro_)? {
            writeln!(
                &mut out,
                "\t{}",
                dump_tdr_macro_xml(macro_)?
            )?;
        }
    }

    // Write macro groups
    for macrogroup in metalib.macrogroups.iter() {
        writeln!(
            &mut out,
            "{}",
            dump_tdr_macrogroup_xml(metalib, macrogroup)?
        )?;
    }

    // Write unions/structs.
    for meta in metalib.metas.iter() {
        writeln!(&mut out, "{}", dump_tdr_meta_xml(metalib, meta)?)?;
    }

    // Close `metalib` tag.
    writeln!(&mut out, "</metalib>")?;

    Ok(out)
}

fn export_metalib_xml(metalib: &Metalib) -> Result<String> {
    let mut out = String::new();
    write!(&mut out, "{}", dump_metalib_xml(metalib)?)?;
    Ok(out)
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: mldec <path to file containg compiled metalib> <hex offset>");
        anyhow::bail!("Not enough arguments");
    }

    let input_filepath = &args[1];
    let offset = &args[2];
    let offset =
        u64::from_str_radix(offset.trim_start_matches("0x"), 16).expect("unable to parse offset");

    println!("Attempting to load TDR Metalib in file:{input_filepath}, offset:{offset:X}");

    // Read metalib
    let mut file = BufReader::new(File::open(input_filepath)?);
    _ = file.seek(SeekFrom::Start(offset));
    let metalib = read_metalib(&mut file)?;

    let _xml_data = export_metalib_xml(&metalib)?;
    // println!("{_xml_data}");


    // Find input file name
    let input_path_stem: String = Path::new(input_filepath).file_stem().unwrap().to_string_lossy().to_string();

    let mut file = File::create(format!("./output/{input_path_stem}.xml"))?;
    file.write_all(_xml_data.as_bytes())?;

    Ok(())
}
