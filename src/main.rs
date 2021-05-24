use itertools::Itertools;
use crate::SimulationEvent::{SelfBuffApplication, TargetConditionApplication, WeaponSwap, TargetBuffApplication, PhysicalHit, ConditionTick, LifeStealHit};
use std::collections::HashMap;
use crate::gamedata::{SkillData, SkillType, get_skill_type, get_stack_limit, BuffStackingType};
use crate::evtc::EvtcSkill;

mod evtc;
mod ids;
mod gamedata;
mod stats;
mod extract;

pub enum Stat {
    Power,
    Precision,
    Ferocity,
    ConditionDamage,
    Expertise,
}

#[derive(Eq, PartialEq, Hash, Copy, Clone, Debug)]
pub enum DamagingCondition {
    Bleeding,
    Burning,
    Confusion,
    Poisoned,
    Torment,
}

impl DamagingCondition {
    pub fn from_id(skill_id: u32) -> Self {
        match skill_id {
            ids::skills::BLEEDING => DamagingCondition::Bleeding,
            ids::skills::BURNING => DamagingCondition::Burning,
            ids::skills::CONFUSION => DamagingCondition::Confusion,
            ids::skills::POISONED => DamagingCondition::Poisoned,
            ids::skills::TORMENT => DamagingCondition::Torment,
            _ => panic!("Unknown condition id!")
        }
    }
    pub fn to_id(&self) -> u32 {
        match self {
            DamagingCondition::Bleeding => ids::skills::BLEEDING,
            DamagingCondition::Burning => ids::skills::BURNING,
            DamagingCondition::Confusion => ids::skills::CONFUSION,
            DamagingCondition::Poisoned => ids::skills::POISONED,
            DamagingCondition::Torment => ids::skills::TORMENT,
        }
    }
}

pub struct StatAmount {
    stat: Stat,
    amount: u32,
}

pub enum PhysicalHitSource {
    Unknown,
    Skill(u32),
}

pub enum Trait {
    AbyssalChill,
}

pub enum Food {
    GhostPepperPopper,
}

pub enum ConditionApplicationSource {
    Unknown,
    Skill(u32),
    Sigil(Sigil),
    Trait(Trait),
    Food(Food),
}

pub enum LifeStealSource {
    Unknown,
    Buff(u32),
}

pub enum BuffTarget {
    Player,
    Target,
}

pub enum WeaponType {
    DualWield,
    TwoHanded,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum WeaponSet {
    Land1,
    Land2,
}

pub enum SimulationEvent {
    /// Player hits the target with a physical attack.
    PhysicalHit { time: i64, base_damage: u32, coefficient: f64, enemy_armor: u32, source: PhysicalHitSource, critical: bool },
    /// Player applies buff to self.
    SelfBuffApplication { time: i64, skill_id: u32, base_duration: u32 },
    /// Player applies buff to target.
    TargetBuffApplication { time: i64, skill_id: u32, base_duration: u32 },
    /// Player applies a damaging condition to target.
    TargetConditionApplication { time: i64, condition: DamagingCondition, base_duration: u32, source: ConditionApplicationSource },
    /// Condition ticks for damage.
    ConditionTick { time: i64, target_moving: bool },
    /// Player damages the enemy with life steal. TODO: Source, if it even can be detected
    LifeStealHit { time: i64, base_damage: f64, power_scaling: f64, source: LifeStealSource },
    /// Player swaps weapons to another weapon set.
    WeaponSwap { time: i64, weapon_set: WeaponSet },
}

pub trait BuffUptimes {
    fn add_stack(&mut self, skill_id: u32, duration: i64, time: i64);
    fn remove_stack(&mut self, skill_id: u32, time: i64);
    fn remove_last_stack(&mut self, skill_id: u32, time: i64);
    fn is_applied(&mut self, skill_id: u32, time: i64) -> bool;
    fn get_stack_count(&mut self, skill_id: u32, time: i64) -> u32;
}

struct SimBuffStack {
    duration: i64,
}

enum SimBuffState {
    Duration {
        last_update_time: i64,
        active: Option<SimBuffStack>,
        queued_stacks: Vec<SimBuffStack>,
        stack_limit: usize,
    },
    Intensity {
        last_update_time: i64,
        stacks: Vec<SimBuffStack>,
        stack_limit: usize,
    },
}

struct SimBuffUptimes {
    states: HashMap<u32, SimBuffState>
}

impl SimBuffUptimes {
    fn update_state(&mut self, skill_id: u32, time: i64) {
        if !self.states.contains_key(&skill_id) {
            self.states.insert(skill_id, match gamedata::get_stacking_type(skill_id) {
                BuffStackingType::Duration => SimBuffState::Duration {
                    last_update_time: time,
                    active: None,
                    queued_stacks: Vec::new(),
                    stack_limit: get_stack_limit(skill_id) as usize,
                },
                BuffStackingType::Intensity => SimBuffState::Intensity {
                    last_update_time: time,
                    stacks: Vec::new(),
                    stack_limit: get_stack_limit(skill_id) as usize,
                }
            });
        }

        let state = self.states.get_mut(&skill_id).unwrap();
        if let SimBuffState::Duration { last_update_time, active, queued_stacks, .. } = state {
            let mut elapsed_time = time - *last_update_time;
            assert!(elapsed_time >= 0);
            while active.is_some() {
                if elapsed_time <= 0 {
                    break;
                }

                let new_active_duration = (active.as_ref().unwrap().duration - elapsed_time).max(0);
                let time_diff = active.as_ref().unwrap().duration - new_active_duration;
                let fully_expired = new_active_duration == 0;
                if fully_expired {
                    // TODO: Assuming longest stack is used, but it might just be queued
                    //       and replaced in there - investigate logs
                    if let Some(next_stack_index) = queued_stacks.iter().position_max_by_key(|x| x.duration) {
                        let stack = queued_stacks.swap_remove(next_stack_index);
                        *active = Some(stack);
                    }
                } else {
                    if let Some(stack) = active {
                        stack.duration = new_active_duration;
                    }
                }

                elapsed_time -= time_diff;
            }
            *last_update_time = time;
        } else if let SimBuffState::Intensity { last_update_time, stacks, .. } = state {
            let elapsed_time = time - *last_update_time;
            assert!(elapsed_time >= 0);
            for mut stack in &mut *stacks {
                let new_duration = (stack.duration - elapsed_time).max(0);
                stack.duration = new_duration;
            }
            stacks.retain(|x| x.duration > 0);
            *last_update_time = time;
        }
    }

    fn current_stack_count(&self, skill_id: u32) -> u32 {
        let state = self.states.get(&skill_id);
        if let Some(&SimBuffState::Duration { active, .. }) = state.as_ref() {
            if active.is_some() { 1 } else { 0 }
        } else if let Some(SimBuffState::Intensity { stacks, .. }) = state.as_ref() {
            stacks.len() as u32
        } else {
            unreachable!()
        }
    }

    fn insert_stack(&mut self, skill_id: u32, duration: i64) {
        let state = self.states.get_mut(&skill_id).unwrap();
        if let SimBuffState::Duration { queued_stacks, stack_limit, active, .. } = state {
            if active.is_none() {
                *active = Some(SimBuffStack { duration })
            } else {
                let free_spots = *stack_limit - (queued_stacks.len() + 1); // + 1 for the active stack
                if free_spots > 0 {
                    queued_stacks.push(SimBuffStack { duration });
                } else {
                    // TODO: Verify this is correct behavior
                    // Evict shortest stack
                    if let Some(shortest_stack_index) = queued_stacks.iter().position_min_by_key(|x| x.duration) {
                        if duration > queued_stacks[shortest_stack_index].duration {
                            queued_stacks.swap_remove(shortest_stack_index);
                            queued_stacks.push(SimBuffStack { duration });
                        }
                    }
                }
            }
        } else if let SimBuffState::Intensity { stacks, stack_limit, .. } = state {
            let free_spots = *stack_limit - stacks.len();
            if free_spots > 0 {
                stacks.push(SimBuffStack { duration });
            } else {
                // TODO: Verify this is correct behavior
                // Evict shortest stack
                if let Some(shortest_stack_index) = stacks.iter().position_min_by_key(|x| x.duration) {
                    if duration > stacks[shortest_stack_index].duration {
                        stacks.swap_remove(shortest_stack_index);
                        stacks.push(SimBuffStack { duration });
                    }
                }
            }
        }
    }
}

impl BuffUptimes for SimBuffUptimes {
    fn add_stack(&mut self, skill_id: u32, duration: i64, time: i64) {
        self.update_state(skill_id, time);
        self.insert_stack(skill_id, duration);
    }

    fn remove_stack(&mut self, skill_id: u32, time: i64) {
        self.update_state(skill_id, time);
        unimplemented!()
    }

    fn remove_last_stack(&mut self, skill_id: u32, time: i64) {
        self.update_state(skill_id, time);
        unimplemented!();
    }

    fn is_applied(&mut self, skill_id: u32, time: i64) -> bool {
        self.update_state(skill_id, time);
        self.current_stack_count(skill_id) > 0
    }

    fn get_stack_count(&mut self, skill_id: u32, time: i64) -> u32 {
        self.update_state(skill_id, time);
        self.current_stack_count(skill_id)
    }
}

pub struct LogBuffUptimes {
    stack_counts: HashMap<u32, u32>
}

impl BuffUptimes for LogBuffUptimes {
    fn add_stack(&mut self, skill_id: u32, _duration: i64, _time: i64) {
        *self.stack_counts.entry(skill_id).or_insert(0) += 1;
    }

    fn remove_stack(&mut self, skill_id: u32, _time: i64) {
        *self.stack_counts.entry(skill_id).or_insert(0) -= 1;
    }

    fn remove_last_stack(&mut self, skill_id: u32, _time: i64) {
        *self.stack_counts.entry(skill_id).or_insert(0) = 0;
    }

    fn is_applied(&mut self, skill_id: u32, _time: i64) -> bool {
        *self.stack_counts.get(&skill_id).unwrap_or(&0) > 0
    }

    fn get_stack_count(&mut self, skill_id: u32, _time: i64) -> u32 {
        *self.stack_counts.get(&skill_id).unwrap_or(&0)
    }
}

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum Sigil {
    None,
    Geomancy,
    Earth,
    Doom,
    Demons,
    Malice,
    Bursting,
}

pub struct PlayerStats<TUptimes: BuffUptimes> {
    power: u32,
    precision: u32,
    ferocity: u32,
    condition_damage: u32,
    expertise: u32,
    concentration: u32,
    set_1_sigils: [Sigil; 2],
    set_2_sigils: [Sigil; 2],
    extra_condition_durations_all: f64,
    extra_condition_durations: HashMap<u32, f64>,
    extra_condition_durations_under_buff: HashMap<u32, (u32, f64)>,
    extra_condition_damages: HashMap<DamagingCondition, f64>,
    weapon_set: WeaponSet,
    weapon_set_types: [WeaponType; 2],
    buff_uptimes: TUptimes,
}

impl<TUptimes: BuffUptimes> PlayerStats<TUptimes> {
    fn power(&mut self, time: i64) -> u32 {
        let might = self.buff_uptimes.get_stack_count(ids::skills::MIGHT, time);
        assert!(might <= gamedata::get_stack_limit(ids::skills::MIGHT));
        self.power + might * 30
    }
    fn precision(&mut self, time: i64) -> u32 {
        unimplemented!("Beware, touching precision makes crits from original unrealistic");
        //self.precision
    }
    fn ferocity(&mut self, time: i64) -> u32 {
        let kallas_fervor = self.buff_uptimes.get_stack_count(ids::skills::KALLAS_FERVOR, time);
        assert!(kallas_fervor <= gamedata::get_stack_limit(ids::skills::KALLAS_FERVOR));
        self.ferocity + kallas_fervor * 30
    }
    fn condition_damage(&mut self, time: i64) -> u32 {
        let might = self.buff_uptimes.get_stack_count(ids::skills::MIGHT, time);
        assert!(might <= gamedata::get_stack_limit(ids::skills::MIGHT));
        self.condition_damage + might * 30
    }
    fn condition_duration(&mut self, skill_id: u32, time: i64) -> f64 {
        assert_eq!(get_skill_type(skill_id), SkillType::Condition);
        let mut duration = 1.
            + self.expertise as f64 / 1500.
            + self.extra_condition_durations_all
            + *self.extra_condition_durations.get(&skill_id).unwrap_or(&0.);

        if let Some((buff, extra_duration)) = self.extra_condition_durations_under_buff.get(&skill_id) {
            if self.buff_uptimes.is_applied(*buff, time) {
                duration += extra_duration;
            }
        }

        if skill_id == ids::skills::TORMENT && self.current_sigils().contains(&Sigil::Demons) {
            duration += 0.2;
        }

        if self.current_sigils().contains(&Sigil::Malice) {
            duration += 0.1;
        }

        duration.min(2.)
    }
    fn boon_duration(&mut self, time: i64) -> f64 {
        (1. + self.concentration as f64 / 1500.).min(2.)
    }
    fn condition_damage_mult(&mut self, condition: DamagingCondition, time: i64) -> f64 {
        // These multipliers seem to be multiplicative
        let mut multiplier = 1.;
        let kallas_fervor = self.buff_uptimes.get_stack_count(ids::skills::KALLAS_FERVOR, time);
        assert!(kallas_fervor <= gamedata::get_stack_limit(ids::skills::KALLAS_FERVOR));
        multiplier *= 1. + kallas_fervor as f64 * stats::KALLAS_FERVOR_CONDITION_DAMAGE_MULTIPLIER;

        if let Some(extra_damage) = self.extra_condition_damages.get(&condition) {
            multiplier *= 1. + extra_damage;
        }

        if self.current_sigils().contains(&Sigil::Bursting) {
            multiplier *= 1.05;
        }

        multiplier
    }
    fn power_damage_mult<TTargetBuffs: BuffUptimes>(&mut self, time: i64, target_buffs: &mut TTargetBuffs, target_health: f64) -> f64 {
        // TODO: We assume all these multipliers are additive, but it's not tested.

        let mut multiplier = 1.0;
        // TODO: Put behind trait flag
        // Destructive Impulses
        let weapon_type = match self.weapon_set {
            WeaponSet::Land1 => &self.weapon_set_types[0],
            WeaponSet::Land2 => &self.weapon_set_types[1],
        };
        multiplier += match weapon_type {
            WeaponType::DualWield => 0.1,
            WeaponType::TwoHanded => 0.05,
        };

        // TODO: Put behind trait flag
        // Targeted Destruction
        let target_vuln = target_buffs.get_stack_count(ids::skills::VULNERABILITY, time);
        assert!(target_vuln <= gamedata::get_stack_limit(ids::skills::VULNERABILITY));
        multiplier += target_vuln as f64 * 0.005;

        // TODO: Put behind trait flag
        // Unsuspecting Strikes
        if target_health >= 0.8 {
            multiplier += 0.25;
        }

        multiplier
    }

    fn current_sigils(&self) -> &[Sigil; 2] {
        match self.weapon_set {
            WeaponSet::Land1 => &self.set_1_sigils,
            WeaponSet::Land2 => &self.set_2_sigils,
        }
    }
}

struct TargetConditions {
    stacks: HashMap<DamagingCondition, Vec<ConditionStack>>
}

struct ConditionStack {
    duration: i64,
    last_update: i64,
}

impl TargetConditions {
    fn new() -> Self {
        let mut stacks = HashMap::new();
        let conditions = [DamagingCondition::Torment, DamagingCondition::Confusion, DamagingCondition::Bleeding, DamagingCondition::Burning, DamagingCondition::Poisoned];
        for condition in std::array::IntoIter::new(conditions) {
            stacks.insert(condition, Vec::new());
        }

        TargetConditions { stacks }
    }
    fn add_condition(&mut self, condition: DamagingCondition, duration: i64, time: i64) {
        let stacks = self.stacks.get_mut(&condition).unwrap();
        stacks.push(ConditionStack { duration, last_update: time })
    }
}

struct DamageDistribution {
    damage_by_skill: HashMap<u32, u64>,
    total_damage: u64,
}

impl DamageDistribution {
    pub fn new() -> Self {
        DamageDistribution { damage_by_skill: HashMap::new(), total_damage: 0 }
    }
    pub fn add_damage(&mut self, skill_id: u32, damage: u64) {
        *self.damage_by_skill.entry(skill_id).or_insert(0) += damage;
        self.total_damage += damage;
    }
    pub fn total_damage(&self) -> u64 {
        self.total_damage
    }
}

fn main() {
    // step 0: load gamedata
    let gamedata = SkillData::from_file("gamedata/damage-multipliers-2021.03.11").expect("Failed to read skill data");

    // step 1: open arcdps file (unzip if needed)
    //let log_bytes = std::fs::read("logs/20210322-195505.evtc").expect("Failed to read log file");
    let log_bytes = std::fs::read("logs/20210408-013544.evtc").expect("Failed to read log file");

    // step 2: parse structs
    let evtc_log = evtc::parsing::evtc_parser(&log_bytes).expect("Failed to parse log").1;

    // step 3: build setup
    const CHARACTER_NAME: &str = "Name The Unnameable";
    let player = evtc_log.agents.iter()
        .filter(|x| x.is_player() && x.name.split('\0').next().unwrap() == CHARACTER_NAME)
        .next()
        .expect("Player not found");
    let target = evtc_log.agents.iter()
        .filter(|x| !x.is_player() && x.profession == evtc_log.boss_species_id as u32)
        .next()
        .expect("Target not found");
    println!("Found player: {}", player.name.replace('\0', " | "));
    println!("Found target: {}", target.name);

    // Beware, land 1 and land 2 sets need to be correctly identified from the log manually
    // 1 = Shortbow
    // 2 = Mace/axe
    let stats = PlayerStats {
        power: 2173,
        precision: 1633,
        ferocity: 0,
        condition_damage: 1672,
        expertise: 633,
        weapon_set: WeaponSet::Land1, // Started on shortbow
        weapon_set_types: [WeaponType::TwoHanded, WeaponType::DualWield],
        set_1_sigils: [Sigil::Earth, Sigil::Geomancy],
        set_2_sigils: [Sigil::Earth, Sigil::Doom],
        extra_condition_durations_all: 0.2,
        extra_condition_durations: std::array::IntoIter::new([
            (ids::skills::BLEEDING, 0.1),
            (ids::skills::BURNING, 0.1),
            (ids::skills::POISONED, 0.1),
            (ids::skills::CONFUSION, 0.1),
            (ids::skills::TORMENT, 0.1),
        ]).collect(),
        extra_condition_durations_under_buff: std::array::IntoIter::new([
            (ids::skills::BLEEDING, (ids::skills::FURY, 0.25)),
        ]).collect(),
        extra_condition_damages: std::array::IntoIter::new([
            (DamagingCondition::Torment, 0.1),
            (DamagingCondition::Bleeding, 0.25),
        ]).collect(),
        concentration: 0,
        buff_uptimes: LogBuffUptimes { stack_counts: Default::default() },
    };

    // step 4: analyze hits, build base damage and stuff, build a resimable representation

    let simulation_events = extract::get_events(&evtc_log, &player, &target, stats, &gamedata);

    // step 5: resim
    eprintln!("WARNING: Make sure precision is the same, crits are taken from original log!");
    eprintln!("WARNING: Make sure enemy health is correct, it's hardcoded!");
    eprintln!("WARNING: Make sure Geomancy and Doom are only on ONE weaponset in the original log!");
    // TODO: Implement enemy max health reading, it's in the log
    let enemy_max_health = 11698890;

    // TODO: Viper, Sinister, (Grieving?)
    for &chestplate_sinister in &[false, true] {
        for expertise_infusions in 0..=18 {
            let condition_damage_infusions = 18 - expertise_infusions;

            for runes in &[Runes::Nightmare, Runes::Tormenting, Runes::Tempest, Runes::TrapperWithBlackDiamond, Runes::TrapperWith25CondiDamage] {
                for replacement_sigil11 in &[None, Some(Sigil::Bursting), Some(Sigil::Demons), Some(Sigil::Malice)] {
                    for replacement_sigil12 in &[None, Some(Sigil::Bursting), Some(Sigil::Demons), Some(Sigil::Malice)] {
                        if replacement_sigil11.is_some() && replacement_sigil11 == replacement_sigil12 {
                            continue;
                        }
                        for replacement_sigil21 in &[None, Some(Sigil::Bursting), Some(Sigil::Demons), Some(Sigil::Malice)] {
                            for replacement_sigil22 in &[None, Some(Sigil::Bursting), Some(Sigil::Demons), Some(Sigil::Malice)] {
                                if replacement_sigil21.is_some() && replacement_sigil21 == replacement_sigil22 {
                                    continue;
                                }
                                // TODO: Avoid trying both swaps (make an order and start on higher index)
                                let mut new_stats = PlayerStats {
                                    power: 2173,
                                    precision: 1633,
                                    ferocity: 0,
                                    condition_damage: 1672,
                                    expertise: 633,
                                    weapon_set: WeaponSet::Land1, // Started on shortbow
                                    weapon_set_types: [WeaponType::TwoHanded, WeaponType::DualWield],
                                    set_1_sigils: [Sigil::Earth, Sigil::Geomancy],
                                    set_2_sigils: [Sigil::Earth, Sigil::Doom],
                                    extra_condition_durations_all: 0.2,
                                    extra_condition_durations: std::array::IntoIter::new([
                                        (ids::skills::BLEEDING, 0.1),
                                        (ids::skills::BURNING, 0.1),
                                        (ids::skills::POISONED, 0.1),
                                        (ids::skills::CONFUSION, 0.1),
                                        (ids::skills::TORMENT, 0.1),
                                    ]).collect(),
                                    extra_condition_durations_under_buff: std::array::IntoIter::new([
                                        (ids::skills::BLEEDING, (ids::skills::FURY, 0.25)),
                                    ]).collect(),
                                    extra_condition_damages: std::array::IntoIter::new([
                                        (DamagingCondition::Torment, 0.1),
                                        (DamagingCondition::Bleeding, 0.25),
                                    ]).collect(),
                                    concentration: 0,
                                    buff_uptimes: SimBuffUptimes { states: Default::default() },
                                };

                                // Infusions
                                new_stats.condition_damage -= 18 * 5; // Remove old infusions
                                new_stats.condition_damage += condition_damage_infusions * 5;
                                new_stats.expertise += expertise_infusions * 5;

                                // Gear
                                if chestplate_sinister {
                                    new_stats.condition_damage -= 121;
                                    new_stats.expertise -= 67;
                                    new_stats.power -= 67;
                                    new_stats.precision -= 67;
                                    new_stats.condition_damage += 141;
                                    new_stats.power += 101;
                                    new_stats.precision += 101;
                                    // WARNING: Precision change
                                }

                                // TODO: Gear Stats
                                // TODO: Utility enhancement choices

                                // Runes
                                match runes {
                                    Runes::Nightmare => {
                                        // already applied
                                        // 175 cdamage, 20% duration
                                    }
                                    Runes::Tormenting => {
                                        // 175 cdamage, 50% torment duration
                                        new_stats.extra_condition_durations_all = 0.;
                                        *new_stats.extra_condition_durations.get_mut(&ids::skills::TORMENT).unwrap() += 0.5;
                                    }
                                    Runes::Tempest => {
                                        // 36 all stats
                                        // 25% condition duration
                                        new_stats.condition_damage -= 175;
                                        new_stats.extra_condition_durations_all = 0.25;
                                        new_stats.power += 36;
                                        new_stats.precision += 36;
                                        new_stats.ferocity += 36;
                                        new_stats.condition_damage += 36;
                                        new_stats.expertise += 36;
                                        new_stats.concentration += 36;

                                        // WARNING: Changes precision!!
                                    }
                                    Runes::TrapperWithBlackDiamond => {
                                        // 175 cdamage, 15% duration
                                        new_stats.extra_condition_durations_all = 0.15;
                                        // 17 cdamage, 17 power, 9 expertise, 9 precision
                                        new_stats.condition_damage += 17;
                                        new_stats.power += 17;
                                        new_stats.expertise += 9;
                                        new_stats.precision += 9;

                                        // WARNING: Changes precision!!
                                    }
                                    Runes::TrapperWith25CondiDamage => {
                                        // 175 cdamage, 15% duration
                                        new_stats.extra_condition_durations_all = 0.15;
                                        // 25 cdamage (any such rune)
                                        new_stats.condition_damage += 25;
                                    }
                                }

                                // WARNING: Does not respect weapon set for geomancy and doom!
                                let mut remove_geomancy = false;
                                let mut remove_doom = false;
                                let mut remove_earth_1 = false;
                                let mut remove_earth_2 = false;
                                // Sigils
                                if let Some(sigil) = replacement_sigil11 {
                                    if new_stats.set_1_sigils[0] == Sigil::Doom {
                                        remove_doom = true;
                                    }
                                    if new_stats.set_1_sigils[0] == Sigil::Geomancy {
                                        remove_geomancy = true;
                                    }
                                    if new_stats.set_1_sigils[0] == Sigil::Earth {
                                        remove_earth_1 = true;
                                    }
                                    new_stats.set_1_sigils[0] = *sigil;
                                }
                                if let Some(sigil) = replacement_sigil12 {
                                    if new_stats.set_1_sigils[1] == Sigil::Doom {
                                        remove_doom = true;
                                    }
                                    if new_stats.set_1_sigils[1] == Sigil::Geomancy {
                                        remove_geomancy = true;
                                    }
                                    if new_stats.set_1_sigils[1] == Sigil::Earth {
                                        remove_earth_1 = true;
                                    }
                                    new_stats.set_1_sigils[1] = *sigil;
                                }
                                if let Some(sigil) = replacement_sigil21 {
                                    if new_stats.set_2_sigils[0] == Sigil::Doom {
                                        remove_doom = true;
                                    }
                                    if new_stats.set_2_sigils[0] == Sigil::Geomancy {
                                        remove_geomancy = true;
                                    }
                                    if new_stats.set_2_sigils[0] == Sigil::Earth {
                                        remove_earth_2 = true;
                                    }
                                    new_stats.set_2_sigils[0] = *sigil;
                                }
                                if let Some(sigil) = replacement_sigil22 {
                                    if new_stats.set_2_sigils[1] == Sigil::Doom {
                                        remove_doom = true;
                                    }
                                    if new_stats.set_2_sigils[1] == Sigil::Geomancy {
                                        remove_geomancy = true;
                                    }
                                    if new_stats.set_2_sigils[1] == Sigil::Earth {
                                        remove_earth_2 = true;
                                    }
                                    new_stats.set_2_sigils[1] = *sigil;
                                }


                                print!("Inf: E{} C{} | {:?} | [{:?};{:?}] [{:?};{:?}] | Chest {} |",
                                       expertise_infusions, condition_damage_infusions,
                                       runes,
                                       new_stats.set_1_sigils[0], new_stats.set_1_sigils[1],
                                       new_stats.set_2_sigils[0], new_stats.set_2_sigils[1],
                                       if chestplate_sinister { "S" } else { "V" }
                                );
                                let result = sim(new_stats, &simulation_events, &evtc_log.skills, enemy_max_health, remove_doom, remove_geomancy, remove_earth_1, remove_earth_2);
                                println!("{}", result.total_damage())
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
enum Runes {
    Nightmare,
    Tormenting,
    Tempest,
    TrapperWithBlackDiamond,
    TrapperWith25CondiDamage,
}

fn sim(mut stats: PlayerStats<SimBuffUptimes>,
       events: &[SimulationEvent],
       skills: &[EvtcSkill],
       enemy_max_health: u64,
       remove_doom: bool,
       remove_geomancy: bool,
       remove_earth_1: bool,
       remove_earth_2: bool,
) -> DamageDistribution {
    let mut target_uptimes = SimBuffUptimes { states: Default::default() };
    let mut target_conditions = TargetConditions::new();

    fn get_duration(stats: &mut PlayerStats<SimBuffUptimes>, skill_id: u32, base_duration: u32, time: i64) -> i64 {
        let duration = match gamedata::get_skill_type(skill_id) {
            SkillType::Unknown => unreachable!("Unknown buff tracked"),
            SkillType::Ability => unreachable!("Ability tracked as buff"),
            SkillType::Condition => (base_duration as f64 * stats.condition_duration(skill_id, time)) as u32,
            SkillType::Boon => (base_duration as f64 * stats.boon_duration(time)) as u32,
            SkillType::GenericBuff => base_duration,
        };

        duration as i64
    }

    let mut damage_distribution = DamageDistribution::new();
    for event in events {
        match event {
            PhysicalHit { time, base_damage, coefficient, source, critical, enemy_armor } => {
                if let PhysicalHitSource::Skill(ids::skills::RING_OF_EARTH) = source {
                    if remove_geomancy {
                        continue;
                    }
                }

                let mut damage = *base_damage as f64 * stats.power(*time) as f64 * *coefficient / *enemy_armor as f64;
                if *critical {
                    damage *= 1.5 + stats.ferocity(*time) as f64 / 1500.;
                }
                let vuln_multiplier = 1. + 0.01 * target_uptimes.get_stack_count(ids::skills::VULNERABILITY, *time) as f64;
                let enemy_health = (enemy_max_health as f64 - damage_distribution.total_damage() as f64) / enemy_max_health as f64;
                damage *= vuln_multiplier;
                damage *= stats.power_damage_mult(*time, &mut target_uptimes, enemy_health);
                if let PhysicalHitSource::Skill(skill_id) = source {
                    damage_distribution.add_damage(*skill_id, damage.round() as u64);
                    //println!("[{}] physical hit {}->{} (crit {}, pwr {}, ferocity {}, might {}, vuln {}) | skill {}",
                    //         time,
                    //         base_damage,
                    //         damage,
                    //         critical,
                    //         stats.power(*time),
                    //         stats.ferocity(*time),
                    //         stats.buff_uptimes.get_stack_count(ids::skills::MIGHT, *time),
                    //         vuln_multiplier,
                    //         skill_id
                    //);
                } else {
                    panic!("Unknown skill for physical damage;")
                }
            }
            SelfBuffApplication { time, skill_id, base_duration } => {
                let duration = get_duration(&mut stats, *skill_id, *base_duration, *time);
                stats.buff_uptimes.add_stack(*skill_id, duration, *time);
                //println!("[{}] self buff {}->{}", time, base_duration, duration);
            }
            TargetBuffApplication { time, skill_id, base_duration } => {
                let duration = get_duration(&mut stats, *skill_id, *base_duration, *time);
                target_uptimes.add_stack(*skill_id, duration, *time);
                //println!("[{}] target buff {}->{}", time, base_duration, duration);
            }
            TargetConditionApplication { time, condition, base_duration, source } => {
                if let ConditionApplicationSource::Sigil(Sigil::Doom) = source {
                    if remove_doom {
                        continue;
                    }
                }
                if let ConditionApplicationSource::Sigil(Sigil::Earth) = source {
                    if remove_earth_1 && stats.weapon_set == WeaponSet::Land1 {
                        continue;
                    }
                    if remove_earth_2 && stats.weapon_set == WeaponSet::Land2 {
                        continue;
                    }
                }
                if let ConditionApplicationSource::Skill(ids::skills::RING_OF_EARTH) = source {
                    if remove_geomancy {
                        continue;
                    }
                }

                let duration = get_duration(&mut stats, condition.to_id(), *base_duration, *time);
                target_conditions.add_condition(*condition, duration, *time);
                //println!("[{}] target condi application, duration {}->{}, condi {:?}", time, base_duration, duration, condition)
            }
            ConditionTick { time, target_moving } => {
                for (condition, stacks) in target_conditions.stacks.iter_mut() {
                    let base_damage = match condition {
                        DamagingCondition::Bleeding => gamedata::BLEEDING_BASE_DAMAGE,
                        DamagingCondition::Burning => gamedata::BURNING_BASE_DAMAGE,
                        DamagingCondition::Confusion => gamedata::CONFUSION_BASE_DAMAGE,
                        DamagingCondition::Poisoned => gamedata::POISONED_BASE_DAMAGE,
                        DamagingCondition::Torment if *target_moving => gamedata::TORMENT_MOVING_BASE_DAMAGE,
                        DamagingCondition::Torment => gamedata::TORMENT_BASE_DAMAGE
                    };

                    let multiplier = match condition {
                        DamagingCondition::Bleeding => gamedata::BLEEDING_MULTIPLIER,
                        DamagingCondition::Burning => gamedata::BURNING_MULTIPLIER,
                        DamagingCondition::Confusion => gamedata::CONFUSION_MULTIPLIER,
                        DamagingCondition::Poisoned => gamedata::POISONED_MULTIPLIER,
                        DamagingCondition::Torment if *target_moving => gamedata::TORMENT_MOVING_MULTIPLIER,
                        DamagingCondition::Torment => gamedata::TORMENT_MULTIPLIER
                    };

                    let mut damage = base_damage + stats.condition_damage(*time) as f64 * multiplier;
                    damage *= stats.condition_damage_mult(*condition, *time);
                    let vuln_multiplier = 1. + target_uptimes.get_stack_count(ids::skills::VULNERABILITY, *time) as f64 * 0.01;
                    damage *= vuln_multiplier;
                    //println!("{:?}: ({} + {} * {}) * {} * {} = {}", condition, base_damage, stats.condition_damage(*time) as f64, multiplier, stats.condition_damage_mult(*condition, *time), vuln_multiplier, damage);

                    for stack in stacks.iter_mut() {
                        let elapsed = 1000;
                        let new_remaining_duration = (stack.duration - elapsed).max(0);
                        if new_remaining_duration == 0 {
                            // Partial damage

                            // Round to nearest 1000 / 25 (tick rate)
                            // https://discord.com/channels/456611641526845473/569588485951062017/737481152482246657
                            let adjusted_remaining_duration = stack.duration as f64 + (stack.duration as f64 % (1000. / 25.));
                            let ratio = adjusted_remaining_duration / 1000.;
                            damage_distribution.add_damage(condition.to_id(), (damage * ratio).round() as u64);
                        } else {
                            // Full damage
                            damage_distribution.add_damage(condition.to_id(), damage.round() as u64);
                        }
                        stack.duration = new_remaining_duration;
                        stack.last_update = *time;
                    }

                    stacks.retain(|x| x.duration > 0);
                }
                //println!("[{}] condi tick!", time);
                //println!("      BLEED {} BURN {} TORMENT {} POISON {} CONFUSION {}",
                //         target_conditions.stacks.get(&DamagingCondition::Bleeding).unwrap().len(),
                //         target_conditions.stacks.get(&DamagingCondition::Burning).unwrap().len(),
                //         target_conditions.stacks.get(&DamagingCondition::Torment).unwrap().len(),
                //         target_conditions.stacks.get(&DamagingCondition::Poisoned).unwrap().len(),
                //         target_conditions.stacks.get(&DamagingCondition::Confusion).unwrap().len(),
                //);
            }
            LifeStealHit { time, base_damage, power_scaling, source } => {
                let damage = *base_damage as f64 + stats.power(*time) as f64 * power_scaling;
                if let LifeStealSource::Buff(buff_id) = source {
                    damage_distribution.add_damage(*buff_id, damage.round() as u64);
                    // May be a bit off if might share happens at the same time, the order
                    // is not perfect in that case (life steal from battle scars seems to happen after)
                    //println!("[{}] life steal {}->{} (scaling {}, pwr {})", time, base_damage, damage, power_scaling, stats.power(*time));
                } else {
                    panic!("Unknown source of life steal!")
                }
            }
            WeaponSwap { time, weapon_set } => {
                stats.weapon_set = *weapon_set;
                //println!("[{}] weaponswap to {:?}", time, weapon_set);
            }
        }
    }

    //println!("total damage {}", damage_distribution.total_damage());
    //for (id, damage) in damage_distribution.damage_by_skill.iter().sorted_by_key(|(id, damage)| *damage).rev() {
    //    let name = skills.iter().filter(|x| x.id == *id as i32).next().map(|x| x.name.clone()).unwrap_or(String::from("???"));
    //    println!("{};{};{}", name, id, damage)
    //}
    damage_distribution
}
