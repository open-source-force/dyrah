use glam::IVec2;

pub struct SpellDef {
    pub name: &'static str,
    pub mana_cost: f32,
    pub cooldown: f32,
    pub damage: f32,
    pub range: i32,
}

pub const GRAN_MAS_FLAM: SpellDef = SpellDef {
    name: "exevo gran mas flam",
    mana_cost: 60.0,
    cooldown: 4.0,
    damage: 10.0,
    range: 4,
};

pub fn get(name: &str) -> Option<&'static SpellDef> {
    match name {
        "exevo gran mas flam" => Some(&GRAN_MAS_FLAM),
        _ => None,
    }
}

pub fn area(origin: IVec2, range: i32) -> Vec<IVec2> {
    let mut tiles = Vec::new();
    for y in -range..=range {
        for x in -range..=range {
            let shrink = y.abs();
            if x.abs() <= range - shrink {
                tiles.push(origin + IVec2::new(x, y));
            }
        }
    }
    tiles
}
