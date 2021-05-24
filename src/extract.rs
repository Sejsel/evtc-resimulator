use crate::{gamedata, SimulationEvent, LogBuffUptimes, PlayerStats, Sigil, BuffUptimes, DamagingCondition, ids, LifeStealSource, PhysicalHitSource, ConditionApplicationSource, WeaponSet};
use crate::evtc::{EvtcCombatItem, EvtcLog, EvtcAgent};
use crate::gamedata::{SkillType, SkillData};
use std::ops::Range;
use crate::SimulationEvent::{ConditionTick, PhysicalHit, SelfBuffApplication, TargetConditionApplication, TargetBuffApplication};
use itertools::Itertools;

const TRACKED_DAMAGING_CONDITION_IDS: [u32; 5] = [ids::skills::BLEEDING, ids::skills::BURNING, ids::skills::CONFUSION, ids::skills::POISONED, ids::skills::TORMENT];
const TRACKED_PLAYER_BUFF_IDS: [u32; 3] = [ids::skills::FURY, ids::skills::MIGHT, ids::skills::KALLAS_FERVOR];
const TRACKED_TARGET_BUFF_IDS: [u32; 1] = [ids::skills::VULNERABILITY];


pub fn get_events(evtc_log: &EvtcLog, player: &EvtcAgent, target: &EvtcAgent, mut stats: PlayerStats<LogBuffUptimes>, gamedata: &SkillData) -> Vec<SimulationEvent> {
    let mut last_condition_tick = 0;
    let mut target_health = 1.;

    // This is only used to reverse values when building the representation
    let mut target_buffs = LogBuffUptimes { stack_counts: Default::default() };
    let mut simulation_events = Vec::new();
    let mut player_inst_id = evtc_log.combat_items
        .iter()
        .filter(|x| x.is_state_change == 0 && x.src_agent == player.address)
        .next()
        .expect("Found no event with player as src_agent")
        .src_inst_id;

    // There's no implementation for reversing the other sigils.
    // Notably, anything that may cause overstacks (vuln) will be resimmed wrong because overstacked applications will be missing.
    assert!(stats.set_1_sigils.iter().all(|x| *x == Sigil::Earth || *x == Sigil::Doom || *x == Sigil::Geomancy));

    fn get_same_time_events(events: &Vec<&EvtcCombatItem>, i: usize, delta: i64) -> Range<usize> {
        let time = events[i].time;
        let mut min_i = i;
        let mut max_i = i;
        while min_i > 0 {
            if events[min_i - 1].time >= time - delta {
                min_i -= 1;
            } else {
                break;
            }
        }
        while max_i < events.len() - 1 {
            if events[max_i + 1].time <= time + delta {
                max_i += 1;
            } else {
                break;
            }
        }
        min_i..(max_i + 1)
    }

    fn get_base_duration(stats: &mut PlayerStats<LogBuffUptimes>, skill_id: u32, duration: u32, time: i64) -> u32 {
        let mut duration = match gamedata::get_skill_type(skill_id) {
            SkillType::Unknown => unreachable!("Unknown buff tracked"),
            SkillType::Ability => unreachable!("Ability tracked as buff"),
            SkillType::Condition => (duration as f64 / stats.condition_duration(skill_id, time)) as u32,
            SkillType::Boon => (duration as f64 / stats.boon_duration(time)) as u32,
            SkillType::GenericBuff => duration,
        };

        if duration % 100 == 99 {
            duration += 1
        }

        duration
    }

    let sorted_events: Vec<_> = evtc_log.combat_items.iter().sorted_by_key(|x| x.time).collect();

    eprintln!("WARNING: The extraction currently expects unique condition durations, especially for the Earth sigil. If there is any non-Earth 6s bleed application, it will be sourced incorrectly!");

    for (i, event) in sorted_events.iter().enumerate() {
        if event.is_state_change == 8 && event.src_agent == target.address {
            // Health update (statechange 8) for the target
            target_health = event.dst_agent as f64 / 10000.;
        } else if (event.is_state_change == 18 && event.is_buff == 18) || event.is_buff_apply() {
            // Initial buff event (statechange 18) or a buff apply event
            let skill_id = event.skill_id;
            let agent = event.dst_agent;
            let source_agent = event.src_agent;
            let active = event.is_shields > 0;
            let duration = event.value as u32;
            let buff_stack_id = event.padding;

            if TRACKED_PLAYER_BUFF_IDS.contains(&skill_id)
                && source_agent == player.address && agent == player.address {
                // Limitation: Effects caused by enemies to players are implicit
                assert_eq!(event.is_offcycle, 0, "Boon extensions are not supported");

                let base_duration = get_base_duration(&mut stats, skill_id, duration, event.time);

                stats.buff_uptimes.add_stack(skill_id, duration as i64, event.time);
                simulation_events.push(SelfBuffApplication { time: event.time, skill_id, base_duration });
            }

            if TRACKED_DAMAGING_CONDITION_IDS.contains(&skill_id) {
                assert_eq!(event.is_offcycle, 0, "Boon extensions are not supported");
                assert!(gamedata::get_skill_type(skill_id) == SkillType::Condition);
                if source_agent == player.address && agent == target.address {
                    let base_duration = get_base_duration(&mut stats, skill_id, duration, event.time);
                    // Misery Swipe (mace aa1) 5s torment
                    // Anguish Swipe (mace aa2) 5s torment
                    // Manifest Toxin (mace aa3) 12s poison
                    // Searing Fissure (mace 2) 3s burning x3
                    //                          1s burning (pulses)
                    // Temporal Rift (axe5) 12s torment x4
                    // Shattershot (sb aa) 3s bleed
                    // Bloodbane Path (sb 2) 8s bleed x3
                    // Sevenshot (sb 3) 7s torment x7
                    // Spiritcrush (sb 4) 3s burning x4
                    // Scorchrazor (sb 5) 4s burning
                    // Citadel Bombardment 1.5s burning
                    // Embrace the Darkness 6s torment (1+2x if empowered)
                    // Geomancy 8s bleed x3
                    // Doom 8s poison x3
                    // Earth 6s bleed
                    // Invoke torment - 10s torment x2
                    //                - 10s poison x2
                    //                - 4s burning x2
                    // Expose Defenses - 5s vuln x5
                    // Icerazor 3s vuln x2
                    // Fire field projectile: 1s burning
                    let mut source = ConditionApplicationSource::Unknown;
                    if skill_id == ids::skills::BLEEDING && base_duration == 8000 {
                        // Geomancy candidate
                        // Can also be Bloodbane Path (sb2)
                        let mut candidate_bleeds = 0;
                        let mut ring_of_earth_found = false;
                        for j in get_same_time_events(&sorted_events, i, 5) {
                            let event = sorted_events[j];
                            if event.is_physical_hit() && event.skill_id == ids::skills::RING_OF_EARTH {
                                ring_of_earth_found = true;
                            }
                            if event.is_buff_apply() && event.skill_id == ids::skills::BLEEDING && event.value as u32 == duration {
                                candidate_bleeds += 1;
                            }
                        }
                        if ring_of_earth_found && candidate_bleeds == 3 {
                            source = ConditionApplicationSource::Skill(ids::skills::RING_OF_EARTH);
                        } else if ring_of_earth_found && candidate_bleeds > 3 {
                            unimplemented!("Only marking the first 3 bleed stacks as Geomancy effect is not implemented yet.")
                        }
                    }

                    if skill_id == ids::skills::POISONED && base_duration == 8000 {
                        // Doom candidate
                        let mut candidate_poisons = 0;
                        let mut doom_removal_found = false;
                        for j in get_same_time_events(&sorted_events, i, 5) {
                            let event = sorted_events[j];
                            if event.is_buff_remove() && event.skill_id == ids::skills::DOOM {
                                doom_removal_found = true;
                            }
                            if event.is_buff_apply() && event.skill_id == ids::skills::POISONED && event.value as u32 == duration {
                                candidate_poisons += 1;
                            }
                        }
                        if doom_removal_found && candidate_poisons == 3 {
                            source = ConditionApplicationSource::Sigil(Sigil::Doom);
                        } else if doom_removal_found && candidate_poisons > 3 {
                            unimplemented!("Only marking the first 3 poison stacks as Doom effect is not implemented yet.")
                        }
                    }

                    if skill_id == ids::skills::BLEEDING && base_duration == 6000 {
                        // Earth candidate
                        source = ConditionApplicationSource::Sigil(Sigil::Earth);
                    }
                    simulation_events.push(TargetConditionApplication {
                        time: event.time,
                        condition: DamagingCondition::from_id(skill_id),
                        base_duration,
                        source,
                    });
                }
            }

            if TRACKED_TARGET_BUFF_IDS.contains(&skill_id)
                && source_agent == player.address && agent == target.address {
                assert_eq!(event.is_offcycle, 0, "Boon extensions are not supported");
                let base_duration = get_base_duration(&mut stats, skill_id, duration, event.time);

                target_buffs.add_stack(skill_id, duration as i64, event.time);
                simulation_events.push(TargetBuffApplication { time: event.time, skill_id, base_duration });
            }
        } else if event.is_state_change == 11 {
            if event.src_agent == player.address {
                if event.dst_agent == 4 {
                    simulation_events.push(SimulationEvent::WeaponSwap { time: event.time, weapon_set: WeaponSet::Land1 });
                } else if event.dst_agent == 5 {
                    simulation_events.push(SimulationEvent::WeaponSwap { time: event.time, weapon_set: WeaponSet::Land2 });
                }
                // Other weapon sets are ignored
            }
        } else if event.is_buff_remove() {
            let skill_id = event.skill_id;
            let target_agent = event.src_agent;
            let remover_agent = event.dst_agent;
            let stack_count = event.result;
            if TRACKED_PLAYER_BUFF_IDS.contains(&skill_id) && target_agent == player.address {
                if event.is_buff_remove == 1 {
                    // last/all stack
                    stats.buff_uptimes.remove_last_stack(skill_id, event.time);
                } else if event.is_buff_remove == 2 {
                    // single stack
                    stats.buff_uptimes.remove_stack(skill_id, event.time);
                } else if event.is_buff_remove == 3 {
                    // manual single stack (extra by arc) when last/all
                    // should be ignorable?
                } else {
                    unreachable!("Invalid buff remove type")
                }
            }
            if TRACKED_TARGET_BUFF_IDS.contains(&skill_id)
                && target_agent == target.address {
                if event.is_buff_remove == 1 {
                    // last/all stack
                    target_buffs.remove_last_stack(skill_id, event.time);
                } else if event.is_buff_remove == 2 {
                    // single stack
                    target_buffs.remove_stack(skill_id, event.time);
                } else if event.is_buff_remove == 3 {
                    // manual single stack (extra by arc) when last/all
                    // should be ignorable?
                } else {
                    unreachable!("Invalid buff remove type")
                }
            }
        } else if event.is_physical_hit() {
            // Barrier is ignored, damage into barrier is counted as damage
            if event.result == 8 || event.result == 9 || event.result == 10 {
                // Killing blow (8) or enemy downed (9) or breakbar damage (10)
                continue;
            }
            let skill_id = event.skill_id;
            let damage = event.value;
            let crit = event.result == 1;

            assert_ne!(event.src_master_inst_id, player_inst_id); // minion damage, not implemented

            if event.src_agent != player.address || event.dst_agent != target.address {
                continue;
            }

            assert_ne!(event.result, 2); // glance
            assert_ne!(event.result, 3); // block
            assert_ne!(event.result, 4); // evade
            assert_ne!(event.result, 5); // interrupt of enemy
            assert_ne!(event.result, 6); // absorbed
            assert_ne!(event.result, 7); // blind - miss


            let skill_multiplier = match skill_id {
                ids::skills::SEARING_FISSURE => {
                    // Searing Fissure has different multipliers depending on whether
                    // it's the first strike or one of the additional ones.
                    let mut first_burnings = 0;
                    let mut additional_burnings = 0;
                    for j in get_same_time_events(&sorted_events, i, 5) {
                        let event = sorted_events[j];
                        if event.is_buff_apply()
                            && event.src_agent == player.address
                            && event.skill_id == ids::skills::BURNING {
                            let duration = event.value as u32;
                            let base_duration = get_base_duration(&mut stats, event.skill_id, duration, event.time);
                            if (base_duration as i64 - gamedata::SEARING_FISSURE_FIRST_STRIKE_BURNING_DURATION as i64).abs() < 5 {
                                first_burnings += 1;
                            } else if (base_duration as i64 - gamedata::SEARING_FISSURE_ADDITIONAL_STRIKE_BURNING_DURATION as i64).abs() < 5 {
                                additional_burnings += 1;
                            }
                        }
                    }

                    // This is likely also doable by checking for skill cast events
                    // in case this is ever unreliable.
                    if first_burnings == 3 && additional_burnings == 0 {
                        gamedata::SEARING_FISSURE_FIRST_STRIKE_MULTIPLIER
                    } else if first_burnings == 0 && additional_burnings == 1 {
                        gamedata::SEARING_FISSURE_ADDITIONAL_STRIKE_MULTIPLIER
                    } else if first_burnings >= 3 {
                        // May be a false positive in case many skills with the same base duration
                        // land at the same time.
                        eprintln!("Warning, unsure about Searing Fissure type: {} burn matching first burn, {} burn matching additional burn, guessing first strike", first_burnings, additional_burnings);
                        gamedata::SEARING_FISSURE_FIRST_STRIKE_MULTIPLIER
                    } else {
                        // First burnings < 3, very unlikely to be first strike
                        eprintln!("Warning, unsure about Searing Fissure type: {} burn matching first burn, {} burn matching additional burn, guessing additional strike", first_burnings, additional_burnings);
                        gamedata::SEARING_FISSURE_ADDITIONAL_STRIKE_MULTIPLIER
                    }
                }
                _ => gamedata.power_multiplier(skill_id).expect("Failed to find skill multiplier")
            };

            let target_armor = target.toughness as u32 + gamedata::BASE_ENEMY_ARMOR as u32;
            let mut base_damage = damage as f64 / stats.power(event.time) as f64 / skill_multiplier * target_armor as f64;
            if crit {
                let crit_damage = 1.5 + stats.ferocity(event.time) as f64 / 1500.;
                base_damage /= crit_damage;
            }
            base_damage /= 1. + target_buffs.get_stack_count(ids::skills::VULNERABILITY, event.time) as f64 * 0.01;
            base_damage /= stats.power_damage_mult(event.time, &mut target_buffs, target_health);
            //eprintln!("{}->{} | skill {} | PWR {} | CRIT {} | FERO {} |", base_damage, damage, skill_id, stats.power(event.time), crit, stats.ferocity(event.time))


            simulation_events.push(PhysicalHit {
                time: event.time,
                base_damage: base_damage as u32,
                coefficient: skill_multiplier,
                source: PhysicalHitSource::Skill(skill_id),
                enemy_armor: target_armor,
                critical: crit,
            });
        } else if event.is_buff > 0 && event.value == 0 && event.is_state_change == 0 && event.is_activation == 0 && event.is_buff_remove == 0 {
            // Buff damage
            let damage = event.buff_dmg;
            if event.result != 0 {
                // Damage did not hit.
                continue;
            }
            if event.src_agent != player.address {
                continue;
            }

            if event.is_offcycle > 0 {
                assert_eq!(event.skill_id, ids::skills::BATTLE_SCARS);
                if event.skill_id == ids::skills::BATTLE_SCARS {
                    simulation_events.push(SimulationEvent::LifeStealHit {
                        time: event.time,
                        base_damage: gamedata::BATTLE_SCARS_DAMAGE,
                        power_scaling: gamedata::BATTLE_SCARS_MULTIPLIER,
                        source: LifeStealSource::Buff(ids::skills::BATTLE_SCARS),
                    });
                } else {
                    unimplemented!();
                }
            } else {
                let damaging_condition = DamagingCondition::from_id(event.skill_id);
                if damaging_condition == DamagingCondition::Torment {
                    let vuln_multiplier = 1. + target_buffs.get_stack_count(ids::skills::VULNERABILITY, event.time) as f64 * 0.01;
                    let assuming_moving = damage as f64 / stats.condition_damage_mult(damaging_condition, event.time) / vuln_multiplier
                        - gamedata::TORMENT_MOVING_MULTIPLIER * stats.condition_damage(event.time) as f64;
                    let assuming_static = damage as f64 / stats.condition_damage_mult(damaging_condition, event.time) / vuln_multiplier
                        - gamedata::TORMENT_MULTIPLIER * stats.condition_damage(event.time) as f64;
                    if assuming_moving >= 0. {
                        eprintln!("[{}] static {} moving {} | damage {} mult {}, cdamage {}, vuln_mult {}",
                                  event.time, assuming_static, assuming_moving,
                                  damage,
                                  stats.condition_damage_mult(damaging_condition, event.time),
                                  stats.condition_damage(event.time),
                                  vuln_multiplier
                        );
                        unimplemented!("Detecting moving torment is not implemented, but seems to appear in the log.")
                    }
                }

                if event.time - last_condition_tick > 5 {
                    assert_eq!(gamedata::get_skill_type(event.skill_id), SkillType::Condition);
                    let moving = false;
                    // Always assumed to be unmoving, the code above detects it
                    // (assuming it's actually correct), but to properly use it,
                    // we'd need to run it on all damage events, not just the
                    // first one from the batch (that one might not be torment).
                    simulation_events.push(ConditionTick { time: event.time, target_moving: moving });
                }
                last_condition_tick = event.time;
            }
        }
        // TODO: life steal (dark field)
        // TODO: combo fields?
    }

    simulation_events
}