//! Monstruos y NPC: el aspecto de cada número de monstruo que manda el servidor.
//!
//! Es lo que hace `CreateMonster(Type, x, y, Key)` (ZzzCharacter.cpp:12577 del Main 5.2): primero los
//! creadores de cada mapa (Cursed Temple, Hellas, Battle Castle, Crywolf, Aida, Kanturu, Elbeland,
//! Raklion...), luego el `switch(Type)` grande, y al final `Setting_Monster` (escala de Blood Castle y tipo).
//! La tabla ([`table`]) la genera `tools/gen_monsters.py` recorriendo ese código; aquí van los tipos, la
//! búsqueda y lo que el original calcula aparte:
//! - [`Def`]: modelo, escala, armas (solo el tipo y nivel del item: dibujarlas es cosa de los items),
//!   `Level`, `BlendMesh`, `HiddenMesh`...
//! - [`MonsterModel`]: `PlaySpeed` de cada acción y `Loop` de `OpenMonsterModel` (ZzzOpenData.cpp:2367).
//! - [`NpcModel`]: archivo y `PlaySpeed` de los modelos de `OpenNpc` (ZzzOpenData.cpp:1865).
//!
//! Los nombres que lee el original (`NpcName_<idioma>.txt`, `OpenMonsterScript`) no están en nuestra Data:
//! salen de `Monsters/Monsters.json` de MuMain (en español si lo tiene).

mod table;

pub use table::{DEFAULT_DEF, DEFS, MONSTER_MODELS, MONSTER_WAVES, NPC_MODELS, OTHER_LINK_BONES};

/// Acciones de los modelos de monstruo y NPC (`MONSTER01_*`, _define.h:483-494).
pub mod action {
    pub const STOP1: usize = 0;
    pub const STOP2: usize = 1;
    pub const WALK: usize = 2;
    pub const ATTACK1: usize = 3;
    pub const ATTACK2: usize = 4;
    pub const SHOCK: usize = 5;
    pub const DIE: usize = 6;
    pub const APEAR: usize = 7;
    pub const ATTACK3: usize = 8;
    pub const ATTACK4: usize = 9;
    pub const RUN: usize = 10;
    pub const ATTACK5: usize = 11;
    /// Las que tienen `PlaySpeed` propio en las tablas.
    pub const COUNT: usize = 12;
}

/// Un item (arma, pieza de armadura...): `MODEL_<grupo> + index` del original, con su nivel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item {
    /// 0 espadas, 1 hachas, 2 mazas, 3 lanzas, 4 arcos, 5 bastones, 6 escudos, 7 cascos, 8 armaduras,
    /// 9 pantalones, 10 guantes, 11 botas, 12 alas, 13 ayudantes...
    pub group: u8,
    pub index: u16,
    pub level: u8,
    /// `Option1` (excelente).
    pub option: u8,
}

/// Pieza del cuerpo (`c->BodyPart[...]`) de los que usan el esqueleto del jugador o los mercaderes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    None,
    /// Una armadura de jugador (`MODEL_HELM + n`...).
    Item(Item),
    /// Una pieza de los mercaderes (`MODEL_MERCHANT_MAN_HEAD`...): ver [`NpcModel::parts`].
    Npc(&'static str),
}

/// Qué modelo se dibuja.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Model {
    /// `MODEL_MONSTER01 + n`: `Data/Monster/Monster{n+1:02}.bmd`.
    Monster(u16),
    /// Un modelo de `OpenNpc` (nombre del `MODEL_*`): [`npc_model`].
    Npc(&'static str),
    /// `MODEL_PLAYER`: el esqueleto de `player.bmd` con [`Def::sub_type`] (esqueletos) o [`Def::body`].
    Player,
    /// `CreateCharacter(Key, n, ...)`: el modelo del objeto `n` del mapa (las trampas).
    Object(u16),
    /// Otro modelo global (`MODEL_WARCRAFT`, `MODEL_BALL`): [`other_model`].
    Other(&'static str),
}

/// `o->SubType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubType {
    Value(i32),
    /// Un modelo (`MODEL_SKELETON1`...): lo que se dibuja sobre el esqueleto del jugador.
    Model(&'static str),
}

/// `o->Kind` (`KIND_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Monster,
    Npc,
    Trap,
    /// `KIND_TMP` (el 451).
    Tmp,
}

/// El aspecto de un número de monstruo, como lo deja `CreateMonster` (lo que no pone vale lo de
/// `CreateCharacterPointer`, ZzzCharacter.cpp:11263: [`BASE`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Def {
    pub number: u16,
    /// Mapas (`WorldActive`) en los que vale esta entrada; vacío = en los demás.
    pub maps: &'static [u8],
    pub model: Model,
    pub scale: f32,
    /// `c->Weapon[0..2]` (mano derecha, izquierda).
    pub weapons: [Option<Item>; 2],
    /// `c->BodyPart[HELM, ARMOR, PANTS, GLOVES, BOOTS]`.
    pub body: [Part; 5],
    pub wing: Option<Item>,
    pub helper: Option<Item>,
    /// `c->Level`: los monstruos con nivel brillan distinto (`RenderCharacter`).
    pub level: u8,
    /// `o->HiddenMesh` (-1 = ninguna).
    pub hidden_mesh: i16,
    /// `o->BlendMesh` (-1 = ninguna).
    pub blend_mesh: i16,
    pub blend_mesh_light: f32,
    /// `c->m_bFixForm`: los remolinos no lo hacen girar (`StormTime`, ZzzEffect.cpp:149).
    pub fix_form: bool,
    pub hide_shadow: bool,
    /// `o->m_bRenderShadow`.
    pub render_shadow: bool,
    /// `c->Blood`: ya ha soltado su sangre (no la suelta al morir).
    pub blood: bool,
    pub sub_type: SubType,
    /// `o->Kind` puesto a mano en el `case` (si no, [`kind`]).
    pub kind: Option<Kind>,
    pub alpha_target: f32,
    /// `c->MoveSpeed`: unidades de MU por tick (100 = una casilla).
    pub move_speed: u8,
    pub edge_scale: f32,
    pub visible: bool,
    pub live: bool,
    pub light_enable: bool,
    pub pc_room: bool,
    pub life_time: Option<i32>,
    /// `c->Class` (los del Doppelganger con forma de personaje).
    pub class: Option<&'static str>,
    pub skin: u8,
    /// `SetAction` al crearlo.
    pub start_action: Option<u16>,
    /// `o->CurrentAction = PLAYER_*` (los NPC con forma de personaje).
    pub start_player_action: Option<&'static str>,
    /// `o->AnimationFrame` al crearlo.
    pub start_frame: Option<f32>,
    /// Altura sobre el suelo (`Position[2] = RequestTerrainHeight(...) + h`).
    pub height: Option<f32>,
    /// `Position[2] += z`.
    pub z_offset: f32,
    /// `Angle[i] = ...`.
    pub angle: [Option<f32>; 3],
    /// Giro de `CreateCharacter(..., Rotation)`.
    pub rotation: Option<f32>,
    /// Casilla desplazada en `CreateCharacter(Key, Type, x + dx, y + dy)`.
    pub tile_offset: (i8, i8),
    pub bbox_max: Option<[f32; 3]>,
    /// `Models[...].Actions[i].PlaySpeed = v` en el `case` (cambia el modelo para todos).
    pub model_speeds: &'static [(u8, f32)],
    /// `Models[...].Meshs[i].NoneBlendMesh`.
    pub none_blend: &'static [(u8, bool)],
    /// `Models[...].StreamMesh`.
    pub stream_mesh: Option<i16>,
    /// Lo del `case` que aún no se traduce (efectos, huesos con nombre...), con su archivo:línea.
    pub notes: &'static [&'static str],
    /// Archivo:línea del `case`.
    pub src: &'static str,
    /// Mapas (`WorldActive`) donde sale, según `Monsters.json` de MuMain (no es del original: solo para
    /// elegir monstruos de prueba).
    pub seen_in: &'static [u8],
    pub name: &'static str,
    /// Nombre en inglés si es distinto.
    pub name_en: &'static str,
}

/// Lo que deja `CreateCharacterPointer` (ZzzCharacter.cpp:11263-11539) antes del `case`.
pub const BASE: Def = Def {
    number: 0,
    maps: &[],
    model: Model::Monster(0),
    scale: 0.9,
    weapons: [None, None],
    body: [Part::None; 5],
    wing: None,
    helper: None,
    level: 0,
    hidden_mesh: -1,
    blend_mesh: -1,
    blend_mesh_light: 1.0,
    fix_form: false,
    hide_shadow: false,
    render_shadow: true,
    blood: false,
    sub_type: SubType::Value(0),
    kind: None,
    alpha_target: 1.0,
    move_speed: 10,
    edge_scale: 1.2,
    visible: false,
    live: true,
    light_enable: true,
    pc_room: false,
    life_time: None,
    class: None,
    skin: 0,
    start_action: None,
    start_player_action: None,
    start_frame: None,
    height: None,
    z_offset: 0.0,
    angle: [None; 3],
    rotation: None,
    tile_offset: (0, 0),
    bbox_max: None,
    model_speeds: &[],
    none_blend: &[],
    stream_mesh: None,
    notes: &[],
    src: "",
    seen_in: &[],
    name: "",
    name_en: "",
};

impl Def {
    /// Nombre en inglés, como el cliente S6 en inglés (`name_en` solo está si es distinto de `name`).
    pub fn display_name(&self) -> &'static str {
        if self.name_en.is_empty() { self.name } else { self.name_en }
    }
}

/// Las entradas de `number` que valen en el mapa `world_active` (`WorldActive`: Lorencia = 0): una, o
/// varias si el original elige con `rand()`. Si el número no sale en ningún `case`, la del `default:`.
pub fn defs(number: u16, world_active: i32) -> &'static [Def] {
    let start = DEFS.partition_point(|d| d.number < number);
    let end = DEFS.partition_point(|d| d.number <= number);
    let all = &DEFS[start..end];
    if all.is_empty() {
        return std::slice::from_ref(&DEFAULT_DEF);
    }
    // Las de mapas concretos van antes que la general.
    let fits = |d: &Def| d.maps.is_empty() || (0..=255).contains(&world_active) && d.maps.contains(&(world_active as u8));
    let Some(first) = all.iter().position(fits) else { return std::slice::from_ref(&DEFAULT_DEF) };
    let maps = all[first].maps;
    let len = all[first..].iter().take_while(|d| d.maps == maps).count();
    &all[first..first + len]
}

/// `Setting_Monster` (ZzzCharacter.cpp:12477-12549): `o->Kind` según el número (si el `case` no lo pone).
pub fn kind(number: u16) -> Kind {
    let t = number as i32;
    let mut k = if t == 200 || t >= 260 {
        Kind::Monster
    } else if t > 200 {
        Kind::Npc
    } else if t >= 150 || t > 110 {
        Kind::Monster
    } else if t >= 100 {
        Kind::Trap
    } else {
        Kind::Monster
    };
    if matches!(t, 368..=370) {
        k = Kind::Npc;
    }
    // ZzzCharacter.cpp:12524-12556 (los de `ASG_ADD_KARUTAN_NPC` y `LEM_ADD_LUCKYITEM` no están activos).
    const NPC: &[i32] = &[
        367, 371, 375, 376, 377, 379, 380, 381, 382, 383, 384, 385, 406, 407, 408, 414, 415, 416, 417, 450, 452, 453,
        464, 465, 467, 468, 469, 470, 471, 472, 473, 474, 475, 478, 479, 492, 540, 541, 542, 522, 543, 544, 545, 546,
        547,
    ];
    if NPC.contains(&t) {
        k = Kind::Npc;
    }
    if (480..=491).contains(&t) {
        k = Kind::Monster;
    }
    if t == 451 {
        k = Kind::Tmp;
    }
    k
}

impl Def {
    /// `o->Kind` final: el del `case` (`o->Kind = KIND_NPC`) lo pisa `Setting_Monster`, que va después.
    pub fn kind(&self) -> Kind {
        kind(self.number)
    }

    /// `Object.Scale` con lo que suma `Setting_Monster` en Blood Castle (ZzzCharacter.cpp:12479-12487):
    /// los 84..=143 crecen `int(nCastle / 3) · 0.05`, con `nCastle = 7 + (WorldActive - 17)` entre 1 y 7.
    pub fn scale_in(&self, world_active: i32) -> f32 {
        let castle = 7 + (world_active - 17);
        if (1..=7).contains(&castle) && (84..=143).contains(&self.number) {
            self.scale + (castle / 3) as f32 * 0.05
        } else {
            self.scale
        }
    }

    /// `c->Level` con lo de `Setting_Monster`: en Blood Castle los 84..=143 quedan con nivel 0.
    pub fn level_in(&self, world_active: i32) -> u8 {
        let castle = 7 + (world_active - 17);
        if (1..=7).contains(&castle) && (84..=143).contains(&self.number) { 0 } else { self.level }
    }
}

/// Lo de `OpenMonsterModel` para un `MonsterNN.bmd`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonsterModel {
    /// `Actions[i].PlaySpeed` (fotogramas por tick de 25 Hz); `None` = el original no la pone.
    pub speeds: [Option<f32>; action::COUNT],
    /// Bit `i`: `Actions[i].Loop` (la animación se queda en su último fotograma).
    pub hold: u16,
    /// `BoneHead` (-1 = ninguno).
    pub bone_head: i16,
    /// `StreamMesh` (-1 = ninguna).
    pub stream_mesh: i16,
    /// `Sounds[0..10]`: ranuras de `SOUND_MONSTER` ([`MONSTER_WAVES`]); -1 = ninguna.
    pub sounds: [i16; 10],
    /// `Weapon[0..2].LinkBone`: hueso de la mano de cada arma.
    pub link_bones: [u8; 2],
}

/// `PlaySpeed` de una acción sin valor en el original: el `new Action_t[]` de `BMD::Open2` la deja sin
/// iniciar (ZzzBMD.cpp:2584-2596). Se usa la de `MONSTER01_STOP1`, 0.25 (ZzzOpenData.cpp:2413).
pub const UNSET_PLAY_SPEED: f32 = 0.25;

impl MonsterModel {
    pub fn play_speed(&self, action: usize) -> f32 {
        self.speeds.get(action).copied().flatten().unwrap_or(UNSET_PLAY_SPEED)
    }

    /// `Actions[action].Loop`: se queda en el último fotograma en vez de volver a empezar.
    pub fn holds(&self, action: usize) -> bool {
        action < 16 && self.hold & (1 << action) != 0
    }
}

/// `OpenMonsterModel` del modelo `MODEL_MONSTER01 + index`.
pub fn monster_model(index: u16) -> Option<&'static MonsterModel> {
    MONSTER_MODELS.get(index as usize)
}

/// Archivo del modelo `MODEL_MONSTER01 + index` (`AccessModel(..., "Monster", Type + 1)`).
pub fn monster_file(index: u16) -> String {
    format!("Monster{:02}.bmd", index + 1)
}

/// Carpeta de las texturas de `MODEL_MONSTER01 + index` (ZzzOpenData.cpp:2380-2391): las de Chaos
/// Castle 70-72 en `Npc`, la 74 de Battle Castle en `Object31`; el resto en `Monster`.
pub fn monster_texture_dir(index: u16, world_active: i32) -> &'static str {
    let chaos_castle = (18..=23).contains(&world_active) || world_active == 53;
    if chaos_castle && (70..=72).contains(&index) {
        "Npc"
    } else if world_active == 30 && index == 74 {
        "Object31"
    } else {
        "Monster"
    }
}

/// Un modelo de `OpenNpc`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NpcModel {
    /// `MODEL_*`.
    pub name: &'static str,
    /// Carpeta del .bmd bajo `Data` (`Npc`, `Object35`...).
    pub dir: &'static str,
    pub file: &'static str,
    /// Carpeta de su `OpenTexture` del `case` (al final se buscan primero en `Npc`, ZzzOpenData.cpp:2271).
    pub textures: &'static str,
    /// Otros modelos que carga el mismo `case` (cabeza, torso... de los mercaderes): (nombre, carpeta,
    /// archivo, carpeta de texturas).
    pub parts: &'static [(&'static str, &'static str, &'static str, &'static str)],
    /// `PlaySpeed` que cambian (el resto 0.25, ZzzOpenData.cpp:2268).
    pub speeds: &'static [(u8, f32)],
    /// Los `MODEL_LITTLESANTA+k`: el nombre de su primera textura lleva el número `k + 1` en la posición 9
    /// (ZzzOpenData.cpp:2170-2183, `xmassanta.jpg` → `xmassanta1.jpg`).
    pub texture_patch: bool,
    pub src: &'static str,
}

impl NpcModel {
    /// `Actions[action].PlaySpeed`: 0.25 (ZzzOpenData.cpp:2268) o el que le pone el segundo `switch`.
    pub fn play_speed(&self, action: usize) -> f32 {
        self.speeds.iter().find(|(a, _)| *a as usize == action).map_or(0.25, |(_, s)| *s)
    }
}

/// El modelo `MODEL_*` de `OpenNpc` (también sus piezas: `MODEL_MERCHANT_MAN_HEAD`... dan el `NpcModel`
/// que las carga).
pub fn npc_model(name: &str) -> Option<&'static NpcModel> {
    NPC_MODELS.iter().find(|m| m.name == name)
}

/// Pieza `name` de un modelo de `OpenNpc`: (carpeta, archivo, carpeta de texturas).
pub fn npc_part(name: &str) -> Option<(&'static str, &'static str, &'static str)> {
    NPC_MODELS.iter().flat_map(|m| m.parts.iter()).find(|p| p.0 == name).map(|p| (p.1, p.2, p.3))
}

/// Modelos globales que usan algunos números (y los `SubType` de los esqueletos): (carpeta, archivo,
/// carpeta de texturas). `OpenSkills` (ZzzOpenData.cpp:3973-4185) les da las texturas de `Skill`.
pub fn other_model(name: &str) -> Option<(&'static str, &'static str, &'static str)> {
    Some(match name {
        // ZzzOpenData.cpp:4048.
        "MODEL_WARCRAFT" => ("Skill", "HellGate.bmd", "Skill"),
        // ZzzOpenData.cpp:4114.
        "MODEL_BALL" => ("Skill", "Ball01.bmd", "Skill"),
        // ZzzOpenData.cpp:3988-3991.
        "MODEL_SKELETON1" => ("Skill", "Skeleton01.bmd", "Skill"),
        "MODEL_SKELETON2" => ("Skill", "Skeleton02.bmd", "Skill"),
        "MODEL_SKELETON3" => ("Skill", "Skeleton03.bmd", "Skill"),
        "MODEL_SKELETON_PCBANG" => ("Skill", "Skeleton03.bmd", "Skill"),
        // ZzzOpenData.cpp:3992.
        "MODEL_HALLOWEEN" => ("Skill", "Jack.bmd", "Skill"),
        // ZzzOpenData.cpp:455.
        "MODEL_GM_CHARACTER" => ("Skill", "youngza.bmd", "Skill"),
        // ZzzOpenData.cpp:4038-4041.
        "MODEL_PANDA" => ("Item", "panda.bmd", "Item"),
        "MODEL_SKELETON_CHANGED" => ("Item", "trans_skeleton.bmd", "Item"),
        _ => return None,
    })
}

/// `Weapon[0..2].LinkBone` del modelo de un monstruo (`CreateCharacterPointer`, ZzzCharacter.cpp:11540-11676;
/// el resto, 33 y 42).
pub fn link_bones(model: Model) -> [u8; 2] {
    match model {
        Model::Monster(i) => monster_model(i).map_or([33, 42], |m| m.link_bones),
        Model::Npc(n) | Model::Other(n) => OTHER_LINK_BONES.iter().find(|(k, _)| *k == n).map_or([33, 42], |(_, b)| *b),
        _ => [33, 42],
    }
}

/// Archivo de sonido de la ranura `SOUND_MONSTER + slot`.
pub fn monster_wave(slot: i16) -> Option<&'static str> {
    MONSTER_WAVES.iter().find(|(s, _)| *s as i16 == slot).map(|(_, f)| *f)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../referencias/MuMain/src/bin/Data")
    }

    #[test]
    fn monstruos_conocidos() {
        let one = |n: u16| {
            let d = defs(n, 0);
            assert_eq!(d.len(), 1, "{n}");
            d[0]
        };
        // ZzzCharacter.cpp:13464-13476: el toro con su hacha.
        let bull = one(0);
        assert_eq!(bull.model, Model::Monster(0));
        assert_eq!(bull.scale, 0.8);
        assert_eq!(bull.weapons[0], Some(Item { group: 1, index: 6, level: 0, option: 0 }));
        assert_eq!(bull.hidden_mesh, 0);
        // 13537-13541: Budge Dragon.
        let budge = one(2);
        assert_eq!((budge.model, budge.scale), (Model::Monster(2), 0.5));
        // 13450-13454: la araña es `Monster10.bmd`.
        let spider = one(3);
        assert_eq!((spider.model, spider.scale), (Model::Monster(9), 0.4));
        assert_eq!(monster_file(9), "Monster10.bmd");
        // 13579-13591: el esqueleto es el jugador con `Skeleton01.bmd`, espada y escudo.
        let skel = one(14);
        assert_eq!((skel.model, skel.scale, skel.sub_type), (Model::Player, 0.95, SubType::Model("MODEL_SKELETON1")));
        assert_eq!(skel.weapons[1], Some(Item { group: 6, index: 4, level: 0, option: 0 }));
        // 13544-13551: Dark Knight con nivel 1.
        let dk = one(10);
        assert_eq!((dk.model, dk.scale, dk.level), (Model::Monster(3), 0.8, 1));
        // Un número sin `case`: el `default:` (toro sin arma, escala de `CreateCharacterPointer`).
        assert_eq!(defs(9999, 0)[0].model, Model::Monster(0));
        assert_eq!(defs(9999, 0)[0].scale, 0.9);
        // Los nombres.
        assert_eq!(bull.display_name(), "Bull Fighter");
        assert_eq!(defs(1, 0)[0].display_name(), "Hound");
        assert_eq!(kind(0), Kind::Monster);
        assert_eq!(kind(240), Kind::Npc);
        assert_eq!(kind(100), Kind::Trap);
    }

    #[test]
    fn variantes_por_mapa_y_azar() {
        // 163 (Chaos Castle): `rand()%2` elige el arquero (Monster72) o el mago (Monster73).
        let cc = defs(163, 18);
        assert_eq!(cc.len(), 2);
        assert_eq!(cc[0].model, Model::Monster(71));
        assert_eq!(cc[1].model, Model::Monster(72));
        // 88: el nivel del arma depende del castillo (ZzzCharacter.cpp:12905).
        assert_eq!(defs(88, 11)[0].weapons[0].unwrap().level, 8);
        assert_eq!(defs(88, 15)[0].weapons[0].unwrap().level, 0);
        // Setting_Monster: en Blood Castle 7 (WorldActive 17) los 84..143 crecen 0.10.
        let d = defs(84, 17)[0];
        assert!((d.scale_in(17) - (d.scale + 0.10)).abs() < 1e-6);
        assert!((d.scale_in(0) - d.scale).abs() < 1e-6);
    }

    #[test]
    fn velocidades_de_animacion() {
        // ZzzOpenData.cpp:2413-2419 y el `switch` de después.
        let m = monster_model(0).unwrap();
        assert_eq!(m.play_speed(action::STOP1), 0.25);
        assert_eq!(m.play_speed(action::WALK), 0.34);
        assert_eq!(m.play_speed(action::DIE), 0.55);
        assert!(m.holds(action::DIE));
        assert!(!m.holds(action::WALK));
        assert_eq!(monster_model(2).unwrap().play_speed(action::WALK), 0.7);
        // Type 3: todo ×1.2 menos DIE (2422-2428).
        let dk = monster_model(3).unwrap();
        assert!((dk.play_speed(action::STOP1) - 0.3).abs() < 1e-6);
        assert_eq!(dk.play_speed(action::DIE), 0.55);
        // 14 (asesino): STOP2 con `Loop` (3168-3176).
        assert!(monster_model(14).unwrap().holds(action::STOP2));
        // NPC: 0.25 y los del segundo `switch` (BARNERT cae en el `case` de RAUL: 0.5).
        assert_eq!(npc_model("MODEL_SMITH").unwrap().play_speed(0), 0.25);
        assert_eq!(npc_model("MODAL_GENS_NPC_BARNERT").unwrap().play_speed(1), 0.5);
        assert_eq!(link_bones(Model::Monster(0)), [42, 33]);
    }

    /// Todos los modelos de la tabla existen en la Data del S6 y se leen.
    #[test]
    fn modelos_en_la_data() {
        let data = data();
        if !data.is_dir() {
            return;
        }
        let dir = |rel: &str| {
            let mut d = data.clone();
            for part in rel.split('/') {
                d = crate::fs::find(&d, part).unwrap_or_else(|| d.join(part));
            }
            d
        };
        let mut missing = Vec::new();
        let mut checked = 0;
        for d in DEFS.iter() {
            let (folder, file) = match d.model {
                Model::Monster(i) => ("Monster".to_string(), monster_file(i)),
                Model::Npc(n) => match npc_model(n) {
                    Some(m) => (m.dir.to_string(), m.file.to_string()),
                    None => {
                        missing.push(format!("{}: {n} sin OpenNpc", d.number));
                        continue;
                    }
                },
                Model::Other(n) => match other_model(n) {
                    Some((f, file, _)) => (f.to_string(), file.to_string()),
                    None => {
                        missing.push(format!("{}: {n} sin archivo", d.number));
                        continue;
                    }
                },
                _ => continue,
            };
            match crate::fs::read(&dir(&folder), &file).and_then(|b| crate::bmd::read(&b)) {
                Ok(_) => checked += 1,
                Err(e) => missing.push(format!("{} ({}): {e}", d.number, d.src)),
            }
        }
        eprintln!("{checked} modelos leídos; faltan {}:", missing.len());
        for m in &missing {
            eprintln!("  {m}");
        }
        // Los de la Data del S6 (los de Season posteriores que el Main 5.2 ya conoce pueden faltar).
        for n in [0u16, 1, 2, 3, 6, 7, 14, 17, 18, 19, 24, 26, 28, 33, 40, 52, 60, 69, 240, 251, 253, 254] {
            for d in defs(n, 0) {
                assert!(!missing.iter().any(|m| m.starts_with(&format!("{n} ")) || m.starts_with(&format!("{n}:"))), "{n} {:?}", d.model);
            }
        }
    }
}
