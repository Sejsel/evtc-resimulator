pub mod parsing;

pub struct EvtcLog {
    pub version: String,
    pub revision: u8,
    pub boss_species_id: u16,
    pub agents: Vec<EvtcAgent>,
    pub skills: Vec<EvtcSkill>,
    pub combat_items: Vec<EvtcCombatItem>,
}

pub struct EvtcAgent {
    pub address: u64,
    pub profession: u32,
    pub is_elite: u32,
    pub toughness: i16,
    pub concentration: i16,
    pub healing: i16,
    pub condition: i16,
    pub hitbox_width: i16,
    pub hitbox_height: i16,
    pub name: String,
}

pub struct EvtcSkill {
    pub id: i32,
    pub name: String,
}

pub struct EvtcCombatItem {
    pub time: i64,
    pub src_agent: u64,
    pub dst_agent: u64,
    pub value: i32,
    pub buff_dmg: i32,
    pub overstack_value: u32,
    pub skill_id: u32,
    pub src_inst_id: u16,
    pub dst_inst_id: u16,
    pub src_master_inst_id: u16,
    pub dst_master_inst_id: u16,
    pub iff: u8,
    pub is_buff: u8,
    pub result: u8,
    pub is_activation: u8,
    pub is_buff_remove: u8,
    pub is_ninety: u8,
    pub is_fifty: u8,
    pub is_moving: u8,
    pub is_state_change: u8,
    pub is_flanking: u8,
    pub is_shields: u8,
    pub is_offcycle: u8,
    pub padding: u32,
}

impl EvtcAgent {
    pub fn is_player(&self) -> bool {
        self.is_elite != 0xff_ff_ff_ff
    }
}

impl EvtcCombatItem {
    /// Does not include initial buff statechange.
    pub fn is_buff_apply(&self) -> bool {
        self.is_buff > 0 && self.buff_dmg == 0 && self.is_state_change == 0 && self.is_activation == 0 && self.is_buff_remove == 0 && self.value != 0
    }

    pub fn is_physical_hit(&self) -> bool {
        self.is_state_change == 0 && self.is_activation == 0 && self.is_buff_remove == 0 && self.is_buff == 0
    }

    pub fn is_buff_remove(&self) -> bool {
        self.is_state_change == 0 && self.is_activation == 0 && self.is_buff_remove > 0 && self.is_buff > 0
    }
}
