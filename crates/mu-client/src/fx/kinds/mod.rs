//! El código de cada tipo de efecto, traducido de MuMain. Cada archivo es una familia y tiene
//! `pub fn register(r: &mut Registry)` que registra sus tipos.

pub mod particles_1;
pub mod particles_2;
pub mod particles_3;
pub mod particles_4;
pub mod particles_5;
pub mod particles_6;
pub mod joints_1;
pub mod joints_2;
pub mod joints_3;
pub mod effects_dk1;
pub mod effects_dk2;
pub mod skills_dk;
pub mod objects_1;
pub mod objects_2;
pub mod objects_login;
pub mod fire;

use super::core::Registry;

pub fn register_all(r: &mut Registry) {
    particles_1::register(r);
    particles_2::register(r);
    particles_3::register(r);
    particles_4::register(r);
    particles_5::register(r);
    particles_6::register(r);
    joints_1::register(r);
    joints_2::register(r);
    joints_3::register(r);
    effects_dk1::register(r);
    effects_dk2::register(r);
    skills_dk::register(r);
    objects_1::register(r);
    objects_2::register(r);
    objects_login::register(r);
    fire::register(r);
}
