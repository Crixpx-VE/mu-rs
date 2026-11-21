//! Reglas de dibujo de los objetos de Aida (`WD_33AIDA`, carpeta World34), Crywolf
//! (`WD_34CRYWOLF_1ST`, World35), Kanturu 1 (`WD_37KANTURU_1ST`, World38), Kanturu 2
//! (`WD_38KANTURU_2ND`, World39), Kanturu 3 (`WD_39KANTURU_3RD`, World40) y el mapa de los GM
//! (`WD_40AREA_FOR_GM`, World41).
//!
//! Sacado de `GMAida.cpp`, `GMCrywolf1st.cpp`, `GM_kanturu_1st.cpp`, `GM_Kanturu_2nd.cpp` y
//! `GM_Kanturu_3rd.cpp` (Main 5.2): `MoveObject` (velocidad, `HiddenMesh`, `BlendMeshLight`,
//! desplazamientos de UV), `RenderObjectMesh` (si devuelve `true` no hay `RenderBody` normal),
//! `RenderObjectVisual` (pasadas que se añaden después del `RenderBody` normal de `RenderObject`,
//! `ZzzObject.cpp:2612-2615` y `3504-3505`) y `RenderAfterObjectMesh` (los objetos con
//! `m_bRenderAfterCharacter` se dibujan solo ahí, después de los personajes:
//! `ZzzObject.cpp:3536-3552`, `3672`). Lo que crea partículas, sprites, luces del suelo y sonidos
//! está en `mu-client/src/fx/kinds/objects_kanturu.rs`.
//!
//! El mapa de los GM usa el código de Kanturu 1 (`M40GMArea::IsGmArea` en `GM_kanturu_1st.cpp:23-26`
//! y en las condiciones de las líneas 45, 132, 312 y 345).
//!
//! Los tipos sin modelo en `Data` (Crywolf 79/80, Kanturu 2 21/22/25/28/39/40/42/43/69/84, GM
//! 15/33/52/74) tampoco los dibuja el original (`AccessModel` no encuentra el archivo): se marcan
//! ocultos.
//!
//! TODO(original) — lo que el motor de pasadas aún no sabe hacer (no se inventa: se deja anotado):
//! - `o->Alpha` < 1 en las mallas iluminadas (Kanturu 1 96 con 0.5, `MoveKanturu1stObject` 105-107):
//!   se dibujan opacas / con prueba de alfa.
//! - `glAlphaFunc(GL_GREATER, 0)` de Kanturu 1 96 (`GM_kanturu_1st.cpp:231`): aquí 0.25.
//! - `b->BodyLight` que cambia con el tiempo en una pasada (Kanturu 3 21 y 43) y `BlendMeshLight`
//!   recortado a [0.1, 0.9] (Kanturu 2 10): ver cada tipo.
//! - Desplazamiento de UV con término constante (`-0.1 + ...`, Kanturu 3 43) y texturas
//!   `GL_LINEAR` + `GL_CLAMP_TO_EDGE` propias del mapa (`BITMAP_KANTURU3RD_OBJECT`).

use super::render::{Bitmap, Pass, Saw, flags};
use super::ModelRules;

/// Carpetas (`WorldActive + 1`).
const AIDA: u16 = 34;
const CRYWOLF: u16 = 35;
const KANTURU1: u16 = 38;
const KANTURU2: u16 = 39;
const KANTURU3: u16 = 40;
const GM_AREA: u16 = 41;

const T: u32 = flags::TEXTURE;
const TB: u32 = flags::TEXTURE | flags::BRIGHT;

/// `k · ((int)WorldTime % periodo)` (`k` < 0 = el `-(int)WorldTime % periodo · |k|` del original).
const fn saw(period: f32, k: f32) -> Saw {
    Saw { period, k }
}

/// Reglas de los mapas de este archivo (`None` = no es uno de ellos).
pub fn rules(world: u16, kind: i16) -> Option<ModelRules> {
    let r = ModelRules::default;
    let add = ModelRules::add;
    let hidden = ModelRules::hidden;
    Some(match (world, kind) {
        // 67 en todos estos mapas: la rama `default` de `CreateObject` (`ZzzObject.cpp:4895-4912`)
        // acaba en `TheMapProcess().CreateObject` (`w_MapProcess.cpp:206`), y
        // `GMUnitedMarketPlace::CreateObject` (sin mirar el mapa) le pone `HiddenMesh = -2` (y
        // `CreateOperate`). Ninguna función anterior de la cadena se queda con el 67 en estos mapas.
        (AIDA | CRYWOLF | KANTURU1 | KANTURU2 | KANTURU3 | GM_AREA, 67) => hidden(),

        // --- Aida (`GMAida.cpp`) ------------------------------------------------------------------
        // 25, 28 (`MoveAidaObject` 50-59): `BlendMeshTexCoordV -= 0.015` por tick = 0.000375 por ms
        // (diente de sierra de 8000 ms = 3 vueltas enteras: el salto no se ve). La malla
        // `waterfall_R` es su propia `BlendMesh` (`RenderBody`), así que se desplaza.
        (AIDA, 25 | 28) => r().tex_v(8000.0, -0.000375),
        // 41 (`RenderAidaObjectVisual` 154-168): después del `RenderBody` normal, la malla 0 otra vez
        // sumando como `StreamMesh` (color plano 0.52). El `Alpha = 0.5` de `MoveAidaObject` (74-78)
        // no cuenta en ninguna de las dos (la malla `_R` es `BlendMesh` y la `StreamMesh` usa
        // `glColor3fv`).
        (AIDA, 41) => r().passes(AIDA_41),
        // Puntos de agua (56-58), niebla (59, 62, 63), llama (67, ya oculto arriba) y fantasma (70)
        // (`MoveAidaObject` 79-88); 60 (mariposas) lo oculta `RenderAidaObjectVisual` (206).
        (AIDA, 56..=60 | 62 | 63 | 70) => hidden(),
        // 64 (`MoveAidaObject` 89-91): `Velocity = 0.05`.
        (AIDA, 64) => r().speed(0.05),
        // 65, 66 (226-242): la malla 0 otra vez como `StreamMesh` (color plano, opaca) con
        // `U = -(int)WT % 100000 · 0.00005`: tapa a la del `RenderBody`.
        (AIDA, 65 | 66) => r().passes(AIDA_65_66),
        // 77, 78 (312-321): igual con `U = (int)WT % 100000 · 0.0002`.
        (AIDA, 77 | 78) => r().passes(AIDA_77_78),

        // --- Crywolf (`GMCrywolf1st.cpp`) -----------------------------------------------------------
        // 36 (`RenderCryWolf1stObjectMesh` 442-455): `BodyLight` = 0.4 en paz y en guerra.
        (CRYWOLF, 36) => r().body_light(0.4),
        // `RenderCryWolf1stObjectMesh` devuelve `true` sin dibujar en 57, 71-74, 77, 78 y 81 (474-562;
        // 81 solo se dibuja en guerra) y `MoveCryWolf1stObject` oculta 82-84 (364-368).
        // TODO(original): 81 en guerra (`CRYWOLF_OCCUPATION_STATE_WAR`, 540-561): `Scale = 1.03`,
        // `BodyLight` según la vida de la estatua y la malla 0 con `RENDER_BRIGHT | RENDER_CHROME`
        // (`StreamMesh` 0) y `RENDER_BRIGHT | RENDER_CHROME2` (`StreamMesh` 1). Aquí, en paz.
        (CRYWOLF, 57 | 71..=74 | 77 | 78 | 81..=84) => hidden(),
        // Sin modelo en `Data/Object35`.
        (CRYWOLF, 79 | 80) => hidden(),

        // --- Kanturu 1 y mapa de los GM (`GM_kanturu_1st.cpp`) --------------------------------------
        // Sin modelo en `Data/Object41`.
        (GM_AREA, 15 | 33 | 52 | 74) => hidden(),
        // `MoveKanturu1stObject` (53-61, 76, 83, 109) y `RenderKanturu1stObjectVisual` (148).
        (KANTURU1 | GM_AREA, 37 | 59..=62 | 81..=83 | 97 | 107 | 108) => hidden(),
        // Velocidades (`MoveKanturu1stObject` 62-66, 85-86, 99-100).
        (KANTURU1 | GM_AREA, 44) => r().speed(0.02),
        // 46 (65-70): `BlendMeshLight = sin(WT·0.0015)·0.8 + 1` (la malla `_R` de `RenderBody`).
        (KANTURU1 | GM_AREA, 46) => r().speed(0.01).pulse(0.0015, 0.8, 1.0),
        (KANTURU1 | GM_AREA, 70 | 90) => r().speed(0.04),
        // 76: `RenderKanturu1stObjectMesh` (329-336) lo deja para después de los personajes y
        // `RenderKanturu1stAfterObjectMesh` (350-361) solo dibuja la malla 0 sumando como `StreamMesh`
        // (color plano 0.52) con `U = -(int)WT % 100000 · 0.00001`. `Alpha = 0.5` (92-94) no cuenta.
        (KANTURU1 | GM_AREA, 76) => r().passes(KANTURU1_76),
        // 77 (95-98): `BlendMeshTexCoordV = -(int)WT % 10000 · 0.0002`; `RenderBody` normal después de
        // los personajes (394-400).
        (KANTURU1 | GM_AREA, 77) => r().tex_v(10000.0, -0.0002),
        // 85 (`RenderKanturu1stObjectVisual` 205-215): la malla 3 otra vez sumando y con cromado
        // restando, después del `RenderBody` normal.
        (KANTURU1 | GM_AREA, 85) => r().passes(KANTURU1_85),
        // 95 (363-383): solo la malla 0 sumando y con `RENDER_CHROME7 | RENDER_DARK` (que suma: las
        // ramas de `ZzzBMD.cpp:1110-1121` van antes que la de `RENDER_DARK`).
        (KANTURU1 | GM_AREA, 95) => r().passes(KANTURU1_95),
        // 96 (229-238): la malla 0 otra vez como `StreamMesh` con `V = -(int)WT % 20000 · 0.00005`,
        // con alfa 1 (ver TODO del principio: `Alpha = 0.5` y `glAlphaFunc(GL_GREATER, 0)`).
        (KANTURU1 | GM_AREA, 96) => r().passes(KANTURU1_96),
        // 102 (120-122): `BlendMeshLight = sin(WT·0.001) + 1`.
        (KANTURU1 | GM_AREA, 102) => r().pulse(0.001, 1.0, 1.0),
        // 103 (316-327): `RenderBody` sin la malla 1 y la malla 1 como `BlendMesh` con
        // `fLumi = (sin(WT·0.002) + 1)·0.5`: lo mismo que `BlendMesh = 1` con ese brillo.
        (KANTURU1 | GM_AREA, 103) => add(1).pulse(0.002, 0.5, 0.5),
        // 105 (385-392): `RenderBody` y la malla 1 con cromado sumando, después de los personajes.
        (KANTURU1 | GM_AREA, 105) => r().passes(KANTURU1_105),
        // 91, 92 (394-400): `RenderBody` normal después de los personajes.

        // --- Kanturu 2 (`GM_Kanturu_2nd.cpp`) -------------------------------------------------------
        // Sin modelo en `Data/Object39` (42 tendría `U = V = -(int)WT % 10000 · 0.0002`, 245-250).
        (KANTURU2, 21 | 22 | 25 | 28 | 39 | 40 | 42 | 43 | 69 | 84) => hidden(),
        // 1, 2 (`Render_Kanturu2nd_ObjectMesh` 537-556): como Kanturu 1 103.
        (KANTURU2, 1 | 2) => add(1).pulse(0.002, 0.5, 0.5),
        // 8 (`Render_Kanturu2nd_AfterObjectMesh` 640-644): `RenderBody` y la malla 1 con cromado sumando.
        (KANTURU2, 8) => r().passes(KANTURU2_8),
        // 10 (`Move_Kanturu2nd_Object` 225-239, `AfterObjectMesh` 645-650): `Velocity = 0.04`,
        // `RenderBody` sin la malla 2 y la 2 con cromado sumando.
        // TODO(original): `BlendMeshLight = sin(WT·0.0015) + 1` recortado a [0.1, 0.9]; el motor no
        // recorta el pulso, así que va el seno del mismo periodo entre esos dos valores.
        (KANTURU2, 10) => r().speed(0.04).pulse(0.0015, 0.4, 0.5).passes(KANTURU2_10),
        // 12 (557-568): `RenderBody` sin la malla 0 (`StreamMesh` 1 con `V = -(int)WT % 2000 · 0.0005`),
        // la 0 como `BlendMesh` con `U = V = (int)WT % 2000 · 0.0005` y luego cromada restando.
        (KANTURU2, 12) => r().passes(KANTURU2_12),
        // 13, 14 (569-586): `StreamMesh` 1 con `V = -(int)WT % 2000 · 0.0005`.
        (KANTURU2, 13 | 14) => r().passes(KANTURU2_13_14),
        // 15, 16 (587-596): igual con `StreamMesh` 3.
        (KANTURU2, 15 | 16) => r().passes(KANTURU2_15_16),
        // 27 (597-607): `RenderBody` sin la malla 2 (`StreamMesh` 4 con `V`) y la 2 sumando.
        (KANTURU2, 27) => r().passes(KANTURU2_27),
        // 31 (`AfterObjectMesh` 634-639): `RenderBody` sin la malla 0 y la 0 sumando.
        (KANTURU2, 31) => r().passes(KANTURU2_31),
        // 35, 36 (651-656): `RenderBody` sin la malla 1 y la 1 sumando.
        (KANTURU2, 35 | 36) => r().passes(KANTURU2_35_36),
        // 38 (`Move_Kanturu2nd_Object` 240-244): `BlendMeshLight = sin(WT·0.001) + 1`.
        (KANTURU2, 38) => r().pulse(0.001, 1.0, 1.0),
        // 44 (251-255): `Velocity = 0.02`.
        (KANTURU2, 44) => r().speed(0.02),
        // Niebla, agua, rayos (256-272).
        (KANTURU2, 45..=56 | 65) => hidden(),
        // 76 (657-677): `BodyLight` 0.4, solo la malla 0 sumando y con `RENDER_CHROME7 | RENDER_DARK`.
        (KANTURU2, 76) => r().passes(KANTURU2_76),
        // 33, 59, 80 (678-681): `RenderBody` normal después de los personajes.

        // --- Kanturu 3 (`GM_Kanturu_3rd.cpp`) -------------------------------------------------------
        // TODO(original): 0 es Maya. `CreateKanturu3rdObject` (101-103) la baja 2000 (de -749.5 a
        // -2749.5) y `RenderKanturu3rdObjectVisual` solo la sube cuando aparece en la batalla
        // (`GetMayaAppear`, 219-231) o la deja en -749.5 en los estados de la batalla; sin batalla
        // (`KANTURU_STATE_NONE`) se queda 2000 bajo su sitio. El motor no mueve objetos, así que no se
        // dibuja (ni su humo del hueso 34, 234-238, ni `MayaAction`).
        (KANTURU3, 0) => hidden(),
        // `CreateKanturu3rdObject` (104-114), `MoveKanturu3rdObject` (45, 48: el 45 se oculta en el
        // primer tick con `rand() % 3 == 0`) y `RenderKanturu3rdObjectVisual` (1, 11, 46, 49, 50).
        (KANTURU3, 1 | 11 | 32 | 45..=54 | 57 | 58 | 70) => hidden(),
        // 2 (`RenderKanturu3rdObjectMesh` 473-479): cromado opaco con `BITMAP_CHROME2` y luego sumando
        // con `BodyLight` 0.3.
        (KANTURU3, 2) => r().passes(KANTURU3_2),
        // 8, 10 (`RenderKanturu3rdAfterObjectMesh` 575-596): las mallas 1-5 (1-7) y la 0 sumando.
        (KANTURU3, 8) => r().passes(KANTURU3_8),
        (KANTURU3, 10) => r().passes(KANTURU3_10),
        // 15-17 (480-487): la malla 1 como `BlendMesh` con `V = -(int)WT % 10000 · 0.0005`.
        (KANTURU3, 15..=17) => r().passes(KANTURU3_15_17),
        // 19, 20 (597-614): `RenderBody` sin la malla 7 (6) y esa malla (`hanmcrystal2_R`) como
        // `BlendMesh` con `V = -(int)WT % 10000 · 0.0001`: lo mismo que el `RenderBody` normal (la
        // malla `_R` ya es su `BlendMesh`) con ese desplazamiento.
        (KANTURU3, 19 | 20) => r().tex_v(10000.0, -0.0001),
        // 21 (615-622): solo la malla 0 sumando con `BodyLight = sin(WT·0.002)·0.2 + 0.5`.
        // TODO(original): la luz animada de una pasada; va la del centro del seno (0.5).
        (KANTURU3, 21) => r().passes(KANTURU3_21),
        // 24, 25 (623-631): las mallas 0, 2, 3 y la 1 sumando.
        (KANTURU3, 24 | 25) => r().passes(KANTURU3_24_25),
        // 33 (488-493): la malla 1 como `BlendMesh` con `V = -(int)WT % 2000 · 0.0005`.
        (KANTURU3, 33) => r().passes(KANTURU3_33),
        // 40-42 (494-503): malla 1 cromada opaca (`BodyLight` (0.6, 0.7, 1)) y la 0 como `BlendMesh`
        // con `BodyLight` (0.7, 0.6, 0.7) y `V = -(int)WT % 200000 · 0.00001`.
        (KANTURU3, 40..=42) => r().passes(KANTURU3_40_42),
        // 43 (504-518): ver `KANTURU3_43`.
        (KANTURU3, 43) => r().passes(KANTURU3_43),
        // 73 (632-640): sin batalla ganada (`KanturuSuccessMap`), la malla 0 dos veces sumando con
        // `V` y luego `U = (int)WT % 10000 · 0.0002`. TODO(original): con la torre conquistada no se
        // dibuja (estado del servidor).
        (KANTURU3, 73) => r().passes(KANTURU3_73),
        // 74 (313-323): `RenderBody` normal (su malla `test12_H` no se dibuja).

        _ => return None,
    })
}

const AIDA_41: &[Pass] = &[Pass::body(T), Pass::mesh(0, TB).light([0.52; 3]).stream(0)];
const AIDA_65_66: &[Pass] = &[Pass::body(T), Pass::mesh(0, T).stream(0).tex_coord(saw(100000.0, -0.00005), Saw::ZERO)];
const AIDA_77_78: &[Pass] = &[Pass::body(T), Pass::mesh(0, T).stream(0).tex_coord(saw(100000.0, 0.0002), Saw::ZERO)];

const KANTURU1_76: &[Pass] = &[Pass::mesh(0, TB).light([0.52; 3]).stream(0).tex_coord(saw(100000.0, -0.00001), Saw::ZERO)];
const KANTURU1_85: &[Pass] = &[
    Pass::body(T),
    Pass::mesh(3, TB),
    Pass::mesh(3, flags::CHROME | flags::DARK).texture(Bitmap::Chrome),
];
const KANTURU1_95: &[Pass] = &[Pass::mesh(0, TB), Pass::mesh(0, flags::CHROME7 | flags::DARK).texture(Bitmap::Chrome)];
const KANTURU1_96: &[Pass] = &[Pass::body(T), Pass::mesh(0, T).stream(0).tex_coord(Saw::ZERO, saw(20000.0, -0.00005))];
const KANTURU1_105: &[Pass] = &[Pass::body(T), Pass::mesh(1, flags::CHROME | flags::BRIGHT).texture(Bitmap::Chrome)];

const KANTURU2_8: &[Pass] = &[Pass::body(T), Pass::mesh(1, flags::CHROME | flags::BRIGHT).texture(Bitmap::Chrome)];
/// `RenderBody(HiddenMesh 2)`: la malla 1 (`songk21crystal02_R`) es su propia `BlendMesh`.
const KANTURU2_10: &[Pass] = &[
    Pass::mesh(0, T),
    Pass::mesh(1, T).blend_mesh(1),
    Pass::mesh(3, T),
    Pass::mesh(2, flags::CHROME | flags::BRIGHT).texture(Bitmap::Chrome),
];
/// `V = -(int)WT % 2000 · 0.0005` de la `StreamMesh`.
const K2_STREAM_V: Saw = saw(2000.0, -0.0005);
const KANTURU2_12: &[Pass] = &[
    Pass::mesh(1, T).stream(1).tex_coord(Saw::ZERO, K2_STREAM_V),
    Pass::mesh(2, T).stream(1),
    Pass::mesh(3, T).stream(1),
    Pass::mesh(0, T).stream(-1).blend_mesh(0).tex_coord(saw(2000.0, 0.0005), saw(2000.0, 0.0005)),
    Pass::mesh(0, flags::CHROME | flags::DARK).stream(-1).texture(Bitmap::Chrome),
];
const KANTURU2_13_14: &[Pass] = &[Pass::body(T).stream(1).tex_coord(Saw::ZERO, K2_STREAM_V)];
const KANTURU2_15_16: &[Pass] = &[Pass::body(T).stream(3).tex_coord(Saw::ZERO, K2_STREAM_V)];
const KANTURU2_27: &[Pass] = &[
    Pass::mesh(0, T).stream(4).tex_coord(Saw::ZERO, K2_STREAM_V),
    Pass::mesh(1, T).stream(4).tex_coord(Saw::ZERO, K2_STREAM_V),
    Pass::mesh(3, T).stream(4).tex_coord(Saw::ZERO, K2_STREAM_V),
    Pass::mesh(4, T).stream(4).tex_coord(Saw::ZERO, K2_STREAM_V),
    Pass::mesh(2, TB).stream(-1),
];
const KANTURU2_31: &[Pass] = &[
    Pass::mesh(1, T),
    Pass::mesh(2, T),
    Pass::mesh(3, T),
    Pass::mesh(4, T),
    Pass::mesh(5, T),
    Pass::mesh(0, TB),
];
const KANTURU2_35_36: &[Pass] = &[Pass::mesh(0, T), Pass::mesh(2, T), Pass::mesh(1, TB)];
const KANTURU2_76: &[Pass] = &[
    Pass::mesh(0, TB).light([0.4; 3]),
    Pass::mesh(0, flags::CHROME7 | flags::DARK).texture(Bitmap::Chrome).light([0.4; 3]),
];

const KANTURU3_2: &[Pass] = &[Pass::mesh(0, flags::CHROME).texture(Bitmap::Chrome2), Pass::mesh(0, TB).light([0.3; 3])];
const KANTURU3_8: &[Pass] = &[
    Pass::mesh(1, T),
    Pass::mesh(2, T),
    Pass::mesh(3, T),
    Pass::mesh(4, T),
    Pass::mesh(5, T),
    Pass::mesh(0, TB),
];
const KANTURU3_10: &[Pass] = &[
    Pass::mesh(1, T),
    Pass::mesh(2, T),
    Pass::mesh(3, T),
    Pass::mesh(4, T),
    Pass::mesh(5, T),
    Pass::mesh(6, T),
    Pass::mesh(7, T),
    Pass::mesh(0, TB),
];
const KANTURU3_15_17: &[Pass] = &[Pass::mesh(0, T), Pass::mesh(1, T).blend_mesh(1).tex_coord(Saw::ZERO, saw(10000.0, -0.0005))];
const KANTURU3_21: &[Pass] = &[Pass::mesh(0, TB).light([0.5; 3])];
const KANTURU3_24_25: &[Pass] = &[Pass::mesh(0, T), Pass::mesh(2, T), Pass::mesh(3, T), Pass::mesh(1, TB)];
const KANTURU3_33: &[Pass] = &[Pass::mesh(0, T), Pass::mesh(1, TB).blend_mesh(1).tex_coord(Saw::ZERO, saw(2000.0, -0.0005))];
const KANTURU3_40_42: &[Pass] = &[
    Pass::mesh(1, flags::CHROME).light([0.6, 0.7, 1.0]),
    Pass::mesh(0, TB).light([0.7, 0.6, 0.7]).blend_mesh(0).tex_coord(Saw::ZERO, saw(200000.0, -0.00001)),
];
/// Kanturu 3 43 (`RenderKanturu3rdObjectMesh` 504-518): la malla 2 normal; la 0 como `BlendMesh` con
/// `BodyLight` 1 y la textura `BITMAP_KANTURU3RD_OBJECT` (`Object40\Mtowereffe.JPG`,
/// `MapManager.cpp:472`) con `V = -0.1 + (int)WT % 5000 · 0.0001`; la 0 otra vez normal y la 1 con
/// `BodyLight = sin(WT·0.0015)·0.2 + 0.4`.
/// TODO(original): el `-0.1` del desplazamiento, el `GL_CLAMP_TO_EDGE` de esa textura (aquí se repite)
/// y el seno de la luz de la malla 1 (va el centro, 0.4).
const KANTURU3_43: &[Pass] = &[
    Pass::mesh(2, T),
    Pass::mesh(0, TB)
        .light([1.0; 3])
        .blend_mesh(0)
        .texture(Bitmap::File { path: "Object40/Mtowereffe.jpg", nearest_clamp: false })
        .tex_coord(Saw::ZERO, saw(5000.0, 0.0001)),
    Pass::mesh(0, T),
    Pass::mesh(1, T).light([0.4; 3]),
];
const KANTURU3_73: &[Pass] = &[
    Pass::mesh(0, TB).blend_mesh(0).tex_coord(Saw::ZERO, saw(10000.0, 0.0002)),
    Pass::mesh(0, TB).blend_mesh(0).tex_coord(saw(10000.0, 0.0002), Saw::ZERO),
];

#[cfg(test)]
mod tests {
    use super::super::render::{LookBlend, LookColor, MeshInfo, ObjectDraw, resolve_all};
    use super::*;

    fn draw(rules: &ModelRules) -> ObjectDraw {
        ObjectDraw {
            blend_mesh: rules.additive_mesh.map_or(-1, |m| m as i32),
            stream_mesh: rules.stream_mesh.map_or(-1, |m| m as i32),
            tex_coord: rules.tex_coord,
            hidden_mesh: -1,
        }
    }

    fn mesh(i: usize) -> MeshInfo {
        MeshInfo { index: i, texture_index: i as i32, script: None, has_alpha: false }
    }

    #[test]
    fn solo_estos_mapas() {
        assert!(rules(1, 25).is_none());
        assert!(rules(AIDA, 2).is_none());
        assert!(rules(AIDA, 59).unwrap().hidden);
        // El mapa de los GM usa el código de Kanturu 1.
        assert_eq!(rules(GM_AREA, 77), rules(KANTURU1, 77));
        assert!(rules(GM_AREA, 74).unwrap().hidden);
        assert!(rules(KANTURU1, 74).is_none());
    }

    #[test]
    fn cascada_de_aida_al_ritmo_del_original() {
        // -0.015 por tick (25 por segundo) = -0.375 por segundo.
        let v = rules(AIDA, 25).unwrap().tex_coord[1];
        assert!((v.at(1000.0) + 0.375).abs() < 1e-5);
        // Al dar la vuelta salta un número entero de texturas.
        assert!((v.k * v.period).fract().abs() < 1e-6);
    }

    #[test]
    fn la_pasada_de_stream_tapa_a_la_normal() {
        // Aida 65: la malla 0 solo queda con la segunda pasada (opaca, plana, desplazándose).
        let rules = rules(AIDA, 65).unwrap();
        let looks = resolve_all(rules.passes, &draw(&rules), &mesh(0));
        assert_eq!(looks.len(), 1);
        assert_eq!(looks[0].0, 1);
        assert_eq!((looks[0].1.blend, looks[0].1.color), (LookBlend::Opaque, LookColor::Flat));
        assert!(looks[0].1.wave.is_some());
    }

    #[test]
    fn cromado7_resta_que_suma() {
        // `RENDER_CHROME7 | RENDER_DARK`: la rama de CHROME7 (suma) va antes que la de DARK.
        let rules = rules(KANTURU1, 95).unwrap();
        let looks = resolve_all(rules.passes, &draw(&rules), &mesh(0));
        assert_eq!(looks.iter().map(|(_, l)| l.blend).collect::<Vec<_>>(), vec![LookBlend::Add, LookBlend::Add]);
        // Kanturu 1 85: el cromado de la malla 3 con `RENDER_DARK` sí resta.
        let rules = super::rules(KANTURU1, 85).unwrap();
        let looks = resolve_all(rules.passes, &draw(&rules), &mesh(3));
        assert_eq!(looks.last().unwrap().1.blend, LookBlend::Subtract);
    }

    #[test]
    fn kanturu2_12_sin_la_malla_0_normal() {
        let rules = rules(KANTURU2, 12).unwrap();
        let looks = resolve_all(rules.passes, &draw(&rules), &mesh(0));
        // Solo la `BlendMesh` (suma) y el cromado (resta).
        assert_eq!(looks.iter().map(|(_, l)| l.blend).collect::<Vec<_>>(), vec![LookBlend::Add, LookBlend::Subtract]);
        assert_eq!(looks[0].1.color, LookColor::BlendLight);
        // La malla 1 es la `StreamMesh`: plana y desplazándose.
        let looks = resolve_all(rules.passes, &draw(&rules), &mesh(1));
        assert_eq!(looks.len(), 1);
        assert_eq!(looks[0].1.color, LookColor::Flat);
        assert!(looks[0].1.wave.is_some());
    }
}
