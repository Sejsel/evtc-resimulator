use nom::IResult;
use nom::number::complete::*;
use std::str::{from_utf8, Utf8Error};
use nom::bytes::complete::take;
use nom::multi::many0;
use crate::evtc::{EvtcAgent, EvtcSkill, EvtcCombatItem, EvtcLog};

// log metadata
// 12 bytes: arc build version (string)
// 1 byte: revision
// 2 bytes: boss species id (u16)
// 1 byte unused

// agents
// i32: agent count
// 96B agent:
// - u64 address
// - u32 profession
// - u32 is_elite
// - i16 toughness
// - i16 concentration
// - i16 healing
// - i16 hitbox_width
// - i16 condition
// - i16 hitbox_height
// - 68 bytes name (utf-8 string)

// skills
// i32: skill count
// 68 byte: skill
// i32: skill ID
// 64 bytes: name

// combat items
// 64 bytes until end


fn parse_arc_string(i: &[u8]) -> Result<String, Utf8Error> {
    from_utf8(i).map(|str| str.trim_matches(|c| c == '\0').to_string())
}

fn parse_agent(i: &[u8]) -> IResult<&[u8], EvtcAgent> {
    let (i, address) = le_u64(i)?;
    let (i, profession) = le_u32(i)?;
    let (i, is_elite) = le_u32(i)?;
    let (i, toughness) = le_i16(i)?;
    let (i, concentration) = le_i16(i)?;
    let (i, healing) = le_i16(i)?;
    let (i, condition) = le_i16(i)?;
    let (i, hitbox_width) = le_i16(i)?;
    let (i, hitbox_height) = le_i16(i)?;
    let (i, name_bytes) = take(68usize)(i)?;

    Ok((i, EvtcAgent {
        address,
        profession,
        is_elite,
        toughness,
        concentration,
        healing,
        condition,
        hitbox_width,
        hitbox_height,
        name: parse_arc_string(name_bytes).unwrap_or("Invalid UTF-8 in name".to_string()),
    }))
}
fn parse_agents(i: &[u8]) -> IResult<&[u8], Vec<EvtcAgent>> {
    let (i, count) = le_i32(i)?;
    let (i, agents) = nom::multi::count(parse_agent, count as usize)(i)?;

    Ok((i, agents))
}

fn parse_skill(i: &[u8]) -> IResult<&[u8], EvtcSkill> {
    let (i, id) = le_i32(i)?;
    let (i, name_bytes) = take(64usize)(i)?;

    Ok((i, EvtcSkill {
        id,
        name: parse_arc_string(name_bytes).unwrap_or("Invalid UTF-8 in name".to_string()),
    }))
}

fn parse_skills(i: &[u8]) -> IResult<&[u8], Vec<EvtcSkill>> {
    let (i, count) = le_i32(i)?;
    let (i, skills) = nom::multi::count(parse_skill, count as usize)(i)?;

    Ok((i, skills))
}

fn parse_combat_item(i: &[u8]) -> IResult<&[u8], EvtcCombatItem> {
    let (i, time) = le_i64(i)?;
    let (i, src_agent) = le_u64(i)?;
    let (i, dst_agent) = le_u64(i)?;
    let (i, value) = le_i32(i)?;
    let (i, buff_dmg) = le_i32(i)?;
    let (i, overstack_value) = le_u32(i)?;
    let (i, skill_id) = le_u32(i)?;
    let (i, src_inst_id) = le_u16(i)?;
    let (i, dst_inst_id) = le_u16(i)?;
    let (i, src_master_inst_id) = le_u16(i)?;
    let (i, dst_master_inst_id) = le_u16(i)?;
    let (i, iff) = u8(i)?;
    let (i, buff) = u8(i)?;
    let (i, result) = u8(i)?;
    let (i, is_activation) = u8(i)?;
    let (i, is_buff_remove) = u8(i)?;
    let (i, is_ninety) = u8(i)?;
    let (i, is_fifty) = u8(i)?;
    let (i, is_moving) = u8(i)?;
    let (i, is_state_change) = u8(i)?;
    let (i, is_flanking) = u8(i)?;
    let (i, is_shields) = u8(i)?;
    let (i, is_offcycle) = u8(i)?;
    let (i, padding) = le_u32(i)?;

    Ok((i, EvtcCombatItem {
        time,
        src_agent,
        dst_agent,
        value,
        buff_dmg,
        overstack_value,
        skill_id,
        src_inst_id,
        dst_inst_id,
        src_master_inst_id,
        dst_master_inst_id,
        iff,
        is_buff: buff,
        result,
        is_activation,
        is_buff_remove,
        is_ninety,
        is_fifty,
        is_moving,
        is_state_change,
        is_flanking,
        is_shields,
        is_offcycle,
        padding
    }))
}

fn parse_combat_items(i: &[u8]) -> IResult<&[u8], Vec<EvtcCombatItem>> {
    many0(parse_combat_item)(i)
}

pub fn evtc_parser(i: &[u8]) -> IResult<&[u8], EvtcLog> {
    let (i, version_bytes) = nom::bytes::complete::take(12usize)(i)?;
    let (i, revision) = u8(i)?;
    if revision != 1 {
        // The API for custom errors is insane, TODO: fix properly
        panic!("Only revision 1 is supported");
    }
    let (i, boss_species_id) = le_u16(i)?;
    let (i, _) = u8(i)?; // unused byte
    let (i, agents) = parse_agents(i)?;
    let (i, skills) = parse_skills(i)?;
    let (i, combat_items) = parse_combat_items(i)?;

    Ok((i, EvtcLog {
        version: parse_arc_string(version_bytes).unwrap_or("???".to_string()),
        revision,
        boss_species_id,
        agents,
        skills,
        combat_items
    }))
}
