//! The item's own body drawing: `RenderPartObjectBody` (ZzzObject.cpp:6894-8906), translated into data.
//!
//! `RenderPartObjectEffect` sets `b->BodyLight` from the item level and calls `RenderPartObjectBody`
//! (ZzzObject.cpp:10098-10274) before the level / excellent / ancient glow passes. That function is a
//! long `if`/`else` over the model type: most items end in the default `RenderBody(RenderType, ...)`
//! (ZzzObject.cpp:8904), but about a hundred have their own `RenderMesh` / `RenderBody` calls (chrome
//! blades, scrolling flames, card shines...). Each call is a [`Pass`]; [`body_passes`] returns them in
//! order, with the `b->BodyLight` each one sees ([`LightOp`]).
//!
//! Conventions, as the callers of this module use the original:
//! - `RenderType` is `RENDER_TEXTURE` (`RenderObjectScreen` and `RenderLinkObject`; `RENDER_EXTRA` is
//!   only added for the monsters 67/137, ZzzCharacter.cpp:6789) and `RENDER_DOPPELGANGER` is never set.
//! - `Alpha` and `o->Alpha` are 1 (`ItemObjectAttribute`, ZzzObject.cpp:5300): a [`Pass`] only carries
//!   an alpha the branch writes as a constant.
//! - `o->BlendMesh`, `o->HiddenMesh` are the caller's [`crate::objects::render::ObjectDraw`] (`None` in
//!   the passes); `o->BlendMeshLight` is 1 and `o->BlendMeshTexCoordU/V` are 0 (see
//!   [`super::model::item_object_meshes`]).
//! - With `o->LightEnable` off (the inventory) a `RenderMesh` in the `RENDER_TEXTURE` branch does not
//!   set the color and keeps the last `glColor`; the passes assume it is `BodyLight` (what the
//!   preceding `RenderBody`, chrome, `StreamMesh` or `glColor3fv(b->BodyLight)` leave) except where the
//!   branch calls `glColor3f` with a constant ([`BodyPass::gl_color`]).
//!
//! Effects that branches spawn (`CreateSprite`, `CreateParticle`) are marked `TODO(fx)`; values that
//! come from per-frame static counters or `rand()` are marked `TODO` and left at a value the original
//! also uses at some moment.

use super::Item;
use super::group::*;
use crate::objects::Glow;
use crate::objects::render::flags::*;
use crate::objects::render::{Bitmap, Pass, Saw};

/// `sin(WorldTime · freq_ms)` (or its absolute value) `· amp + base`, `WorldTime` in ms.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wave {
    pub freq_ms: f32,
    pub amp: f32,
    pub base: f32,
    pub abs: bool,
}

impl Wave {
    pub fn at(self, world_time: f32) -> f32 {
        let s = (world_time * self.freq_ms).sin();
        (if self.abs { s.abs() } else { s }) * self.amp + self.base
    }
}

/// How a branch sets `b->BodyLight` from the `Light` it received (the level light).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LightOp {
    /// The incoming `b->BodyLight` (or restored with `VectorCopy(Light, b->BodyLight)`).
    #[default]
    Keep,
    /// `Vector(r, g, b, b->BodyLight)`.
    Set([f32; 3]),
    /// `BodyLight[c] = Light[src[c]] · k[c]`.
    Scale { k: [f32; 3], src: [usize; 3] },
    /// `BodyLight[c] = v + add[c]` with `v` the [`Wave`].
    Wave { wave: Wave, add: [f32; 3] },
    /// `BodyLight = (Light[0] · v, Light[0] · v, Light[0] · v)` (ZzzObject.cpp:7056, 7073, 7981).
    RedWave(Wave),
}

impl LightOp {
    pub fn apply(self, light: [f32; 3], world_time: f32) -> [f32; 3] {
        match self {
            LightOp::Keep => light,
            LightOp::Set(l) => l,
            LightOp::Scale { k, src } => [0, 1, 2].map(|c| light[src[c]] * k[c]),
            LightOp::Wave { wave, add } => {
                let v = wave.at(world_time);
                add.map(|a| v + a)
            }
            LightOp::RedWave(wave) => [light[0] * wave.at(world_time); 3],
        }
    }

    /// Depends on the incoming light (otherwise it replaces it).
    pub fn relative(self) -> bool {
        matches!(self, LightOp::Keep | LightOp::Scale { .. } | LightOp::RedWave(_))
    }

    /// Changes over time by itself.
    pub fn animated(self) -> bool {
        matches!(self, LightOp::Wave { .. } | LightOp::RedWave(_))
    }

    const fn scale(r: f32, g: f32, b: f32) -> Self {
        LightOp::Scale { k: [r, g, b], src: [0, 1, 2] }
    }
}

/// A call of the item's drawing with the `b->BodyLight` it sees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyPass {
    pub pass: Pass,
    pub light: LightOp,
    /// `glColor3f(...)` with a constant right before a `RenderMesh` that, with `o->LightEnable` off,
    /// keeps that color instead of `BodyLight` (`RENDER_TEXTURE` branch, not the `StreamMesh`).
    pub gl_color: Option<[f32; 3]>,
}

/// What the item's drawing leaves in `o` and `b` for the calls drawn after it (the glow passes,
/// `RenderPartObjectBodyColor`): `None` = unchanged.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BodyDraw {
    pub passes: Vec<BodyPass>,
    /// `o->BlendMesh = n`.
    pub blend_mesh: Option<i32>,
    /// `o->HiddenMesh = n`.
    pub hidden_mesh: Option<i32>,
    /// `o->BlendMeshLight = ...`.
    pub glow: Option<Glow>,
    /// `o->BlendMeshTexCoordU = ...`.
    pub tex_u: Option<Saw>,
    /// `b->StreamMesh = n`, kept by the model (`b` is the shared `Models[Type]`).
    pub stream_mesh: Option<i32>,
    /// `b->BodyLight` at the end, which `RenderPartObjectBodyColor2` starts from
    /// (`PartObjectColor2`, ZzzObject.cpp:6771). Not counted: the `BodyLight *= Alpha` of translucent
    /// chrome calls (ZzzBMD.cpp:1116-1127).
    pub light_after: LightOp,
}

/// What the drawing depends on besides the item.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyCtx {
    /// `b->HideSkin`: `true` in windows (`RenderObjectScreen` → `RenderPartObject(..., HideSkin = true)`,
    /// ZzzInventory.cpp:10602), `false` on characters (ZzzCharacter.cpp:9276).
    pub hide_skin: bool,
    /// `b->NumMeshs`.
    pub num_meshes: usize,
    /// `b->BodyLight == (1, 1, 1)` on entry (ZzzObject.cpp:6961, 8674).
    pub white_light: bool,
}

/// `BITMAP_CHROME+1`: `Effect\bab2.jpg` (`GL_LINEAR`, `GL_REPEAT`; ZzzOpenData.cpp:5093).
pub const CHROME_1: Bitmap = file("Effect/bab2.jpg", false, false);
/// `BITMAP_3RDWING_LAYER` (ZzzOpenData.cpp:1327).
const WING3_LAYER: Bitmap = file("Item/msword01_r.jpg", false, false);
/// `BITMAP_ROOLOFPAPER_EFFECT_R` (`GL_LINEAR`, default `GL_CLAMP_TO_EDGE`; ZzzOpenData.cpp:1342).
const ROLL_OF_PAPER_R: Bitmap = lc("Item/rollofpaper_R.jpg");
/// `BITMAP_CHROME_ENERGY` (`GL_NEAREST`, `GL_CLAMP_TO_EDGE`; ZzzOpenData.cpp:5126).
const CHROME_ENERGY: Bitmap = file("Effect/energy01.jpg", true, true);
/// `BITMAP_LAVA` (ZzzOpenData.cpp:5232).
const LAVA: Bitmap = lc("Effect/Lava.jpg");
/// `BITMAP_SOCKETSTAFF` (ZzzOpenData.cpp:5274).
const SOCKET_STAFF: Bitmap = lc("Item/soketmagic_stape02.jpg");
/// `BITMAP_FREETICKET_R` (ZzzOpenData.cpp:5280).
const FREE_TICKET_R: Bitmap = lc("Item/partCharge1/entrance_R.jpg");
/// `BITMAP_CHAOSCARD_R` (ZzzOpenData.cpp:5281).
const CHAOS_CARD_R: Bitmap = lc("Item/partCharge1/juju_R.jpg");
/// `BITMAP_LUCKY_SEAL_EFFECT43..45` (ZzzOpenData.cpp:5283-5285).
const SEAL_43: Bitmap = lc("Item/partCharge1/monmark01a.jpg");
const SEAL_44: Bitmap = lc("Item/partCharge1/monmark02a.jpg");
const SEAL_45: Bitmap = lc("Item/partCharge1/monmark03a.jpg");
/// `BITMAP_LUCKY_CHARM_EFFECT53` (ZzzOpenData.cpp:5287).
const CHARM_53: Bitmap = lc("Item/partCharge1/bujuck01alpa.jpg");
/// `BITMAP_RAREITEM1_R..5_R` (ZzzOpenData.cpp:5288-5292).
const RARE_R: [Bitmap; 5] = [
    lc("Item/partCharge1/expensiveitem01_R.jpg"),
    lc("Item/partCharge1/expensiveitem02a_R.jpg"),
    lc("Item/partCharge1/expensiveitem02b_R.jpg"),
    lc("Item/partCharge1/expensiveitem03a_R.jpg"),
    lc("Item/partCharge1/expensiveitem03b_R.jpg"),
];
/// `BITMAP_CHARACTERCARD_R`, `_R_MA`, `_R_DA` (ZzzOpenData.cpp:5294-5297).
const CHARACTER_CARD_R: Bitmap = lc("Item/partCharge3/alicecard_R.tga");
const CHARACTER_CARD_R_MA: Bitmap = lc("Item/Ingameshop/kacama_R.jpg");
const CHARACTER_CARD_R_DA: Bitmap = lc("Item/Ingameshop/kacada_R.jpg");
/// `BITMAP_NEWCHAOSCARD_GOLD_R`, `_RARE_R`, `_MINI_R` (ZzzOpenData.cpp:5299-5301).
const NEW_CHAOS_CARD_GOLD_R: Bitmap = lc("Item/partCharge3/jujug_R.jpg");
const NEW_CHAOS_CARD_RARE_R: Bitmap = lc("Item/partCharge3/jujul_R.jpg");
const NEW_CHAOS_CARD_MINI_R: Bitmap = lc("Item/partCharge3/jujum_R.jpg");
/// `BITMAP_ITEM_EFFECT_DBSTONE_R`, `HELLHORN_R`, `PFEATHER_R`, `DEYE_R` (ZzzOpenData.cpp:5307-5310).
const DEATH_BEAM_STONE_R: Bitmap = lc("Item/deathbeamstone_R.jpg");
const HELL_HORN_R: Bitmap = lc("Item/hellhorn_R.jpg");
const PHOENIX_FEATHER_R: Bitmap = lc("Item/phoenixfeather_R.tga");
const DEVIL_EYE_R: Bitmap = lc("Item/Deye_R.jpg");
/// `BITMAP_RAREITEM7..12` (ZzzOpenData.cpp:5357-5362).
const RARE_7_12: [Bitmap; 6] = [
    lc("Item/partCharge8/rareitem_ticket_7_body.jpg"),
    lc("Item/partCharge8/rareitem_ticket_8_body.jpg"),
    lc("Item/partCharge8/rareitem_ticket_9_body.jpg"),
    lc("Item/partCharge8/rareitem_ticket_10_body.jpg"),
    lc("Item/partCharge8/rareitem_ticket_11_body.jpg"),
    lc("Item/partCharge8/rareitem_ticket_12_body.jpg"),
];
/// `BITMAP_DOPPLEGANGGER_FREETICKET`, `BITMAP_BARCA_FREETICKET`, `BITMAP_BARCA7TH_FREETICKET`
/// (ZzzOpenData.cpp:5364-5366).
const DOPPELGANGER_TICKET: Bitmap = lc("Item/partCharge8/DoppelCard.jpg");
const BARCA_TICKET: Bitmap = lc("Item/partCharge8/BarcaCard.jpg");
const BARCA7_TICKET: Bitmap = lc("Item/partCharge8/Barca7Card.jpg");
/// `BITMAP_PACKAGEBOX_RED, BLUE, GOLD, GREEN, PUPLE, SKY` (`GL_LINEAR`, `GL_REPEAT`; ZzzOpenData.cpp:1748-1753).
const PACKAGE_BOX: [Bitmap; 6] = [
    file("Item/InGameShop/membership_item_red.jpg", false, false),
    file("Item/InGameShop/membership_item_blue.jpg", false, false),
    file("Item/InGameShop/membership_item_gold.jpg", false, false),
    file("Item/InGameShop/membership_item_green.jpg", false, false),
    file("Item/InGameShop/membership_item_pouple.jpg", false, false),
    file("Item/InGameShop/membership_item_sky.jpg", false, false),
];
/// `BITMAP_INGAMESHOP_PRIMIUM6`, `COMMUTERTICKET4`, `SIZECOMMUTERTICKET3` (ZzzOpenData.cpp:1773-1785).
const SHOP_PREMIUM6: Bitmap = file("Item/InGameShop/mebership_3items_green.jpg", false, false);
const SHOP_COMMUTER4: Bitmap = file("Item/InGameShop/mebership_3items_red.jpg", false, false);
const SHOP_SIZE_COMMUTER3: Bitmap = file("Item/InGameShop/mebership_3items_yellow.jpg", false, false);
/// `BITMAP_INVEN_ARMOR+0..7` and `BITMAP_INVEN_PANTS+0..7`: `Player\InvenArmorMale40-45.tga` (default
/// `GL_NEAREST`, `GL_CLAMP_TO_EDGE`; ZzzOpenData.cpp:587-590) and the lucky sets' (1844-1851).
const INVEN_ARMOR: [Bitmap; 8] = [
    nc("Player/InvenArmorMale40.tga"),
    nc("Player/InvenArmorMale41.tga"),
    nc("Player/InvenArmorMale42.tga"),
    nc("Player/InvenArmorMale43.tga"),
    nc("Player/InvenArmorMale44.tga"),
    nc("Player/InvenArmorMale45.tga"),
    nc("Player/LuckyItem/65/InvenArmorMale40_luck.tga"),
    nc("Player/LuckyItem/70/InvenArmorMale41_luck.tga"),
];
const INVEN_PANTS: [Bitmap; 8] = [
    nc("Player/InvenPantsMale40.tga"),
    nc("Player/InvenPantsMale41.tga"),
    nc("Player/InvenPantsMale42.tga"),
    nc("Player/InvenPantsMale43.tga"),
    nc("Player/InvenPantsMale44.tga"),
    nc("Player/InvenPantsMale45.tga"),
    nc("Player/LuckyItem/65/InvenPantsMale40_luck.tga"),
    nc("Player/LuckyItem/70/InvenPantsMale41_luck.tga"),
];
/// `BITMAP_SKIN_ARMOR_DEVINE`, `_PANTS_DEVINE`, `_ARMOR_SUCCUBUS`, `_PANTS_SUCCUBUS` (ZzzOpenData.cpp:593-600).
const SKIN_ARMOR_DEVINE: Bitmap = nc("Player/Item312_Armoritem.tga");
const SKIN_PANTS_DEVINE: Bitmap = nc("Player/Item312_Pantitem.tga");
const SKIN_ARMOR_SUCCUBUS: Bitmap = nc("Player/SkinClass706_upperitem.tga");
const SKIN_PANTS_SUCCUBUS: Bitmap = nc("Player/SkinClass706_loweritem.tga");

const fn file(path: &'static str, nearest: bool, clamp: bool) -> Bitmap {
    Bitmap::File { path, nearest, clamp }
}
/// `GL_LINEAR`, `GL_CLAMP_TO_EDGE`.
const fn lc(path: &'static str) -> Bitmap {
    file(path, false, true)
}
/// The `LoadBitmap` defaults: `GL_NEAREST`, `GL_CLAMP_TO_EDGE`.
const fn nc(path: &'static str) -> Bitmap {
    file(path, true, true)
}

const T: u32 = TEXTURE;
const TB: u32 = TEXTURE | BRIGHT;
const BC: u32 = BRIGHT | CHROME;

/// `RenderBody(flags, ..., o->HiddenMesh)`.
const fn body_h(flags: u32) -> Pass {
    Pass::body(flags)
}
/// `RenderBody(flags, ...)` without the `HiddenMesh` parameter (default -1).
const fn body(flags: u32) -> Pass {
    Pass::body(flags).hidden(-1)
}
/// `RenderMesh(i, flags, ...)` with `o->BlendMesh`.
const fn mesh(i: usize, flags: u32) -> Pass {
    Pass::mesh(i, flags)
}
/// `BlendMeshLight = sin(WorldTime · freq_ms) · amp + base`.
const fn pulse(freq_ms: f32, amp: f32, base: f32) -> Glow {
    Glow::Pulse { freq: freq_ms * 1000.0, amp, base }
}
/// `BlendMeshLight = |sin(WorldTime · freq_ms)| · amp + base`.
const fn abs_pulse(freq_ms: f32, amp: f32, base: f32) -> Glow {
    Glow::AbsPulse { freq: freq_ms * 1000.0, amp, base }
}
const fn wave(freq_ms: f32, amp: f32, base: f32) -> Wave {
    Wave { freq_ms, amp, base, abs: false }
}
const ZERO: Saw = Saw::ZERO;
const fn lin(k: f32) -> Saw {
    Saw::linear(k)
}

/// The calls being collected, with the `b->BodyLight` the next one will see.
struct Calls {
    out: BodyDraw,
    light: LightOp,
}

impl Calls {
    fn light(&mut self, l: LightOp) -> &mut Self {
        self.light = l;
        self
    }
    fn set(&mut self, r: f32, g: f32, b: f32) -> &mut Self {
        self.light(LightOp::Set([r, g, b]))
    }
    fn call(&mut self, pass: Pass) -> &mut Self {
        self.out.passes.push(BodyPass { pass, light: self.light, gl_color: None });
        self
    }
    /// A call after `glColor3f(color)`.
    fn call_color(&mut self, pass: Pass, color: [f32; 3]) -> &mut Self {
        self.out.passes.push(BodyPass { pass, light: self.light, gl_color: Some(color) });
        self
    }
    /// `RenderMesh(0, RENDER_TEXTURE, ..., texture)` and `RenderMesh(i, RENDER_TEXTURE)` for the
    /// other meshes (ZzzObject.cpp:7729-7731).
    fn first_textured(&mut self, texture: Bitmap, num_meshes: usize) -> &mut Self {
        self.call(mesh(0, T).texture(texture));
        for i in 1..num_meshes {
            self.call(mesh(i, T));
        }
        self
    }
}

/// The calls `RenderPartObjectBody` makes for `item` (ZzzObject.cpp:6894-8906), in order.
pub fn body_passes(item: &Item, ctx: &BodyCtx) -> BodyDraw {
    let mut c = Calls { out: BodyDraw::default(), light: LightOp::Keep };
    let (g, i) = (item.group, item.index);
    let hide = ctx.hide_skin;
    match (g, i) {
        // ZzzObject.cpp:6905.
        (ARMOR | GLOVES | PANTS | BOOTS, 15) => {
            c.call(body_h(T | CHROME).texture(CHROME_1)).call(body_h(T));
        }
        // ZzzObject.cpp:6910.
        (ARMOR | GLOVES | PANTS | BOOTS, 20) => {
            c.light(LightOp::scale(0.85, 0.85, 1.2)).call(body_h(T | CHROME).texture(CHROME_1));
            c.light(LightOp::Keep).call(body_h(T));
        }
        // ZzzObject.cpp:6919.
        (WING, 6) => {
            c.set(0.8, 0.6, 1.0).call(body_h(BC).glow(Glow::Steady(0.5)).texture(CHROME_1));
            c.set(1.0, 1.0, 1.0).call(body_h(T));
        }
        // ZzzObject.cpp:6926.
        (WING, 36) => {
            c.set(1.0, 0.7, 0.5).call(mesh(2, T)).call(mesh(0, TB));
            // TODO: `BlendMeshTexCoordU = (s_iTexAni / 4) · 0.25` with a static counter that advances once
            // per call, i.e. per frame (ZzzObject.cpp:6933-6937): frame-rate dependent; left at its
            // first value 0.
            c.set(0.9, 0.6, 0.3).call(mesh(1, TB).blend_mesh(1)).set(1.0, 1.0, 1.0);
        }
        // ZzzObject.cpp:6947.
        (WING, 39) => {
            c.set(1.0, 1.0, 1.0).call(mesh(1, T));
            let w = Wave { freq_ms: 0.001, amp: 0.3, base: 0.0, abs: true };
            c.light(LightOp::Wave { wave: w, add: [0.1; 3] }).call(mesh(0, TB).blend_mesh(0));
            let w = Wave { amp: 0.8, ..w };
            c.light(LightOp::Wave { wave: w, add: [0.0; 3] }).call(mesh(1, TB).blend_mesh(1).texture(WING3_LAYER));
        }
        // ZzzObject.cpp:6959.
        (WING, 40) => {
            c.set(1.0, 1.0, 1.0).call(if ctx.white_light { body_h(T) } else { mesh(0, T) });
        }
        // ZzzObject.cpp:6974.
        (WING, 42) => {
            c.set(1.0, 1.0, 1.0).call(body_h(T)).call(mesh(1, TB | CHROME6));
        }
        // ZzzObject.cpp:6981.
        (WING, 43) => {
            c.set(1.0, 1.0, 1.0).call(body_h(T)).call(mesh(1, BC));
        }
        // ZzzObject.cpp:6988.
        (SWORD, 19) => {
            c.call(mesh(0, T | METAL).blend_mesh(0));
            c.call(mesh(0, LIGHTMAP | T).blend_mesh(0).tex_coord(ZERO, lin(0.0001)).texture(Bitmap::Chrome));
            c.call(mesh(1, T).blend_mesh(0));
        }
        // ZzzObject.cpp:6994, 7000.
        (STAFF, 10) | (BOW, 18) => {
            c.call(mesh(0, T | METAL).blend_mesh(0));
            c.call(mesh(0, LIGHTMAP | T).blend_mesh(0).tex_coord(ZERO, lin(-0.0001)).texture(Bitmap::Chrome));
            c.call(mesh(1, T).blend_mesh(-1));
        }
        // ZzzObject.cpp:7006.
        (MACE, 13) => {
            c.call(mesh(0, T).blend_mesh(-1));
            c.call(mesh(0, LIGHTMAP | T).blend_mesh(0).tex_coord(ZERO, lin(-0.0001)).texture(Bitmap::Chrome));
        }
        // ZzzObject.cpp:7011.
        (BOW, 19) => {
            c.light(LightOp::scale(0.8, 0.0, 0.8));
            c.call(mesh(0, LIGHTMAP | T).blend_mesh(0).tex_coord(lin(-0.0002), ZERO).texture(Bitmap::Chrome));
            c.light(LightOp::Keep).call(mesh(0, CHROME | BRIGHT).blend_mesh(0).texture(CHROME_1)).call(body_h(T));
        }
        // ZzzObject.cpp:7022.
        (POTION, 45) => {
            c.call(body_h(T)).call(mesh(1, DARK | CHROME).tex_coord(lin(-0.0002), ZERO).texture(Bitmap::Chrome));
            // TODO(fx): three `BITMAP_LIGHT` sprites on bones 8, 10 and 11 (ZzzObject.cpp:7027-7036).
        }
        // ZzzObject.cpp:7038 (`o->Type >= MODEL_ETC+19 && <= MODEL_ETC+27`).
        (ETC, 19..=27) => {
            c.call(body_h(T));
            c.call(body_h(TB).blend_mesh(0).glow(pulse(0.0015, 0.5, 0.5)).texture(ROLL_OF_PAPER_R));
        }
        // ZzzObject.cpp:7044.
        (SWORD, 31) => {
            c.call(mesh(3, T).blend_mesh(-1)).call(mesh(1, T).blend_mesh(-1));
            // `Alpha = sinf(WorldTime*0.01f)` (ZzzObject.cpp:7052) is not used by the `BlendMesh` branch the
            // mesh takes when its texture is 1 (ZzzBMD.cpp:1171-1188); otherwise the `RENDER_TEXTURE`
            // branch draws it again with alpha test. TODO: that time-varying alpha is not carried.
            c.call(mesh(1, T).blend_mesh(1));
            c.call(mesh(0, CHROME | T).blend_mesh(0).tex_coord(ZERO, lin(0.001)).texture(Bitmap::Chrome));
            c.light(LightOp::RedWave(wave(0.001, 0.5, 0.5)));
            c.call(mesh(2, TB).blend_mesh(2).tex_coord(lin(0.0001), lin(-0.0005)));
        }
        // ZzzObject.cpp:7060.
        (SPEAR, 10) => {
            c.call(body(T)).call(mesh(1, T).blend_mesh(1).tex_coord(lin(0.0001), lin(0.0005)));
        }
        // ZzzObject.cpp:7065.
        (MACE, 7) => {
            c.call(body(T));
            c.light(LightOp::RedWave(wave(0.001, 0.5, 0.3)));
            c.call(mesh(2, T).blend_mesh(2).tex_coord(lin(0.001), lin(-0.0005)));
        }
        // ZzzObject.cpp:7076.
        (HELPER, 4) => {
            c.call(body(T)).set(0.8, 0.4, 0.1).call(body(BC));
        }
        // ZzzObject.cpp:7082.
        (HELPER, 5) => {
            c.call(body(T)).set(0.3, 0.8, 1.0).call(mesh(0, BC));
        }
        // ZzzObject.cpp:7088.
        (MACE, 8) => {
            let glow = pulse(0.001, 0.6, 0.4);
            c.out.glow = Some(glow);
            c.call(body_h(T).glow(glow)).call(mesh(0, T).blend_mesh(0).glow(glow));
        }
        // ZzzObject.cpp:7096.
        (MACE, 9) => {
            c.call(body_h(T));
            c.set(0.1, 0.3, 1.0).call(mesh(0, T).blend_mesh(0).glow(pulse(0.001, 0.6, 0.4)));
            c.set(0.6, 0.8, 1.0);
            c.call(mesh(1, T).blend_mesh(1).glow(Glow::Steady(1.0)).tex_coord(ZERO, lin(0.0003)).texture(Bitmap::Chrome));
            c.out.blend_mesh = Some(1);
            c.out.glow = Some(Glow::Steady(1.0));
        }
        // ZzzObject.cpp:7108.
        (SWORD, 26) => {
            c.call(mesh(0, T)).call(mesh(2, T));
            c.out.blend_mesh = Some(1);
            c.set(1.0, 0.0, 0.2).call(mesh(1, TB).blend_mesh(1));
            c.set(1.0, 1.0, 1.0).call(mesh(1, TB | CHROME).blend_mesh(1));
            c.call(mesh(3, TB).blend_mesh(1)).call(mesh(4, TB).blend_mesh(1));
            // TODO: `BlendMeshTexCoordV = (((int)(WorldTime*0.05)%16)/4)*0.25f` (ZzzObject.cpp:7122), a
            // stepped offset [`Saw`] cannot express; left at 0 (its value during the first 80 ms of 320).
            c.call(mesh(5, TB).blend_mesh(5));
        }
        // ZzzObject.cpp:7125.
        (SWORD, 27) => {
            c.call(body(T)).call(mesh(1, TB | CHROME));
        }
        // ZzzObject.cpp:7130.
        (SWORD, 28) => {
            c.call(body(T)).call(mesh(2, TB));
            let glow = pulse(0.001, 0.6, 0.4);
            c.out.blend_mesh = Some(1);
            c.out.glow = Some(glow);
            c.call(mesh(1, TB).blend_mesh(1).glow(glow));
        }
        // ZzzObject.cpp:7139.
        (MACE, 16) => {
            c.call(body(T).hidden(1)).call(mesh(1, TB)).call(mesh(3, TB)).call(mesh(2, TB));
        }
        // ZzzObject.cpp:7146.
        (STAFF, 30) => {
            c.call(body(T));
            c.out.blend_mesh = Some(1);
            c.out.glow = Some(Glow::Steady(1.0));
            c.set(1.0, 0.5, 0.5).call(mesh(1, TB).blend_mesh(1));
        }
        // ZzzObject.cpp:7154.
        (STAFF, 31) => {
            c.call(body(T)).call(mesh(1, TB));
            let glow = abs_pulse(0.001, 1.0, 0.1);
            c.out.blend_mesh = Some(0);
            c.out.glow = Some(glow);
            // Three identical additive calls: they add up.
            for _ in 0..3 {
                c.call(mesh(0, T).blend_mesh(0).glow(glow).texture(SOCKET_STAFF));
            }
        }
        // ZzzObject.cpp:7164.
        (STAFF, 32) => {
            c.call(body(T)).call(mesh(0, TB | CHROME)).call(mesh(2, TB)).call(mesh(3, TB)).call(mesh(3, TB));
        }
        // ZzzObject.cpp:7172.
        (SHIELD, 17) => {
            c.call(body(T)).call(mesh(1, TB)).call(mesh(2, TB | CHROME));
        }
        // ZzzObject.cpp:7178.
        (SHIELD, 18) => {
            c.call(body(T).hidden(1)).call(mesh(1, TB));
        }
        // ZzzObject.cpp:7183.
        (SHIELD, 19) => {
            c.call(mesh(0, T)).call(mesh(1, TB));
            let glow = abs_pulse(0.001, 1.0, 0.0);
            c.out.blend_mesh = Some(2);
            c.out.glow = Some(glow);
            c.call(mesh(2, TB).blend_mesh(2).glow(glow));
        }
        // ZzzObject.cpp:7191.
        (SHIELD, 20) => {
            c.call(body(T)).call(mesh(1, TB));
        }
        // ZzzObject.cpp:7196. `b->StreamMesh = 1` is never restored: the model keeps it.
        (SHIELD, 21) => {
            c.call_color(mesh(0, T).stream(1), [1.0; 3]);
            c.call(mesh(1, TB | CHROME).stream(1).tex_coord(ZERO, Saw::new(2000.0, -0.0005)));
            c.call(mesh(2, TB).stream(1));
            c.out.stream_mesh = Some(1);
        }
        // ZzzObject.cpp:7204.
        (POTION, 55..=57) => {
            c.call(body_h(T)).call(body_h(BRIGHT | CHROME2));
        }
        // ZzzObject.cpp:7209.
        (HELPER, 49) => {
            c.call(body(T).alpha(0.7).hidden(1));
            c.call(body(TB).blend_mesh(4).glow(pulse(0.002, 0.3, 0.7)).hidden(0));
        }
        // ZzzObject.cpp:7215 (the `BlendMesh` argument `0.5f` becomes the `int` 0).
        (HELPER, 50) => {
            c.call(body(T).hidden(0)).call(body(T).blend_mesh(0).glow(pulse(0.00004, 0.15, 0.5)).hidden(1));
        }
        // ZzzObject.cpp:7221.
        (HELPER, 51) => {
            c.call(body(T).alpha(0.7).hidden(0));
            c.call(body(TB).blend_mesh(0).glow(pulse(0.002, 0.3, 0.7)).hidden(1));
        }
        // ZzzObject.cpp:7227. `o->BlendMeshLight` is 1 here.
        (POTION, 64) => {
            // TODO: `PlayAnimation(..., 2/7, ...)` (ZzzObject.cpp:7234) changes its animation speed.
            c.call(body(T).glow(Glow::Steady(1.5)));
            c.call(body(TB).glow(Glow::Steady(0.25)).tex_coord(lin(0.005), lin(-0.005)).hidden(0));
            // TODO(fx): `BITMAP_LIGHT` sprite on bone 1 (ZzzObject.cpp:7243-7244).
        }
        // ZzzObject.cpp:7246.
        (HELPER, 52) => {
            c.call(body(T).alpha(0.9)).call(body(BC));
        }
        // ZzzObject.cpp:7251.
        (HELPER, 53) => {
            c.call(body(T).alpha(0.9));
        }
        // ZzzObject.cpp:7255-7278: `fLumi = (sin(WorldTime·0.0015) + 1.5) · 0.4`.
        (POTION, 65..=67) => {
            let tex = [DEATH_BEAM_STONE_R, HELL_HORN_R, PHOENIX_FEATHER_R][i as usize - 65];
            c.call(body_h(T)).call(mesh(0, TB).blend_mesh(0).glow(pulse(0.0015, 0.4, 0.6)).texture(tex));
        }
        // ZzzObject.cpp:7279.
        (POTION, 68) => {
            c.call(body_h(T)).call(mesh(1, TB).blend_mesh(1).glow(pulse(0.0015, 0.4, 0.6)).texture(DEVIL_EYE_R));
            c.call(mesh(1, BC).alpha(0.2).blend_mesh(-1));
        }
        // ZzzObject.cpp:7289: `(sin + 1.2) · 0.3`.
        (HELPER, 46..=48) => free_ticket(&mut c, FREE_TICKET_R),
        // ZzzObject.cpp:7295: `(sin + 1.2) · 0.4`.
        (POTION, 54) => card(&mut c, CHAOS_CARD_R),
        // ZzzObject.cpp:7302-7322: `(sin + 1) · 0.5`.
        (POTION, 58..=60) => {
            rare(&mut c, RARE_R[i as usize - 58]);
            c.call(mesh(1, TB));
        }
        // ZzzObject.cpp:7323-7334.
        (POTION, 61 | 62) => rare(&mut c, RARE_R[i as usize - 58]),
        // ZzzObject.cpp:7335: `(sin + 1.5) · 0.5`.
        (POTION, 53) => {
            c.call(body_h(T)).call(mesh(0, TB).blend_mesh(0).glow(pulse(0.0015, 0.5, 0.75)).texture(CHARM_53));
        }
        // ZzzObject.cpp:7341-7366: `(sin(WorldTime·0.001) + 1.5) · 0.25`.
        (HELPER, 43 | 93 | 44 | 94 | 116 | 45) => {
            let tex = match i {
                43 | 93 => SEAL_43,
                45 => SEAL_45,
                _ => SEAL_44,
            };
            c.call(body_h(T)).call(mesh(1, TB).blend_mesh(0).glow(pulse(0.001, 0.25, 0.375)).texture(tex));
            c.call(mesh(1, TB));
        }
        // ZzzObject.cpp:7367.
        (POTION, 70 | 71) => {
            c.call(body(T)).call(mesh(0, CHROME4));
        }
        // ZzzObject.cpp:7372-7391, 7518-7540: `RenderBody(RENDER_TEXTURE, ..., o->HiddenMesh)`.
        (POTION, 72..=82) | (HELPER, 54..=60 | 62..=65) | (POTION, 96..=98) => {
            c.call(body_h(T));
        }
        // ZzzObject.cpp:7392.
        (HELPER, 61) => free_ticket(&mut c, FREE_TICKET_R),
        // ZzzObject.cpp:7398.
        (POTION, 83) => rare(&mut c, RARE_R[4]),
        // ZzzObject.cpp:7404-7439.
        (POTION, 145..=150) => rare(&mut c, RARE_7_12[i as usize - 145]),
        // ZzzObject.cpp:7440-7457.
        (HELPER, 125) => free_ticket(&mut c, DOPPELGANGER_TICKET),
        (HELPER, 126) => free_ticket(&mut c, BARCA_TICKET),
        (HELPER, 127) => free_ticket(&mut c, BARCA7_TICKET),
        // ZzzObject.cpp:7458-7485 (the later `MODEL_POTION+91` branch, ZzzObject.cpp:7760, is never reached).
        (POTION, 91) => card(&mut c, CHARACTER_CARD_R),
        (POTION, 92) => card(&mut c, NEW_CHAOS_CARD_GOLD_R),
        (POTION, 93) => card(&mut c, NEW_CHAOS_CARD_RARE_R),
        (POTION, 95) => card(&mut c, NEW_CHAOS_CARD_MINI_R),
        // ZzzObject.cpp:7486-7517.
        (POTION, 84..=90 | 94) => {
            c.call(body(T));
        }
        // ZzzObject.cpp:7541.
        (MACE, 10) => {
            c.out.hidden_mesh = Some(2);
            c.out.blend_mesh = Some(2);
            c.out.glow = Some(Glow::Steady(1.0));
            c.call(body(T).blend_mesh(2).hidden(2));
            c.call(mesh(2, T).blend_mesh(2).tex_coord(lin(0.0005), lin(0.0005)));
        }
        // ZzzObject.cpp:7553.
        (MACE, 11) => {
            let u = lin(0.0008);
            c.call(body_h(T).glow(Glow::Steady(1.0)).tex_coord(u, ZERO));
            let glow = pulse(0.001, 0.3, 0.7);
            c.call(mesh(1, BC).blend_mesh(0).glow(glow));
            c.out.glow = Some(glow);
            c.out.tex_u = Some(u);
        }
        // ZzzObject.cpp:7561. The `glColor3f(0.43f,0.14f,0.6f)` before mesh 3 does nothing: the chrome
        // branch sets `BodyLight` on every vertex (ZzzBMD.cpp:1286-1289).
        (SWORD, 20) => {
            c.call(body_h(T));
            c.call(mesh(2, T).blend_mesh(2).glow(pulse(0.0008, 0.7, 0.5)));
            c.call(mesh(1, T).blend_mesh(1));
            c.call(mesh(3, BC).blend_mesh(3).tex_coord(lin(0.0001), lin(0.0005)));
        }
        // ZzzObject.cpp:7574. `b->StreamMesh = 1` is never restored: the model keeps it (also for the
        // first `RenderBody` of the next frames).
        (SWORD, 21) => {
            c.call(body_h(T).stream(1));
            c.call(mesh(0, T).blend_mesh(0).tex_coord(lin(0.0005), lin(0.0005)).stream(1));
            c.out.hidden_mesh = Some(0);
            c.call(mesh(1, T).blend_mesh(-1).tex_coord(ZERO, lin(0.0005)).stream(1));
            c.out.stream_mesh = Some(1);
        }
        // ZzzObject.cpp:7582 (its `Luminosity` is computed but not used).
        (ARMOR | GLOVES | PANTS | BOOTS, 23) => {
            c.light(LightOp::Scale { k: [0.3, 0.8, 1.0], src: [0, 1, 1] }).call(body_h(T | CHROME).texture(CHROME_1));
            c.light(LightOp::Keep).call(body_h(T));
        }
        // ZzzObject.cpp:7592. `b->StreamMesh = 0` during the calls.
        (BOW, 21) | (STAFF, 12) | (MACE, 14) | (SWORD, 22) => {
            c.call(mesh(0, T).stream(0));
            match (g, i) {
                (SWORD, 22) => {
                    let u = Saw::new(4000.0, 0.0004).plus(-0.7);
                    c.set(1.0, 0.7, 0.4).call(mesh(0, TB).stream(0).tex_coord(u, ZERO).texture(Bitmap::Chrome7));
                    c.set(0.7, 0.7, 0.7);
                }
                (STAFF, 12) => {
                    let s = Saw::new(2000.0, 0.0002).plus(-0.3);
                    c.call(mesh(0, TB).stream(0).tex_coord(s, s).texture(CHROME_ENERGY));
                }
                _ => {
                    let s = Saw::new(4000.0, 0.0002).plus(-0.3);
                    c.call(mesh(0, TB).stream(0).tex_coord(s, s).texture(Bitmap::Chrome6));
                }
            }
        }
        // ZzzObject.cpp:7616. `o->Alpha = 0.5f` (7625) only reaches the last call (the others' passes
        // take `Alpha`, ZzzObject.cpp:9073), whose `BlendMesh` -2 branch does not use it.
        (SWORD, 23) => {
            c.call(body(T)).call(mesh(0, T).blend_mesh(0));
            let w = wave(0.0015, 0.03, 0.3);
            c.light(LightOp::Wave { wave: w, add: [0.0, 0.0, 0.1] }).call(mesh(2, T).blend_mesh(-2));
            c.set(0.4, 0.4, 0.8).call(mesh(1, T).alpha(0.5).blend_mesh(-2));
        }
        // ZzzObject.cpp:7630-7653.
        (ARMOR | PANTS, 31) => {
            if hide {
                c.call(mesh(0, T)).call(mesh(1, T));
            } else {
                c.call(body_h(T));
            }
        }
        // ZzzObject.cpp:7654.
        (SWORD, 25) => {
            c.out.hidden_mesh = Some(1);
            c.call(body(T).hidden(1));
            c.call(mesh(1, T).alpha(0.5).blend_mesh(0).texture(LAVA));
            c.call(mesh(1, T).alpha(0.7).blend_mesh(1).tex_coord(ZERO, lin(0.0009)));
        }
        // ZzzObject.cpp:7661.
        (MACE, 15) => {
            c.call(body(T));
        }
        // ZzzObject.cpp:7665.
        (BOW, 22) => {
            c.out.hidden_mesh = Some(1);
            c.call(body(T).hidden(1)).call(mesh(1, T).blend_mesh(1)).call(mesh(2, TB).blend_mesh(-1));
            c.call(mesh(2, TB | CHROME5).alpha(0.5).blend_mesh(-1));
        }
        // ZzzObject.cpp:7673.
        (STAFF, 13) => {
            c.out.hidden_mesh = Some(1);
            c.call(body(T).hidden(1)).call(mesh(1, TB).blend_mesh(1).tex_coord(lin(0.0009), lin(0.0009)));
        }
        // ZzzObject.cpp:7679-7711.
        (ARMOR, 36) => skin_or_body(&mut c, hide, 2),
        (PANTS | GLOVES, 36) => skin_or_body(&mut c, hide, 0),
        // ZzzObject.cpp:7712.
        (HELM, 39..=44) => skin_or_body(&mut c, hide, [2, 1, 0, 2, 1, 2][i as usize - 39]),
        // ZzzObject.cpp:7723.
        (ARMOR, 39..=44) => {
            if hide {
                c.first_textured(INVEN_ARMOR[i as usize - 39], ctx.num_meshes);
            } else {
                c.call(body_h(T));
            }
        }
        // ZzzObject.cpp:7736.
        (PANTS, 39..=44) => {
            if hide {
                c.first_textured(INVEN_PANTS[i as usize - 39], ctx.num_meshes);
            } else {
                c.call(body_h(T));
            }
        }
        // ZzzObject.cpp:7749.
        (GLOVES, 44) => skin_or_body(&mut c, hide, 0),
        // ZzzObject.cpp:7760 (`PBG_ADD_CHARACTERCARD`).
        (HELPER, 97) => character_card(&mut c, CHARACTER_CARD_R_MA),
        (HELPER, 98) => character_card(&mut c, CHARACTER_CARD_R_DA),
        // The second chain (`bIsNotRendered`), ZzzObject.cpp:7795.
        (ARMOR | PANTS, 50 | 53) => {
            if hide {
                let tex = match (g, i) {
                    (ARMOR, 50) => SKIN_ARMOR_DEVINE,
                    (PANTS, 50) => SKIN_PANTS_DEVINE,
                    (ARMOR, _) => SKIN_ARMOR_SUCCUBUS,
                    _ => SKIN_PANTS_SUCCUBUS,
                };
                c.first_textured(tex, ctx.num_meshes);
            } else {
                c.call(body_h(T));
            }
        }
        // ZzzObject.cpp:7819-7866.
        (HELM, 49) => skin_or_body(&mut c, hide, 0),
        (ARMOR, 49) => skin_or_body(&mut c, hide, 2),
        (HELM, 50) => skin_or_body(&mut c, hide, 1),
        (HELM, 53) => skin_or_body(&mut c, hide, 2),
        // ZzzObject.cpp:7867.
        (SPEAR, 11) => {
            for m in 2..=5 {
                c.call(mesh(m, T));
            }
            c.call(mesh(0, TB).blend_mesh(0).glow(Glow::Steady(0.4)));
            c.call(mesh(1, TB).blend_mesh(1).glow(Glow::Steady(0.8)));
        }
        // ZzzObject.cpp:7877.
        (MACE, 18) => {
            for m in [0, 2, 3, 4, 5, 6] {
                c.call(mesh(m, T));
            }
            c.call(mesh(1, TB).blend_mesh(1).glow(Glow::Steady(0.5)));
        }
        // ZzzObject.cpp:7888.
        (BOW, 20) => {
            for m in [0, 1, 2, 6] {
                c.call(mesh(m, T).blend_mesh(-1));
            }
            let glow = pulse(0.002, 0.3, 0.5);
            c.call(mesh(3, T).blend_mesh(3).glow(glow)).call(mesh(4, T).blend_mesh(4).glow(glow));
            c.call_color(mesh(5, T).blend_mesh(-1), [1.0; 3]);
        }
        // ZzzObject.cpp:7904. Its `glColor3f(1,1,1)` is replaced by the `StreamMesh` color.
        (POTION, 28) => {
            c.call(mesh(1, T).blend_mesh(-1).tex_coord(ZERO, lin(0.0005)).stream(1));
            c.call(mesh(0, T).blend_mesh(-1).stream(-1));
        }
        // ZzzObject.cpp:7912.
        (POTION, 29) => {
            c.set(1.0, 1.0, 1.0).call(mesh(1, T).blend_mesh(-1).tex_coord(ZERO, lin(0.0005)).stream(1));
            // TODO: `fMeshLight` is a static that moves 0.01 per call (per frame) between 0.01 and 1
            // (ZzzObject.cpp:7920-7931): frame-rate dependent; left at its initial 0.5.
            c.set(1.0, 0.5, 0.0).call(mesh(2, T).blend_mesh(2).glow(Glow::Steady(0.5)).stream(-1));
            c.set(1.0, 1.0, 1.0).call(mesh(0, T).blend_mesh(-1).stream(-1));
            c.call(mesh(0, CHROME | BRIGHT).alpha(0.3).blend_mesh(-1).stream(-1));
        }
        // ZzzObject.cpp:7938.
        (STAFF, 11) => {
            c.call(body(T)).call(mesh(1, CHROME | BRIGHT).blend_mesh(1).glow(Glow::Steady(0.2)));
            c.call(mesh(0, TB).blend_mesh(0).glow(pulse(0.005, 1.0, 0.0)));
            c.call(mesh(0, CHROME4 | BRIGHT).blend_mesh(0));
        }
        // ZzzObject.cpp:7947.
        (STAFF, 18) => {
            c.call(body(T)).call(mesh(1, TB).blend_mesh(1).glow(Glow::Steady(1.0)));
        }
        // ZzzObject.cpp:7952: `(sin(WorldTime·0.002) + 0.5) · 0.5`.
        (STAFF, 19) => {
            c.call(body(T)).call(mesh(1, TB).blend_mesh(1).glow(pulse(0.002, 0.5, 0.25)));
        }
        // ZzzObject.cpp:7958.
        (MACE, 12) => {
            c.call(body(T)).call(mesh(1, TB).blend_mesh(1).glow(Glow::Steady(1.0)));
            c.call(mesh(3, TB).blend_mesh(3).glow(Glow::Steady(1.0)));
        }
        // ZzzObject.cpp:7964.
        (SHIELD, 15) => {
            c.light(LightOp::scale(0.3, 0.3, 0.3)).call(mesh(2, COLOR).blend_mesh(-1));
            c.light(LightOp::Keep);
            c.call(mesh(2, CHROME | BRIGHT).blend_mesh(2).tex_coord(ZERO, lin(0.01)).texture(Bitmap::Chrome));
            c.call(mesh(0, T).blend_mesh(-1));
            // TODO: `BlendMeshTexCoordU/V = (rand()%10)·0.1` on every frame (ZzzObject.cpp:7978); left at 0.
            c.call(mesh(1, T).blend_mesh(1));
            c.light(LightOp::RedWave(wave(0.001, 0.4, 0.6)));
            c.call(mesh(2, TB).blend_mesh(2).tex_coord(lin(0.0001), lin(0.0005)));
        }
        // ZzzObject.cpp:7986 (`glColor4f(..., 0.8f)` and `0.5f` go with the calls' own `Alpha`).
        (SHIELD, 16) => {
            c.call(mesh(1, T).alpha(0.8).blend_mesh(-1)).call(mesh(3, T).alpha(0.5).blend_mesh(-1));
            c.call(mesh(0, T).blend_mesh(-1)).call(mesh(2, T).blend_mesh(2).tex_coord(lin(0.0005), ZERO));
            // TODO: `BlendMeshTexCoordU/V = (rand()%10)·0.1` on every frame (ZzzObject.cpp:7997); left at 0.
            c.call(mesh(3, T).blend_mesh(3));
        }
        // ZzzObject.cpp:8017. `o->HiddenMesh` comes from the item level (ZzzObject.cpp:9817-9824); with
        // any other level nothing is drawn.
        (POTION, 7) => {
            if let Some(m) = level_mesh(item.level) {
                c.set(1.0, 1.0, 1.0).call(mesh(m, T));
                c.set(0.1, 0.5, 1.0).call(mesh(m, METAL | BRIGHT)).call(mesh(m, CHROME | BRIGHT));
            }
        }
        // ZzzObject.cpp:8040 (`o->HiddenMesh` from ZzzObject.cpp:9825-9832).
        (HELPER, 7) => {
            if let Some(m) = level_mesh(item.level) {
                c.set(1.0, 1.0, 1.0).call(mesh(m, T));
            }
        }
        // ZzzObject.cpp:8057.
        (HELPER, 11) => {
            c.set(1.0, 1.0, 1.0).call(mesh(0, T));
            c.set(0.0, 0.5, 1.0).call(mesh(1, CHROME | BRIGHT));
        }
        // ZzzObject.cpp:8067. TODO: with the `eBuff_InfinityArrow` buff on the owner it is drawn
        // yellow with a chrome pass (8069-8074); buffs are not tracked here (never in windows).
        (BOW, 7 | 15) => {
            c.call(body(T));
        }
        // ZzzObject.cpp:8080: the PC-room set (`o->m_bpcroom`) is never set by this client.
        // ZzzObject.cpp:8108.
        (POTION, 42..=44) => {
            c.call(body(T));
        }
        // ZzzObject.cpp:8112 (the seed spheres).
        (WING, 60..=65) => {
            let [r, g, b] = SEED_COLORS[i as usize - 60];
            c.set(r, g, b).call(body_h(T)).call(body_h(BC));
        }
        // ZzzObject.cpp:8139.
        (WING, 100..=129) => {
            c.call(mesh(1, T));
            let [r, g, b] = SEED_COLORS[(i as usize - 100) % 6];
            c.set(r, g, b).call(body(T).hidden(1)).call(body(BC).hidden(1));
        }
        // ZzzObject.cpp:8168. TODO: `_meshLight1` darkens meshes 1 and 2 with `b->BodyAngle[1]` between
        // 0 and 180 (8170-8179); left at the value for the other angles, 0.2.
        (HELPER, 71..=75) => {
            c.call(mesh(0, TB).blend_mesh(0).glow(Glow::Steady(0.35)));
            c.call(mesh(1, TB).blend_mesh(1).glow(Glow::Steady(0.2)));
            c.call(mesh(2, T).blend_mesh(2).glow(Glow::Steady(0.2)));
        }
        // ZzzObject.cpp:8586: `(sin(WorldTime·0.003) + 1) · 0.3 + 0.3`.
        (HELPER, 69) => {
            c.call(mesh(1, T)).call(mesh(0, TB).blend_mesh(0).glow(pulse(0.003, 0.3, 0.6)));
        }
        // ZzzObject.cpp:8592.
        (HELPER, 70) => {
            c.call(mesh(0, T)).call(mesh(1, T)).call(mesh(1, BC));
        }
        // ZzzObject.cpp:8598.
        (HELPER, 81) => {
            c.call(body_h(T)).call(mesh(0, BC)).call(mesh(1, TB));
        }
        // ZzzObject.cpp:8604.
        (HELPER, 82) => {
            c.call(mesh(0, TB)).call(mesh(1, T)).call(mesh(1, TB));
        }
        // ZzzObject.cpp:8610: `(sin(WorldTime·0.003) + 1) · 0.3 + 0.6`.
        (HELPER, 66) => {
            c.call(mesh(0, T)).call(mesh(0, BRIGHT | CHROME4).blend_mesh(0));
            c.call(mesh(1, TB).blend_mesh(1).glow(pulse(0.003, 0.3, 0.9)));
        }
        // ZzzObject.cpp:8618.
        (POTION, 100) => {
            c.call(mesh(0, T));
        }
        // ZzzObject.cpp:8622 (`MODEL_TYPE_CHARM_MIXWING + EWS_BEGIN..EWS_END` = `MODEL_HELPER+83..93`,
        // _enum.h:2286-2301; 93 took an earlier branch).
        (HELPER, 83..=92) => {
            c.call(body_h(T));
        }
        // ZzzObject.cpp:8626, 8641-8650.
        (HELPER, 107 | 109..=115) => {
            c.call(body_h(T)).call(mesh(1, BC).blend_mesh(0));
        }
        // ZzzObject.cpp:8631-8640.
        (HELPER, 104 | 105) => {
            c.call(body_h(T)).call(mesh(0, BC).alpha(0.2).blend_mesh(0));
        }
        // ZzzObject.cpp:8651-8660.
        (POTION, 112 | 113 | 120) => {
            c.call(body_h(T)).call(mesh(0, BC).blend_mesh(0));
        }
        // ZzzObject.cpp:8661.
        (POTION, 121 | 122) => {
            c.call(body_h(T)).call(mesh(1, BC).blend_mesh(1));
        }
        // ZzzObject.cpp:8666.
        (POTION, 123 | 124) => {
            c.call(mesh(1, T)).call(mesh(1, BC).blend_mesh(1)).call(mesh(2, T));
        }
        // ZzzObject.cpp:8672: drawn only with a white `BodyLight`.
        (WING, 130) => {
            if ctx.white_light {
                c.set(1.0, 1.0, 1.0).call(body_h(T));
            }
        }
        // ZzzObject.cpp:8681-8722.
        (POTION, 134..=139) => {
            c.call(mesh(0, T).blend_mesh(-1).texture(PACKAGE_BOX[i as usize - 134]));
        }
        (POTION, 114..=119) => {
            c.call(mesh(0, T).blend_mesh(-1).texture(SHOP_PREMIUM6));
        }
        (POTION, 126..=129) => {
            c.call(mesh(0, T).blend_mesh(-1).texture(SHOP_COMMUTER4));
        }
        (POTION, 130..=132) => {
            c.call(mesh(0, T).blend_mesh(-1).texture(SHOP_SIZE_COMMUTER3));
        }
        // ZzzObject.cpp:8723.
        (HELPER, 121) => free_ticket(&mut c, FREE_TICKET_R),
        // ZzzObject.cpp:8729.
        (POTION, 141..=144) => {
            c.call(mesh(0, T)).call(mesh(0, BC).blend_mesh(0)).call(mesh(1, T));
        }
        // ZzzObject.cpp:8735.
        (HELPER, 122) => {
            c.call(body_h(T)).call(body_h(BC).alpha(0.5).blend_mesh(0));
        }
        // ZzzObject.cpp:8868 (`LEM_ADD_LUCKYITEM`). `Check_LuckyItem(Type, -MODEL_ITEM)` compares the
        // model type (`MODEL_ITEM` = 1170, fx/tables/ids.rs) with `ITEM_HELM..ITEM_WING`
        // (ZzzInventory.cpp:8282-8290), which only covers the groups up to the pants; the lucky gloves
        // and boots take the default call, which draws the same.
        (STAFF..=PANTS, 62..=72) => {
            let hidden_part = match (g, i) {
                (HELM, 65) => Some(2),
                (HELM, 70) => Some(1),
                _ => None,
            };
            let inven = match (g, i) {
                (ARMOR, 65) => Some(INVEN_ARMOR[6]),
                (ARMOR, 70) => Some(INVEN_ARMOR[7]),
                (PANTS, 65) => Some(INVEN_PANTS[6]),
                (PANTS, 70) => Some(INVEN_PANTS[7]),
                _ => None,
            };
            match (hidden_part, inven) {
                (Some(m), _) => {
                    c.call(mesh(m, T));
                }
                (None, Some(tex)) if hide => {
                    c.first_textured(tex, ctx.num_meshes);
                }
                _ => {
                    c.call(body_h(T));
                }
            }
        }
        // ZzzObject.cpp:8904.
        _ => {
            c.call(body(T));
        }
    }
    c.out.light_after = c.light;
    c.out
}

/// The seed spheres' `b->BodyLight` (ZzzObject.cpp:8115-8135, 8144-8164).
const SEED_COLORS: [[f32; 3]; 6] =
    [[0.9, 0.1, 0.2], [0.4, 0.5, 1.0], [1.0, 1.0, 1.0], [0.4, 1.0, 0.6], [1.0, 0.8, 0.4], [1.0, 0.4, 1.0]];

/// `o->HiddenMesh` of `MODEL_POTION+7` and `MODEL_HELPER+7` (ZzzObject.cpp:9817-9832) → the mesh drawn.
fn level_mesh(level: u8) -> Option<usize> {
    match level {
        // `HiddenMesh = 1`: mesh 0.
        0 => Some(0),
        // `HiddenMesh = 0`: mesh 1.
        1 => Some(1),
        _ => None,
    }
}

/// `HideSkin` ? `RenderMesh(m)` : `RenderBody(..., o->HiddenMesh)`.
fn skin_or_body(c: &mut Calls, hide_skin: bool, m: usize) {
    if hide_skin {
        c.call(mesh(m, T));
    } else {
        c.call(body_h(T));
    }
}

/// The tickets: `fLumi = (sin(WorldTime·0.0015) + 1.2) · 0.3` on mesh 0 (ZzzObject.cpp:7289-7294).
fn free_ticket(c: &mut Calls, tex: Bitmap) {
    c.call(body_h(T)).call(mesh(0, TB).blend_mesh(0).glow(pulse(0.0015, 0.3, 0.36)).texture(tex));
}

/// The cards: `fLumi = (sin(WorldTime·0.0015) + 1.2) · 0.4`, and mesh 1 hidden in the bright body
/// (ZzzObject.cpp:7295-7301, 7458-7485).
fn card(c: &mut Calls, tex: Bitmap) {
    c.call(body_h(T)).call(body(TB).blend_mesh(0).hidden(1));
    c.call(mesh(0, TB).blend_mesh(0).glow(pulse(0.0015, 0.4, 0.48)).texture(tex));
}

/// `MODEL_HELPER+97/98` (ZzzObject.cpp:7760-7780): like a card without the bright body.
fn character_card(c: &mut Calls, tex: Bitmap) {
    c.call(body_h(T)).call(mesh(0, TB).blend_mesh(0).glow(pulse(0.0015, 0.4, 0.48)).texture(tex));
}

/// The rare tickets: `fLumi = (sin(WorldTime·0.0015) + 1) · 0.5` (ZzzObject.cpp:7302-7334).
fn rare(c: &mut Calls, tex: Bitmap) {
    c.call(body_h(T)).call(mesh(0, TB).blend_mesh(0).glow(pulse(0.0015, 0.5, 0.5)).texture(tex));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bmd::TextureScript;
    use crate::objects::render::{ChromeMode, LookBlend, LookColor, MeshInfo, ObjectDraw, resolve_all};

    const INVENTORY: BodyCtx = BodyCtx { hide_skin: true, num_meshes: 4, white_light: true };

    fn mesh_info(i: usize, script: Option<TextureScript>) -> MeshInfo {
        MeshInfo { index: i, texture_index: i as i32, script, has_alpha: false }
    }
    fn obj() -> ObjectDraw {
        ObjectDraw { blend_mesh: -1, stream_mesh: -1, tex_coord: [Saw::ZERO; 2], hidden_mesh: -1 }
    }
    fn passes(item: Item) -> Vec<Pass> {
        body_passes(&item, &INVENTORY).passes.iter().map(|p| p.pass).collect()
    }

    /// The Divine Sword of Archangel: the blade (mesh 0, `sword44_R.jpg`) is opaque metal plus an
    /// additive chrome wave; through `RenderBody` alone its `_R` script made it only additive.
    #[test]
    fn divine_sword_of_archangel() {
        let p = passes(Item::new(SWORD, 19));
        let blade = Some(TextureScript { bright: true, ..Default::default() });
        let looks = resolve_all(&p, &obj(), &mesh_info(0, blade));
        assert_eq!(looks.len(), 2, "{looks:?}");
        let (metal, chrome) = (looks[0].1, looks[1].1);
        assert_eq!((metal.blend, metal.texture, metal.chrome), (LookBlend::Opaque, Some(Bitmap::Shiny), Some(ChromeMode::Metal)));
        assert_eq!((chrome.blend, chrome.texture, chrome.color), (LookBlend::Add, Some(Bitmap::Chrome), LookColor::BlendLight));
        let [u, v] = chrome.wave.unwrap();
        assert_eq!(u.at(5000.0), 0.0);
        assert!((v.at(5000.0) - 0.5).abs() < 1e-6);
        // `WorldTime*0.0001f` keeps growing (no `%`).
        assert!((v.at(25_000.0) - 2.5).abs() < 1e-4);
        // The wings: their own texture, opaque.
        let wings = resolve_all(&p, &obj(), &mesh_info(1, None));
        assert_eq!(wings.len(), 1);
        assert_eq!((wings[0].1.texture, wings[0].1.blend, wings[0].1.chrome), (None, LookBlend::Opaque, None));
    }

    #[test]
    fn default_body_ignores_hidden_mesh() {
        let p = passes(Item::new(SWORD, 0));
        assert_eq!(p, vec![Pass::body(TEXTURE).hidden(-1)]);
        // `RenderBody(RenderType, ...)` has no `HiddenMesh`: `o->HiddenMesh` does not hide anything.
        let o = ObjectDraw { hidden_mesh: 0, ..obj() };
        assert_eq!(resolve_all(&p, &o, &mesh_info(0, None)).len(), 1);
    }

    /// Dragon/Black Dragon armor +15 kind: a chrome `BITMAP_CHROME+1` body under the normal one; the
    /// Divine armor (+20) tints the chrome with the incoming light.
    #[test]
    fn chrome_armors() {
        let d = body_passes(&Item::new(ARMOR, 20), &INVENTORY);
        assert_eq!(d.passes.len(), 2);
        assert_eq!(d.passes[0].pass.texture, Some(CHROME_1));
        assert_eq!(d.passes[0].light.apply([1.0; 3], 0.0), [0.85, 0.85, 1.2]);
        assert_eq!(d.passes[1].light, LightOp::Keep);
        // ARMOR+23 takes the blue from the green channel (`Light[1]*1.f`).
        let d = body_passes(&Item::new(ARMOR, 23), &INVENTORY);
        assert_eq!(d.passes[0].light.apply([1.0, 0.5, 0.2], 0.0), [0.3, 0.4, 0.5]);
    }

    /// A ticket: the normal body plus a pulsing additive shine with its `_R` texture on mesh 0.
    #[test]
    fn free_ticket_shine() {
        let p = passes(Item::new(HELPER, 46));
        assert_eq!(p.len(), 2);
        let looks = resolve_all(&p, &obj(), &mesh_info(0, None));
        assert_eq!(looks.len(), 2);
        assert_eq!((looks[1].1.blend, looks[1].1.texture), (LookBlend::Add, Some(FREE_TICKET_R)));
        let g = p[1].glow.unwrap();
        assert!((g.at(0.0) - 0.36).abs() < 1e-6);
        assert!(g.animated());
    }

    /// The writes to `o` reach the glow passes; the inventory textures replace the skin.
    #[test]
    fn object_writes_and_hide_skin() {
        let d = body_passes(&Item::new(MACE, 10), &INVENTORY);
        assert_eq!((d.blend_mesh, d.hidden_mesh), (Some(2), Some(2)));
        let worn = BodyCtx { hide_skin: false, ..INVENTORY };
        let p = passes(Item::new(ARMOR, 40));
        assert_eq!(p.len(), 4);
        assert_eq!((p[0].mesh, p[0].texture), (Some(0), Some(INVEN_ARMOR[1])));
        let p = body_passes(&Item::new(ARMOR, 40), &worn).passes;
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].pass.mesh, None);
        // Wings of Despair-like checks of a white `BodyLight`.
        let dark = BodyCtx { white_light: false, ..INVENTORY };
        assert_eq!(body_passes(&Item::new(WING, 40), &dark).passes[0].pass.mesh, Some(0));
        assert!(body_passes(&Item::new(WING, 130), &dark).passes.is_empty());
    }

    #[test]
    fn light_waves() {
        let w = LightOp::RedWave(wave(0.001, 0.5, 0.5));
        assert_eq!(w.apply([0.8, 0.1, 0.1], 0.0), [0.4; 3]);
        assert!(w.relative() && w.animated());
        let w = LightOp::Wave { wave: Wave { freq_ms: 0.001, amp: 0.3, base: 0.0, abs: true }, add: [0.1; 3] };
        let l = w.apply([0.0; 3], -std::f32::consts::FRAC_PI_2 * 1000.0);
        assert!((l[0] - 0.4).abs() < 1e-5, "{l:?}");
        assert!(!w.relative());
    }
}
