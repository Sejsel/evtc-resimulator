use crate::ids;
use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::io::BufRead;

// WARNING: This data is from 2021-03-28, it may change in the future.

// This hack is needed because the skill shares the same id for first and later strikes
pub const SEARING_FISSURE_FIRST_STRIKE_MULTIPLIER: f64 = 0.5;
pub const SEARING_FISSURE_ADDITIONAL_STRIKE_MULTIPLIER: f64 = 0.25;
pub const SEARING_FISSURE_FIRST_STRIKE_BURNING_DURATION: u32 = 3000;
pub const SEARING_FISSURE_ADDITIONAL_STRIKE_BURNING_DURATION: u32 = 1000;

pub const BASE_ENEMY_ARMOR: u32 = 1223;

pub const BLEEDING_BASE_DAMAGE: f64 = 22.0;
pub const BLEEDING_MULTIPLIER: f64 = 0.06;
pub const BURNING_BASE_DAMAGE: f64 = 131.;
pub const BURNING_MULTIPLIER: f64 = 0.155;
pub const POISONED_BASE_DAMAGE: f64 = 33.5;
pub const POISONED_MULTIPLIER: f64 = 0.06;
pub const CONFUSION_BASE_DAMAGE: f64 = 10.;
pub const CONFUSION_MULTIPLIER: f64 = 0.;
pub const CONFUSION_ACTIVE_BASE_DAMAGE: f64 = 95.5;
pub const CONFUSION_ACTIVE_MULTIPLIER: f64 = 0.195;
pub const TORMENT_BASE_DAMAGE: f64 = 31.8;
pub const TORMENT_MULTIPLIER: f64 = 0.09;
pub const TORMENT_MOVING_BASE_DAMAGE: f64 = 22.;
pub const TORMENT_MOVING_MULTIPLIER: f64 = 0.06;

pub const BATTLE_SCARS_DAMAGE: f64 = 298.0;
pub const BATTLE_SCARS_MULTIPLIER: f64 = 0.1;

#[derive(Eq, PartialEq, Debug)]
pub enum SkillType {
    Unknown,
    Ability,
    Condition,
    GenericBuff,
    Boon,
}

pub enum BuffStackingType {
    Intensity,
    Duration
}

pub fn get_skill_type(skill_id: u32) -> SkillType {
    match skill_id {
        ids::skills::BLEEDING => SkillType::Condition,
        ids::skills::BURNING => SkillType::Condition,
        ids::skills::CONFUSION => SkillType::Condition,
        ids::skills::POISONED => SkillType::Condition,
        ids::skills::TORMENT => SkillType::Condition,
        ids::skills::CHILLED => SkillType::Condition,
        ids::skills::VULNERABILITY => SkillType::Condition,
        ids::skills::MIGHT => SkillType::Boon,
        ids::skills::FURY => SkillType::Boon,
        ids::skills::KALLAS_FERVOR => SkillType::GenericBuff,
        _ => SkillType::Unknown,
    }
}

pub fn get_stack_limit(skill_id: u32) -> u32 {
    match skill_id {
        ids::skills::MIGHT => 25,
        ids::skills::KALLAS_FERVOR => 5,
        ids::skills::VULNERABILITY => 25,
        ids::skills::FURY => 9,
        _ => panic!("Unknown duration buff")
    }
}

pub fn get_stacking_type(skill_id: u32) -> BuffStackingType {
    match skill_id {
        ids::skills::MIGHT => BuffStackingType::Intensity,
        ids::skills::KALLAS_FERVOR => BuffStackingType::Intensity,
        ids::skills::VULNERABILITY => BuffStackingType::Intensity,
        ids::skills::FURY => BuffStackingType::Duration,
        _ => panic!("Unknown buff")
    }
}

pub struct SkillData {
    power_multipliers: HashMap<u32, f64>
}

impl SkillData {
    pub fn from_file(filename: &str) -> io::Result<Self> {
        let file = File::open(filename)?;
        let mut multipliers = HashMap::new();
        for line in io::BufReader::new(file).lines() {
            let line = line?;
            let mut split = line.split_whitespace();
            let skill_id = split.next().expect("Could not read skill ID");
            let multiplier = split.next().expect("Could not read skill ID");
            multipliers.insert(skill_id.parse().expect("Could not parse skill ID"), multiplier.parse().expect("Could not parse skill power multiplier"));
        }
        Ok(SkillData {power_multipliers: multipliers})
    }

    pub fn power_multiplier(&self, skill_id: u32) -> Option<f64> {
        self.power_multipliers.get(&skill_id).copied()
    }
}